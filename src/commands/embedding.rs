//! GAP-005 (v1.0.82): `embedding` subcommand — health and retry of the
//! pending-embeddings queue that buffers memories whose embedding step failed.
//!
//! ## Subcommands
//! - `embedding status` — counts by status
//! - `embedding list [--status <STATUS>]` — list pending entries
//! - `embedding retry <pending_id>` — re-run embedding for one entry
//! - `embedding abandon <pending_id>` — mark as abandoned
//!
//! The pending_embeddings table captures every `embed_with_fallback` failure
//! with `exit_code`, `stderr_tail`, and `backend_chain` for diagnostics. This
//! subcommand makes that state observable and recoverable.

use clap::{Args, Subcommand};
use serde::Serialize;

use crate::cli::LlmBackendChoice;
use crate::errors::AppError;
use crate::output::emit_json_compact;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::pending_embeddings::{self, PendingEmbedding, PendingEmbeddingStatus};

#[derive(Debug, Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Show queue health and counts per status\n  \
    sqlite-graphrag embedding status --json\n\n  \
    # List all pending embeddings waiting for retry\n  \
    sqlite-graphrag embedding list --status pending --json\n\n  \
    # Mark pending_id 7 as abandoned (will not be retried automatically)\n  \
    sqlite-graphrag embedding abandon 7 --yes\n\n  \
    # Note: `embedding retry` requires re-running an LLM subprocess; for full\n  \
    # retry of every pending entry use `enrich --operation re-embed --pending-only`")]
pub struct EmbeddingArgs {
    #[command(subcommand)]
    pub cmd: EmbeddingCmd,
}

#[derive(Debug, Subcommand)]
pub enum EmbeddingCmd {
    /// Show queue health (counts by status).
    Status(EmbeddingStatusArgs),
    /// List pending embeddings filtered by status.
    List(EmbeddingListArgs),
    /// Mark one entry as abandoned.
    Abandon(EmbeddingAbandonArgs),
}

#[derive(Debug, Args)]
pub struct EmbeddingStatusArgs {
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
    /// JSON output (always on; accepted for CLI consistency).
    #[arg(long, hide = true)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct EmbeddingListArgs {
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
    /// Filter by status: pending | in_progress | done | abandoned. Default: pending.
    #[arg(long, value_enum, default_value_t = EmbeddingStatusFilter::Pending)]
    pub status: EmbeddingStatusFilter,
    /// Maximum number of entries to return. Default: 100.
    #[arg(long, default_value_t = 100)]
    pub limit: usize,
    /// JSON output (always on; accepted for CLI consistency).
    #[arg(long, hide = true)]
    pub json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[value(rename_all = "snake_case")]
pub enum EmbeddingStatusFilter {
    Pending,
    InProgress,
    Done,
    Abandoned,
}

impl From<EmbeddingStatusFilter> for PendingEmbeddingStatus {
    fn from(value: EmbeddingStatusFilter) -> Self {
        match value {
            EmbeddingStatusFilter::Pending => Self::Pending,
            EmbeddingStatusFilter::InProgress => Self::InProgress,
            EmbeddingStatusFilter::Done => Self::Done,
            EmbeddingStatusFilter::Abandoned => Self::Abandoned,
        }
    }
}

#[derive(Debug, Args)]
pub struct EmbeddingAbandonArgs {
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
    /// Pending id to abandon.
    pub pending_id: i64,
    /// Skip the interactive confirmation prompt.
    #[arg(long)]
    pub yes: bool,
    /// JSON output (always on; accepted for CLI consistency).
    #[arg(long, hide = true)]
    pub json: bool,
}

#[derive(Serialize)]
struct EmbeddingStatusOutput {
    action: &'static str,
    /// v1.0.84 (ADR-0042): discriminator of the LLM backend that would be
    /// invoked to process live embeddings. `"claude" | "codex"
    /// | "none" | "auto"`. `"auto"` indicates the caller requested Auto and
    /// the codex→claude→none chain would be iterated at runtime.
    backend_invoked: &'static str,
    counts: EmbeddingStatusCounts,
    /// GAP-SG-41: real vector coverage in the persisted tables. The `counts`
    /// above only reflect the async retry queue (empty on the synchronous REST
    /// path), so `coverage` reports the actual rows in `memory_embeddings`,
    /// `entity_embeddings` and `chunk_embeddings` versus their source rows.
    coverage: EmbeddingCoverage,
    elapsed_ms: u64,
}

#[derive(Serialize, Default)]
struct EmbeddingStatusCounts {
    pending: usize,
    in_progress: usize,
    done: usize,
    abandoned: usize,
}

/// GAP-SG-41: actual persisted-vector coverage. Each `*_with_vec` field counts
/// the rows that have an embedding; the `*_total` field counts the source rows
/// (active memories / entities / chunks). When totals are non-zero the operator
/// can audit coverage directly instead of inferring it from `hybrid-search`.
#[derive(Serialize, Default)]
struct EmbeddingCoverage {
    memories_total: i64,
    memories_with_vec: i64,
    /// v1.1.1 (P6b): active memories WITHOUT a row in `memory_embeddings`
    /// (LEFT JOIN, so orphaned vectors never mask a gap). Additive field —
    /// the pre-existing totals keep their meaning.
    memories_missing: i64,
    entities_total: i64,
    entities_with_vec: i64,
    /// v1.1.1 (P6b): entities without a row in `entity_embeddings`.
    entities_missing: i64,
    chunks_total: i64,
    chunks_with_vec: i64,
    /// v1.1.1 (P6b): memory_chunks rows without a row in `chunk_embeddings`.
    chunks_missing: i64,
}

/// Counts a table, returning 0 when the table is absent (legacy DB) instead of
/// failing the whole status report.
fn count_table(conn: &rusqlite::Connection, sql: &str) -> i64 {
    match conn.query_row(sql, [], |r| r.get::<_, i64>(0)) {
        Ok(n) => n,
        Err(rusqlite::Error::SqliteFailure(_, Some(msg))) if msg.contains("no such table") => 0,
        Err(e) => {
            tracing::warn!(target: "embedding", error = %e, sql, "coverage count failed");
            0
        }
    }
}

/// v1.1.1 (P6b): counts source rows without a vector via LEFT JOIN. When the
/// embedding table does not exist (legacy DB) EVERY source row is missing, so
/// the fallback is `total_when_absent` — never a silent 0 that would report
/// full coverage on a table that is not there.
fn count_missing(conn: &rusqlite::Connection, sql: &str, total_when_absent: i64) -> i64 {
    match conn.query_row(sql, [], |r| r.get::<_, i64>(0)) {
        Ok(n) => n,
        Err(rusqlite::Error::SqliteFailure(_, Some(msg))) if msg.contains("no such table") => {
            total_when_absent
        }
        Err(e) => {
            tracing::warn!(target: "embedding", error = %e, sql, "coverage missing-count failed");
            0
        }
    }
}

#[derive(Serialize)]
struct EmbeddingListEntry {
    pending_id: i64,
    memory_id: i64,
    name: String,
    namespace: String,
    backend_chain: String,
    last_error: Option<String>,
    last_exit_code: Option<i32>,
    last_stderr_tail: Option<String>,
    attempt_count: i32,
    status: String,
    updated_at: i64,
}

impl From<&PendingEmbedding> for EmbeddingListEntry {
    fn from(p: &PendingEmbedding) -> Self {
        Self {
            pending_id: p.pending_id,
            memory_id: p.memory_id,
            name: p.name.clone(),
            namespace: p.namespace.clone(),
            backend_chain: p.backend_chain.clone(),
            last_error: p.last_error.clone(),
            last_exit_code: p.last_exit_code,
            last_stderr_tail: p.last_stderr_tail.clone(),
            attempt_count: p.attempt_count,
            status: p.status.as_str().to_string(),
            updated_at: p.updated_at,
        }
    }
}

#[derive(Serialize)]
struct EmbeddingListOutput {
    action: &'static str,
    filter_status: String,
    count: usize,
    entries: Vec<EmbeddingListEntry>,
    elapsed_ms: u64,
}

#[derive(Serialize)]
struct EmbeddingAbandonOutput {
    action: &'static str,
    pending_id: i64,
    status: &'static str,
    elapsed_ms: u64,
    yes: bool,
}

pub fn run(args: EmbeddingArgs, llm_backend: LlmBackendChoice) -> Result<(), AppError> {
    match args.cmd {
        EmbeddingCmd::Status(a) => run_status(a, llm_backend),
        EmbeddingCmd::List(a) => run_list(a),
        EmbeddingCmd::Abandon(a) => run_abandon(a),
    }
}

fn open_conn(db: Option<&str>) -> Result<(AppPaths, rusqlite::Connection), AppError> {
    let paths = AppPaths::resolve(db)?;
    let conn = open_rw(&paths.db)?;
    Ok((paths, conn))
}

fn run_status(args: EmbeddingStatusArgs, llm_backend: LlmBackendChoice) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let (_paths, conn) = open_conn(args.db.as_deref())?;

    let counts = EmbeddingStatusCounts {
        pending: pending_embeddings::list_by_status(
            &conn,
            PendingEmbeddingStatus::Pending,
            100_000,
        )?
        .len(),
        in_progress: pending_embeddings::list_by_status(
            &conn,
            PendingEmbeddingStatus::InProgress,
            100_000,
        )?
        .len(),
        done: pending_embeddings::list_by_status(&conn, PendingEmbeddingStatus::Done, 100_000)?
            .len(),
        abandoned: pending_embeddings::list_by_status(
            &conn,
            PendingEmbeddingStatus::Abandoned,
            100_000,
        )?
        .len(),
    };

    let backend_invoked: &'static str = match llm_backend {
        LlmBackendChoice::Claude => "claude",
        LlmBackendChoice::Codex => "codex",
        LlmBackendChoice::Opencode => "opencode",
        LlmBackendChoice::None => "none",
        LlmBackendChoice::OpenRouter => "openrouter",
        LlmBackendChoice::Auto => "auto",
    };

    // GAP-SG-41: query the actual vector tables so coverage is observable even
    // when the async queue is empty (the synchronous OpenRouter REST path never
    // populates `pending_embeddings`).
    let memories_total = count_table(
        &conn,
        "SELECT COUNT(*) FROM memories WHERE deleted_at IS NULL",
    );
    let entities_total = count_table(&conn, "SELECT COUNT(*) FROM entities");
    let chunks_total = count_table(&conn, "SELECT COUNT(*) FROM memory_chunks");
    let coverage = EmbeddingCoverage {
        memories_total,
        memories_with_vec: count_table(&conn, "SELECT COUNT(*) FROM memory_embeddings"),
        // v1.1.1 (P6b): missing counts via LEFT JOIN so orphaned vector rows
        // never inflate coverage; absent embedding table means ALL missing.
        memories_missing: count_missing(
            &conn,
            "SELECT COUNT(*) FROM memories m \
             LEFT JOIN memory_embeddings me ON me.memory_id = m.id \
             WHERE me.memory_id IS NULL AND m.deleted_at IS NULL",
            memories_total,
        ),
        entities_total,
        entities_with_vec: count_table(&conn, "SELECT COUNT(*) FROM entity_embeddings"),
        entities_missing: count_missing(
            &conn,
            "SELECT COUNT(*) FROM entities e \
             LEFT JOIN entity_embeddings ee ON ee.entity_id = e.id \
             WHERE ee.entity_id IS NULL",
            entities_total,
        ),
        chunks_total,
        chunks_with_vec: count_table(&conn, "SELECT COUNT(*) FROM chunk_embeddings"),
        chunks_missing: count_missing(
            &conn,
            "SELECT COUNT(*) FROM memory_chunks c \
             LEFT JOIN chunk_embeddings ce ON ce.chunk_id = c.id \
             WHERE ce.chunk_id IS NULL",
            chunks_total,
        ),
    };

    let output = EmbeddingStatusOutput {
        action: "embedding_status",
        backend_invoked,
        counts,
        coverage,
        elapsed_ms: start.elapsed().as_millis() as u64,
    };
    emit_json_compact(&output)
}

fn run_list(args: EmbeddingListArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let (_paths, conn) = open_conn(args.db.as_deref())?;
    let status: PendingEmbeddingStatus = args.status.into();
    let rows = pending_embeddings::list_by_status(&conn, status, args.limit)?;
    let count = rows.len();
    let entries: Vec<EmbeddingListEntry> = rows.iter().map(EmbeddingListEntry::from).collect();
    let output = EmbeddingListOutput {
        action: "embedding_list",
        filter_status: status.as_str().to_string(),
        count,
        entries,
        elapsed_ms: start.elapsed().as_millis() as u64,
    };
    emit_json_compact(&output)
}

fn run_abandon(args: EmbeddingAbandonArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let (_paths, conn) = open_conn(args.db.as_deref())?;
    pending_embeddings::abandon(&conn, args.pending_id)?;
    let output = EmbeddingAbandonOutput {
        action: "embedding_abandon",
        pending_id: args.pending_id,
        status: PendingEmbeddingStatus::Abandoned.as_str(),
        elapsed_ms: start.elapsed().as_millis() as u64,
        yes: args.yes,
    };
    emit_json_compact(&output)
}

#[cfg(test)]
mod tests {
    use super::*;

    // GAP-SG-41: the status output exposes real vector coverage, not only the
    // async queue counts.
    #[test]
    fn embedding_status_output_includes_coverage() {
        let output = EmbeddingStatusOutput {
            action: "embedding_status",
            backend_invoked: "openrouter",
            counts: EmbeddingStatusCounts::default(),
            coverage: EmbeddingCoverage {
                memories_total: 10,
                memories_with_vec: 9,
                memories_missing: 1,
                entities_total: 4,
                entities_with_vec: 4,
                entities_missing: 0,
                chunks_total: 7,
                chunks_with_vec: 7,
                chunks_missing: 0,
            },
            elapsed_ms: 1,
        };
        let json = serde_json::to_value(&output).expect("serialize");
        assert_eq!(json["coverage"]["memories_total"], 10);
        assert_eq!(json["coverage"]["memories_with_vec"], 9);
        assert_eq!(json["coverage"]["entities_with_vec"], 4);
        assert_eq!(json["coverage"]["chunks_with_vec"], 7);
        // v1.1.1 (P6b): the missing counters serialize alongside the totals.
        assert_eq!(json["coverage"]["memories_missing"], 1);
        assert_eq!(json["coverage"]["entities_missing"], 0);
        assert_eq!(json["coverage"]["chunks_missing"], 0);
    }

    // v1.1.1 (P6b): the LEFT JOIN counts real gaps and the absent-table
    // fallback reports EVERYTHING missing instead of a silent 0.
    #[test]
    fn count_missing_counts_gaps_and_falls_back_when_table_absent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE entities (id INTEGER PRIMARY KEY, name TEXT);
            CREATE TABLE entity_embeddings (
                entity_id INTEGER PRIMARY KEY,
                embedding BLOB NOT NULL
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO entities (id, name) VALUES (1, 'a'), (2, 'b'), (3, 'c')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO entity_embeddings (entity_id, embedding) VALUES (1, X'00')",
            [],
        )
        .unwrap();

        let missing = count_missing(
            &conn,
            "SELECT COUNT(*) FROM entities e \
             LEFT JOIN entity_embeddings ee ON ee.entity_id = e.id \
             WHERE ee.entity_id IS NULL",
            3,
        );
        assert_eq!(missing, 2, "2 of 3 entities lack a vector row");

        // Absent embedding table: everything counts as missing.
        let missing_absent = count_missing(
            &conn,
            "SELECT COUNT(*) FROM entities e \
             LEFT JOIN chunk_embeddings ce ON ce.chunk_id = e.id \
             WHERE ce.chunk_id IS NULL",
            3,
        );
        assert_eq!(missing_absent, 3, "absent table must report all missing");
    }

    #[test]
    fn status_filter_round_trip() {
        for f in [
            EmbeddingStatusFilter::Pending,
            EmbeddingStatusFilter::InProgress,
            EmbeddingStatusFilter::Done,
            EmbeddingStatusFilter::Abandoned,
        ] {
            let s: PendingEmbeddingStatus = f.into();
            assert_eq!(
                s.as_str(),
                match f {
                    EmbeddingStatusFilter::Pending => "pending",
                    EmbeddingStatusFilter::InProgress => "in_progress",
                    EmbeddingStatusFilter::Done => "done",
                    EmbeddingStatusFilter::Abandoned => "abandoned",
                }
            );
        }
    }
}
