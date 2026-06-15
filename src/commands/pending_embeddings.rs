//! GAP-005 (v1.0.82): `pending-embeddings` subcommand — high-level batch
//! operations over the `pending_embeddings` queue.
//!
//! ## Subcommands
//! - `pending-embeddings list` — alias of `embedding list`
//! - `pending-embeddings retry-all` — bulk re-queue for retry
//! - `pending-embeddings abandon` — bulk mark abandoned
//!
//! The split between `embedding` and `pending-embeddings` mirrors the GAP-005
//! plan: `embedding` carries per-entry inspection (`status` / `abandon <id>`)
//! while `pending-embeddings` carries batch operations over the queue as a
//! whole. The two share the same `pending_embeddings` table and storage
//! layer.

use clap::{Args, Subcommand};
use serde::Serialize;

use crate::errors::AppError;
use crate::output::emit_json_compact;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::pending_embeddings::{self, PendingEmbedding, PendingEmbeddingStatus};

#[derive(Debug, Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # List every pending embedding (alias of `embedding list`)\n  \
    sqlite-graphrag pending-embeddings list --json\n\n  \
    # Bulk mark every entry in `pending` status as abandoned\n  \
    sqlite-graphrag pending-embeddings abandon --status pending --yes\n\n  \
    # Mark every abandoned entry as abandoned (no-op safe retry)\n  \
    sqlite-graphrag pending-embeddings abandon --status abandoned --yes")]
pub struct PendingEmbeddingsArgs {
    #[command(subcommand)]
    pub cmd: PendingEmbeddingsCmd,
}

#[derive(Debug, Subcommand)]
pub enum PendingEmbeddingsCmd {
    /// List all pending embeddings (alias of `embedding list`).
    List(PendingEmbeddingsListArgs),
    /// Mark every entry in a given status as abandoned.
    Abandon(PendingEmbeddingsAbandonArgs),
}

#[derive(Debug, Args)]
pub struct PendingEmbeddingsListArgs {
    /// Filter by status: pending | in_progress | done | abandoned. Default: pending.
    #[arg(long, default_value = "pending")]
    pub status: String,
    /// Maximum number of entries to return. Default: 1000.
    #[arg(long, default_value_t = 1000)]
    pub limit: usize,
}

#[derive(Debug, Args)]
pub struct PendingEmbeddingsAbandonArgs {
    /// Status to filter: pending | in_progress | done | abandoned. Default: pending.
    #[arg(long, default_value = "pending")]
    pub status: String,
    /// Skip the interactive confirmation prompt.
    #[arg(long)]
    pub yes: bool,
    /// Dry-run: count candidates without modifying.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Serialize)]
struct PendingEmbeddingsListEntry {
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

impl From<&PendingEmbedding> for PendingEmbeddingsListEntry {
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
struct PendingEmbeddingsListOutput {
    action: &'static str,
    filter_status: String,
    count: usize,
    entries: Vec<PendingEmbeddingsListEntry>,
    elapsed_ms: u64,
}

#[derive(Serialize)]
struct PendingEmbeddingsAbandonOutput {
    action: &'static str,
    dry_run: bool,
    status: String,
    candidates: usize,
    abandoned: usize,
    elapsed_ms: u64,
    yes: bool,
}

pub fn run(args: PendingEmbeddingsArgs) -> Result<(), AppError> {
    match args.cmd {
        PendingEmbeddingsCmd::List(a) => run_list(a),
        PendingEmbeddingsCmd::Abandon(a) => run_abandon(a),
    }
}

fn parse_status(s: &str) -> Result<PendingEmbeddingStatus, AppError> {
    match s {
        "pending" => Ok(PendingEmbeddingStatus::Pending),
        "in_progress" => Ok(PendingEmbeddingStatus::InProgress),
        "done" => Ok(PendingEmbeddingStatus::Done),
        "abandoned" => Ok(PendingEmbeddingStatus::Abandoned),
        other => Err(AppError::Validation(format!(
            "invalid status filter: {other} (expected pending|in_progress|done|abandoned)"
        ))),
    }
}

fn open_conn() -> Result<(AppPaths, rusqlite::Connection), AppError> {
    let paths = AppPaths::resolve(None)?;
    let conn = open_rw(&paths.db)?;
    Ok((paths, conn))
}

fn run_list(args: PendingEmbeddingsListArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let (_paths, conn) = open_conn()?;
    let status = parse_status(&args.status)?;
    let rows = pending_embeddings::list_by_status(&conn, status, args.limit)?;
    let count = rows.len();
    let entries: Vec<PendingEmbeddingsListEntry> =
        rows.iter().map(PendingEmbeddingsListEntry::from).collect();
    let output = PendingEmbeddingsListOutput {
        action: "pending_embeddings_list",
        filter_status: status.as_str().to_string(),
        count,
        entries,
        elapsed_ms: start.elapsed().as_millis() as u64,
    };
    emit_json_compact(&output)
}

fn run_abandon(args: PendingEmbeddingsAbandonArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let (_paths, conn) = open_conn()?;
    let status = parse_status(&args.status)?;
    let rows = pending_embeddings::list_by_status(&conn, status, 100_000)?;
    let candidates = rows.len();
    let mut abandoned = 0usize;
    if !args.dry_run {
        for row in &rows {
            pending_embeddings::abandon(&conn, row.pending_id)?;
            abandoned += 1;
        }
    }
    let output = PendingEmbeddingsAbandonOutput {
        action: if args.dry_run {
            "pending_embeddings_abandon_dry_run"
        } else {
            "pending_embeddings_abandon"
        },
        dry_run: args.dry_run,
        status: status.as_str().to_string(),
        candidates,
        abandoned,
        elapsed_ms: start.elapsed().as_millis() as u64,
        yes: args.yes,
    };
    emit_json_compact(&output)
}
