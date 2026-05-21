//! Handler for the `fts` CLI subcommand family.
//!
//! Provides two maintenance operations for the FTS5 full-text search index:
//! - `rebuild`: drops and reconstructs the index from the `memories` table.
//! - `check`: runs the FTS5 integrity-check without modifying the index.

use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::{open_ro, open_rw};
use serde::Serialize;

/// Arguments for the `fts` subcommand family.
#[derive(clap::Args)]
#[command(
    about = "FTS5 full-text search index management",
    after_long_help = "EXAMPLES:\n  \
        # Rebuild the full-text search index from memories table\n  \
        sqlite-graphrag fts rebuild\n\n  \
        # Check FTS5 index integrity\n  \
        sqlite-graphrag fts check --json\n\n  \
        # Show FTS5 index statistics\n  \
        sqlite-graphrag fts stats --json"
)]
pub struct FtsArgs {
    #[command(subcommand)]
    pub command: FtsSubcommand,
}

/// Subcommands nested under `fts`.
#[derive(clap::Subcommand)]
pub enum FtsSubcommand {
    /// Rebuild the FTS5 index from the memories table.
    #[command(after_long_help = "EXAMPLES:\n  \
        # Rebuild the full-text search index\n  \
        sqlite-graphrag fts rebuild\n\n  \
        # Rebuild with custom database path\n  \
        sqlite-graphrag fts rebuild --db /path/to/graphrag.sqlite")]
    Rebuild(FtsRebuildArgs),
    /// Run FTS5 integrity-check without modifying the index.
    #[command(after_long_help = "EXAMPLES:\n  \
        # Check FTS5 index integrity\n  \
        sqlite-graphrag fts check\n\n  \
        # Check with custom database path\n  \
        sqlite-graphrag fts check --db /path/to/graphrag.sqlite")]
    Check(FtsCheckArgs),
    /// Show FTS5 index statistics (row count, shadow pages, functional status).
    #[command(after_long_help = "EXAMPLES:\n  \
        # Show FTS5 index statistics\n  \
        sqlite-graphrag fts stats\n\n  \
        # Stats with custom database path\n  \
        sqlite-graphrag fts stats --db /path/to/graphrag.sqlite")]
    Stats(FtsStatsArgs),
}

/// Arguments for `fts rebuild`.
#[derive(clap::Args)]
pub struct FtsRebuildArgs {
    /// No-op; JSON is always emitted on stdout.
    #[arg(long, hide = true)]
    pub json: bool,
    /// Path to the SQLite database file.
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

/// Arguments for `fts check`.
#[derive(clap::Args)]
pub struct FtsCheckArgs {
    /// No-op; JSON is always emitted on stdout.
    #[arg(long, hide = true)]
    pub json: bool,
    /// Path to the SQLite database file.
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

/// Arguments for `fts stats`.
#[derive(clap::Args)]
pub struct FtsStatsArgs {
    /// No-op; JSON is always emitted on stdout.
    #[arg(long, hide = true)]
    pub json: bool,
    /// Path to the SQLite database file.
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct FtsRebuildResponse {
    action: String,
    rows_indexed: i64,
    elapsed_ms: u64,
}

#[derive(Serialize)]
struct FtsCheckResponse {
    action: String,
    integrity_ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
    elapsed_ms: u64,
}

#[derive(Serialize)]
struct FtsStatsResponse {
    total_rows: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    shadow_pages: Option<i64>,
    fts_functional: bool,
    elapsed_ms: u64,
}

/// Dispatch entry point called from `main`.
///
/// # Errors
/// Propagates any [`AppError`] raised by the underlying subcommand.
pub fn run(args: FtsArgs) -> Result<(), AppError> {
    match args.command {
        FtsSubcommand::Rebuild(a) => run_rebuild(a),
        FtsSubcommand::Check(a) => run_check(a),
        FtsSubcommand::Stats(a) => run_stats(a),
    }
}

/// Rebuilds the FTS5 index by issuing the `'rebuild'` special command.
///
/// The FTS5 `INSERT INTO fts_memories(fts_memories) VALUES('rebuild')` statement
/// drops all index data and re-populates it from the content table in a single
/// transaction. Use this after bulk imports or when `fts check` reports a failure.
///
/// # Errors
/// Returns [`AppError::Database`] on any SQLite failure.
fn run_rebuild(args: FtsRebuildArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let paths = AppPaths::resolve(args.db.as_deref())?;
    crate::storage::connection::ensure_db_ready(&paths)?;
    let conn = open_rw(&paths.db)?;

    let table_exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='fts_memories'",
        [],
        |r| r.get::<_, i64>(0).map(|v| v > 0),
    )?;
    if !table_exists {
        return Err(AppError::Validation(
            "FTS5 table 'fts_memories' does not exist — run 'sqlite-graphrag init' first"
                .to_string(),
        ));
    }

    conn.execute_batch("INSERT INTO fts_memories(fts_memories) VALUES('rebuild');")?;

    let rows: i64 = conn.query_row("SELECT COUNT(*) FROM fts_memories", [], |r| r.get(0))?;

    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;

    output::emit_json(&FtsRebuildResponse {
        action: "rebuilt".to_string(),
        rows_indexed: rows,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

/// Runs the FTS5 integrity-check without modifying the index.
///
/// The FTS5 integrity-check is triggered by:
/// ```sql
/// INSERT INTO fts_memories(fts_memories, rank) VALUES('integrity-check', 1);
/// ```
/// SQLite raises an error if the index is corrupt, so a successful `execute_batch`
/// means the index is healthy. On failure, `integrity_ok` is `false` and the
/// `detail` field carries an actionable hint.
///
/// # Errors
/// Returns [`AppError`] only on unexpected I/O or path resolution failures;
/// an FTS5 corruption is reported as `integrity_ok: false`, not as a Rust error.
fn run_check(args: FtsCheckArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let paths = AppPaths::resolve(args.db.as_deref())?;
    crate::storage::connection::ensure_db_ready(&paths)?;
    let conn = open_rw(&paths.db)?;

    let integrity_ok = conn
        .execute_batch("INSERT INTO fts_memories(fts_memories, rank) VALUES('integrity-check', 1);")
        .is_ok();

    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").ok();

    output::emit_json(&FtsCheckResponse {
        action: "checked".to_string(),
        integrity_ok,
        detail: if integrity_ok {
            None
        } else {
            Some("FTS5 integrity-check failed — run 'sqlite-graphrag fts rebuild'".to_string())
        },
        elapsed_ms: start.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

/// Returns FTS5 index statistics: total indexed rows, shadow table page count (best-effort),
/// and a functional liveness check.
///
/// # Errors
/// Returns [`AppError`] only on unexpected I/O or path resolution failures.
fn run_stats(args: FtsStatsArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let paths = AppPaths::resolve(args.db.as_deref())?;
    crate::storage::connection::ensure_db_ready(&paths)?;
    let conn = open_ro(&paths.db)?;

    // 1. Total indexed rows in the FTS5 content table.
    let total_rows: i64 = conn.query_row("SELECT COUNT(*) FROM fts_memories", [], |r| r.get(0))?;

    // 2. Shadow pages — queries the internal `_data` shadow table.
    //    This may not exist on all SQLite builds; treat any failure as None.
    let shadow_pages: Option<i64> = conn
        .query_row("SELECT COUNT(*) FROM fts_memories_data", [], |r| r.get(0))
        .ok();

    // 3. Functional liveness: SELECT with FTS5 match syntax against a wildcard.
    //    A successful LIMIT 0 query confirms the FTS5 module is operational.
    let fts_functional = conn
        .execute_batch("SELECT * FROM fts_memories('*') LIMIT 0;")
        .is_ok();

    output::emit_json(&FtsStatsResponse {
        total_rows,
        shadow_pages,
        fts_functional,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_rebuild_response_serializes_all_fields() {
        let resp = FtsRebuildResponse {
            action: "rebuilt".to_string(),
            rows_indexed: 42,
            elapsed_ms: 10,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["action"], "rebuilt");
        assert_eq!(json["rows_indexed"], 42i64);
        assert_eq!(json["elapsed_ms"], 10u64);
    }

    #[test]
    fn fts_check_response_integrity_ok_omits_detail() {
        let resp = FtsCheckResponse {
            action: "checked".to_string(),
            integrity_ok: true,
            detail: None,
            elapsed_ms: 5,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["action"], "checked");
        assert_eq!(json["integrity_ok"], true);
        assert!(
            json.get("detail").is_none(),
            "detail must be absent when integrity_ok is true"
        );
        assert_eq!(json["elapsed_ms"], 5u64);
    }

    #[test]
    fn fts_check_response_corruption_includes_detail() {
        let resp = FtsCheckResponse {
            action: "checked".to_string(),
            integrity_ok: false,
            detail: Some(
                "FTS5 integrity-check failed — run 'sqlite-graphrag fts rebuild'".to_string(),
            ),
            elapsed_ms: 3,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["integrity_ok"], false);
        assert!(
            json["detail"].as_str().unwrap().contains("fts rebuild"),
            "detail must mention the remediation command"
        );
    }

    #[test]
    fn fts_rebuild_response_elapsed_ms_non_negative() {
        let resp = FtsRebuildResponse {
            action: "rebuilt".to_string(),
            rows_indexed: 0,
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert!(json["elapsed_ms"].as_u64().is_some());
    }

    #[test]
    fn fts_check_response_elapsed_ms_non_negative() {
        let resp = FtsCheckResponse {
            action: "checked".to_string(),
            integrity_ok: true,
            detail: None,
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert!(json["elapsed_ms"].as_u64().is_some());
    }

    #[test]
    fn fts_stats_response_serializes_all_fields() {
        let resp = FtsStatsResponse {
            total_rows: 150,
            shadow_pages: Some(12),
            fts_functional: true,
            elapsed_ms: 8,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["total_rows"], 150i64);
        assert_eq!(json["shadow_pages"], 12i64);
        assert_eq!(json["fts_functional"], true);
        assert_eq!(json["elapsed_ms"], 8u64);
    }

    #[test]
    fn fts_stats_response_omits_shadow_pages_when_none() {
        let resp = FtsStatsResponse {
            total_rows: 0,
            shadow_pages: None,
            fts_functional: false,
            elapsed_ms: 2,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert!(
            json.get("shadow_pages").is_none(),
            "shadow_pages must be absent when None"
        );
        assert_eq!(json["fts_functional"], false);
    }

    #[test]
    fn fts_stats_response_fts_not_functional() {
        let resp = FtsStatsResponse {
            total_rows: 5,
            shadow_pages: None,
            fts_functional: false,
            elapsed_ms: 1,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["fts_functional"], false);
        assert_eq!(json["total_rows"], 5i64);
    }
}
