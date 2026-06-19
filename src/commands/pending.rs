//! GAP-001 (v1.0.82): `pending` subcommand — inspect and manage the
//! three-stage `remember` checkpoint queue persisted in `pending_memories`.
//!
//! ## Subcommands
//! - `pending list [--status <STATUS>]` — show entries by status
//! - `pending show <pending_id>` — show one entry in full
//! - `pending cleanup --staged-cleanup-after <SECONDS>` — remove old abandoned
//!
//! The `pending` table is the durable footprint of the v1.0.82 staging pipeline
//! (Stage A → B → C). When a host crashes between Stage B and Stage C the entry
//! stays in `embedding_done` (or `embedding_in_progress`) and can be inspected
//! or cleaned via this subcommand.

use clap::{Args, Subcommand};
use serde::Serialize;

use crate::errors::AppError;
use crate::output::emit_json_compact;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::pending_memories::{self, PendingMemory, PendingStatus};

#[derive(Debug, Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # List all entries currently waiting for embedding (Stage A done, Stage B pending)\n  \
    sqlite-graphrag pending list --status validated --json\n\n  \
    # Show the full record of pending_id 42\n  \
    sqlite-graphrag pending show 42 --json\n\n  \
    # Clean up entries abandoned for >24h (86400 seconds)\n  \
    sqlite-graphrag pending cleanup --staged-cleanup-after 86400 --yes")]
pub struct PendingArgs {
    #[command(subcommand)]
    pub cmd: PendingCmd,
}

#[derive(Debug, Subcommand)]
pub enum PendingCmd {
    /// List entries by status (defaults to all non-committed).
    List(PendingListArgs),
    /// Show one entry in full (includes body, entities_json, embedding_dim).
    Show(PendingShowArgs),
    /// Remove entries older than `--staged-cleanup-after` seconds.
    Cleanup(PendingCleanupArgs),
}

#[derive(Debug, Args)]
pub struct PendingListArgs {
    /// Filter by status: validated | embedding_in_progress | embedding_done |
    /// committed | abandoned | failed. Default: all.
    #[arg(long, value_enum)]
    pub status: Option<PendingStatusArg>,
    /// Maximum number of entries to return. Default: 100.
    #[arg(long, default_value_t = 100)]
    pub limit: usize,
    /// GAP-E2E-010b (v1.0.89): explicit database path override. Defaults to
    /// the path resolved by `AppPaths::resolve(None)` when omitted. Honors
    /// env var `SQLITE_GRAPHRAG_DB_PATH`.
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[value(rename_all = "snake_case")]
pub enum PendingStatusArg {
    Validated,
    EmbeddingInProgress,
    EmbeddingDone,
    Committed,
    Abandoned,
    Failed,
}

impl From<PendingStatusArg> for PendingStatus {
    fn from(value: PendingStatusArg) -> Self {
        match value {
            PendingStatusArg::Validated => Self::Validated,
            PendingStatusArg::EmbeddingInProgress => Self::EmbeddingInProgress,
            PendingStatusArg::EmbeddingDone => Self::EmbeddingDone,
            PendingStatusArg::Committed => Self::Committed,
            PendingStatusArg::Abandoned => Self::Abandoned,
            PendingStatusArg::Failed => Self::Failed,
        }
    }
}

#[derive(Debug, Args)]
pub struct PendingShowArgs {
    /// Pending id returned by `remember --stage-only`.
    pub pending_id: i64,
    /// GAP-E2E-010b (v1.0.89): explicit database path override. Defaults to
    /// the path resolved by `AppPaths::resolve(None)` when omitted. Honors
    /// env var `SQLITE_GRAPHRAG_DB_PATH`.
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Debug, Args)]
pub struct PendingCleanupArgs {
    /// Age in seconds after which an entry is eligible for cleanup.
    #[arg(long, default_value_t = 86400)]
    pub staged_cleanup_after: u64,
    /// Skip the interactive confirmation prompt.
    #[arg(long)]
    pub yes: bool,
    /// Dry-run: list what would be removed without touching the database.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Serialize)]
struct PendingListEntry {
    pending_id: i64,
    name: String,
    namespace: String,
    memory_type: String,
    status: String,
    attempt_count: i32,
    last_error: Option<String>,
    embedding_dim: Option<i32>,
    created_at: i64,
    updated_at: i64,
}

impl From<&PendingMemory> for PendingListEntry {
    fn from(p: &PendingMemory) -> Self {
        Self {
            pending_id: p.pending_id,
            name: p.name.clone(),
            namespace: p.namespace.clone(),
            memory_type: p.memory_type.clone(),
            status: p.status.as_str().to_string(),
            attempt_count: p.attempt_count,
            last_error: p.last_error.clone(),
            embedding_dim: p.embedding_dim,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

#[derive(Serialize)]
struct PendingListOutput {
    action: &'static str,
    filter_status: Option<String>,
    count: usize,
    entries: Vec<PendingListEntry>,
    elapsed_ms: u64,
}

#[derive(Serialize)]
struct PendingShowOutput {
    action: &'static str,
    entry: PendingMemory,
    elapsed_ms: u64,
}

#[derive(Serialize)]
struct PendingCleanupOutput {
    action: &'static str,
    dry_run: bool,
    staged_cleanup_after_secs: u64,
    candidates: usize,
    removed: usize,
    elapsed_ms: u64,
    yes: bool,
}

pub fn run(args: PendingArgs) -> Result<(), AppError> {
    match args.cmd {
        PendingCmd::List(a) => run_list(a),
        PendingCmd::Show(a) => run_show(a),
        PendingCmd::Cleanup(a) => run_cleanup(a),
    }
}

fn open_conn(db_override: Option<&str>) -> Result<(AppPaths, rusqlite::Connection), AppError> {
    // GAP-E2E-010b (v1.0.89): honor `--db <PATH>` for parity with the
    // rest of the CLI surface. `AppPaths::resolve` accepts the same value
    // passed by callers of other subcommands, keeping path semantics
    // consistent across the entire command surface.
    let paths = AppPaths::resolve(db_override)?;
    let conn = open_rw(&paths.db)?;
    Ok((paths, conn))
}

fn run_list(args: PendingListArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let (_paths, conn) = open_conn(args.db.as_deref())?;

    // If a status filter was provided, query that single status. Otherwise return
    // all six buckets so the operator can see the full staging landscape.
    let entries: Vec<PendingMemory> = if let Some(status) = args.status {
        pending_memories::list_by_status(&conn, status.into(), args.limit)?
    } else {
        let mut all = Vec::new();
        for status in [
            PendingStatus::EmbeddingInProgress,
            PendingStatus::EmbeddingDone,
            PendingStatus::Validated,
            PendingStatus::Abandoned,
            PendingStatus::Failed,
        ] {
            let mut bucket = pending_memories::list_by_status(&conn, status, args.limit)?;
            all.append(&mut bucket);
        }
        all.truncate(args.limit);
        all
    };

    let count = entries.len();
    let entries_out: Vec<PendingListEntry> = entries.iter().map(PendingListEntry::from).collect();
    let output = PendingListOutput {
        action: "pending_list",
        filter_status: args.status.map(|s| {
            match s {
                PendingStatusArg::Validated => "validated",
                PendingStatusArg::EmbeddingInProgress => "embedding_in_progress",
                PendingStatusArg::EmbeddingDone => "embedding_done",
                PendingStatusArg::Committed => "committed",
                PendingStatusArg::Abandoned => "abandoned",
                PendingStatusArg::Failed => "failed",
            }
            .to_string()
        }),
        count,
        entries: entries_out,
        elapsed_ms: start.elapsed().as_millis() as u64,
    };
    emit_json_compact(&output)
}

fn run_show(args: PendingShowArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let (_paths, conn) = open_conn(args.db.as_deref())?;
    let entry = pending_memories::find_by_id(&conn, args.pending_id)?.ok_or_else(|| {
        AppError::NotFound(format!(
            "pending_id {} not found in pending_memories",
            args.pending_id
        ))
    })?;
    let output = PendingShowOutput {
        action: "pending_show",
        entry,
        elapsed_ms: start.elapsed().as_millis() as u64,
    };
    emit_json_compact(&output)
}

fn run_cleanup(args: PendingCleanupArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let (_paths, conn) = open_conn(None)?;

    // Count candidates first so dry-run is non-mutating.
    let candidates = pending_memories::list_by_status(&conn, PendingStatus::Abandoned, 100_000)?
        .into_iter()
        .filter(|p| {
            let now = chrono::Utc::now().timestamp();
            now - p.updated_at >= args.staged_cleanup_after as i64
        })
        .count();

    let removed = if args.dry_run {
        0
    } else {
        pending_memories::cleanup_older_than(&conn, args.staged_cleanup_after as i64)?
    };

    let output = PendingCleanupOutput {
        action: if args.dry_run {
            "pending_cleanup_dry_run"
        } else {
            "pending_cleanup"
        },
        dry_run: args.dry_run,
        staged_cleanup_after_secs: args.staged_cleanup_after,
        candidates,
        removed,
        elapsed_ms: start.elapsed().as_millis() as u64,
        yes: args.yes,
    };
    emit_json_compact(&output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_arg_round_trip_all_variants() {
        for arg in [
            PendingStatusArg::Validated,
            PendingStatusArg::EmbeddingInProgress,
            PendingStatusArg::EmbeddingDone,
            PendingStatusArg::Committed,
            PendingStatusArg::Abandoned,
            PendingStatusArg::Failed,
        ] {
            let status: PendingStatus = arg.into();
            assert_eq!(status.as_str(), arg_to_str(arg));
        }
    }

    fn arg_to_str(arg: PendingStatusArg) -> &'static str {
        match arg {
            PendingStatusArg::Validated => "validated",
            PendingStatusArg::EmbeddingInProgress => "embedding_in_progress",
            PendingStatusArg::EmbeddingDone => "embedding_done",
            PendingStatusArg::Committed => "committed",
            PendingStatusArg::Abandoned => "abandoned",
            PendingStatusArg::Failed => "failed",
        }
    }
}
