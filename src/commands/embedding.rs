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
    /// v1.0.84 (ADR-0042): discriminador do backend LLM que seria
    /// invocado para processar embeddings live. `"claude" | "codex"
    /// | "none" | "auto"`. `"auto"` indica que o caller pediu Auto e
    /// a chain codex→claude→none seria iterada em runtime.
    backend_invoked: &'static str,
    counts: EmbeddingStatusCounts,
    elapsed_ms: u64,
}

#[derive(Serialize, Default)]
struct EmbeddingStatusCounts {
    pending: usize,
    in_progress: usize,
    done: usize,
    abandoned: usize,
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
        LlmBackendChoice::Auto => "auto",
    };

    let output = EmbeddingStatusOutput {
        action: "embedding_status",
        backend_invoked,
        counts,
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
