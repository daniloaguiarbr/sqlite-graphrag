//! Handler for the `optimize` CLI subcommand.

use crate::commands::fts::check_fts_functional;
use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use serde::Serialize;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Run PRAGMA optimize on the default database\n  \
    sqlite-graphrag optimize\n\n  \
    # Optimize a database at a custom path\n  \
    sqlite-graphrag optimize --db /path/to/graphrag.sqlite\n\n  \
    # Skip the FTS5 rebuild even if the index looks unhealthy\n  \
    sqlite-graphrag optimize --skip-fts\n\n  \
    # Dry-run: only report FTS5 health status, do not rebuild\n  \
    sqlite-graphrag optimize --fts-dry-run\n\n  \
    # Run optimize non-interactively (skip confirmation prompts)\n  \
    sqlite-graphrag optimize --yes\n\n  \
    # Force a full FTS5 rebuild even if the index already passes integrity-check\n  \
    sqlite-graphrag optimize --no-fts-skip-when-functional\n\n  \
    # Optimize via SQLITE_GRAPHRAG_DB_PATH env var\n  \
    SQLITE_GRAPHRAG_DB_PATH=/data/graphrag.sqlite sqlite-graphrag optimize")]
pub struct OptimizeArgs {
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
    #[arg(long, default_value_t = false, help = "Skip FTS5 index rebuild")]
    pub skip_fts: bool,
    /// When true (default), the FTS5 rebuild step is skipped when
    /// `fts check` reports the index is already functional. Saves 5-15
    /// minutes on large databases. Set to false to always rebuild.
    #[arg(
        long,
        default_value_t = true,
        help = "Skip FTS5 rebuild when index is already functional (saves minutes on big DBs)"
    )]
    pub fts_skip_when_functional: bool,
    /// G36 Step 2 (v1.0.69): run `fts check` + `fts stats` only, do not
    /// trigger any rebuild. Exit code is 0 when the index is healthy, 1
    /// when a rebuild would be recommended.
    #[arg(
        long,
        default_value_t = false,
        help = "G36: only run fts check + fts stats, do not rebuild (exit 1 if rebuild recommended)"
    )]
    pub fts_dry_run: bool,
    /// G36 Step 3 (v1.0.69): emit a tracing::info! progress line every
    /// N seconds during the FTS5 rebuild. The FTS5 `rebuild` command is
    /// synchronous and does not call the SQLite progress handler, so the
    /// progress is sampled at the configured interval. Use 0 to disable.
    #[arg(
        long,
        default_value_t = 30,
        help = "G36: emit progress line every N seconds during FTS5 rebuild (0 to disable)"
    )]
    pub fts_progress: u64,
    /// G36 Step 4 (v1.0.69): skip all confirmation prompts. Required
    /// for non-interactive CI/CD pipelines that cannot answer `y/N`.
    #[arg(
        long,
        default_value_t = false,
        help = "G36: skip confirmation prompts (required for non-interactive CI)"
    )]
    pub yes: bool,
}

#[derive(Serialize)]
struct OptimizeResponse {
    db_path: String,
    status: String,
    /// True when the FTS5 index was rebuilt during this optimize run.
    fts_rebuilt: bool,
    /// True when the FTS5 rebuild was skipped because the index was already healthy.
    fts_skipped_functional: bool,
    /// True when FTS5 was detected as unhealthy AND the rebuild was attempted.
    fts_unhealthy: bool,
    /// Number of FTS5 rows indexed during the rebuild (G36 progress observability).
    fts_rows_indexed: Option<i64>,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: OptimizeArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    let conn = open_rw(&paths.db)?;
    conn.execute_batch("PRAGMA optimize;")?;

    // G36: pre-check FTS5 health before triggering a multi-minute rebuild.
    let fts_functional = if !args.skip_fts {
        check_fts_functional(&conn).unwrap_or(false)
    } else {
        false
    };

    // G36 Passo 2 (v1.0.69): dry-run path. Run fts check + fts stats, emit
    // JSON envelope, and return exit 1 when a rebuild would be recommended.
    if args.fts_dry_run {
        let recommend_rebuild = !fts_functional;
        output::emit_json(&OptimizeResponse {
            db_path: paths.db.display().to_string(),
            status: if recommend_rebuild {
                "rebuild_recommended".to_string()
            } else {
                "ok".to_string()
            },
            fts_rebuilt: false,
            fts_skipped_functional: false,
            fts_unhealthy: !fts_functional,
            fts_rows_indexed: None,
            elapsed_ms: inicio.elapsed().as_millis() as u64,
        })?;
        if recommend_rebuild {
            std::process::exit(1);
        }
        return Ok(());
    }

    let (fts_rebuilt, fts_skipped_functional, fts_unhealthy, fts_rows_indexed) = if args.skip_fts {
        (false, false, false, None)
    } else if args.fts_skip_when_functional && fts_functional {
        tracing::info!(target: "optimize",
            "FTS5 index already functional; skipping rebuild (use --no-fts-skip-when-functional to override)"
        );
        (false, true, false, None)
    } else {
        if !fts_functional {
            tracing::warn!(target: "optimize",
                "FTS5 index reported unhealthy; running full rebuild"
            );
        }
        // Capture row count BEFORE rebuild so we can report progress.
        // (FTS5 rebuild is synchronous; a true callback would require
        // `sqlite3_progress_handler` which the FTS5 'rebuild' command
        // does not respect. We sample the row count after.)
        let before: i64 = conn
            .query_row("SELECT COUNT(*) FROM fts_memories", [], |r| r.get(0))
            .unwrap_or(0);
        // G36 Passo 3 (v1.0.69): spawn a lightweight background thread that
        // emits a tracing::info! progress line every `args.fts_progress`
        // seconds while the rebuild is in flight. The FTS5 rebuild command
        // is synchronous and does not call the SQLite progress handler, so
        // the only observability we can add is a row-count poll from a
        // background thread. We open a SEPARATE read-only connection
        // because `rusqlite::Connection` is not `Sync` and the rebuild
        // holds the main connection exclusively. Default 30s; 0 disables.
        let progress_thread = if args.fts_progress > 0 {
            let interval = std::time::Duration::from_secs(args.fts_progress);
            let db_path = paths.db.clone();
            let child = std::thread::spawn(move || loop {
                std::thread::sleep(interval);
                let count: i64 = match crate::storage::connection::open_ro(&db_path) {
                    Ok(c) => c
                        .query_row("SELECT COUNT(*) FROM fts_memories", [], |r| r.get(0))
                        .unwrap_or(-1),
                    Err(_) => -1,
                };
                tracing::info!(target: "optimize", fts_rows = count, "FTS5 rebuild progress sample");
            });
            Some(child)
        } else {
            None
        };
        let rebuilt_ok = conn
            .execute_batch("INSERT INTO fts_memories(fts_memories) VALUES('rebuild');")
            .is_ok();
        if let Some(handle) = progress_thread {
            // The thread runs forever in a sleep loop; we leak it on
            // purpose because (a) it terminates when the process exits
            // and (b) we cannot safely join without a stop signal channel
            // which would add complexity not warranted for a 30s sampler.
            std::mem::forget(handle);
        }
        let after: i64 = if rebuilt_ok {
            conn.query_row("SELECT COUNT(*) FROM fts_memories", [], |r| r.get(0))
                .unwrap_or(0)
        } else {
            0
        };
        // G36 progress: rows_indexed == after - before.  Emitted as a
        // tracing::info! line so operators following logs see the
        // rebuild magnitude without needing NDJSON streaming.
        tracing::info!(target: "optimize", before, after, "FTS5 rebuild complete");
        (rebuilt_ok, false, !fts_functional, Some(after - before))
    };

    // G36 Passo 4 (v1.0.69): --yes flag is currently honored for forward
    // compatibility — every interactive prompt path in optimize must
    // check this flag and skip the prompt when set. As of v1.0.69 there
    // are no interactive prompts in optimize (the user is told up front
    // via the after_long_help), but the flag is reserved so future
    // confirmations can be added without breaking the CLI contract.
    let _ = args.yes;

    output::emit_json(&OptimizeResponse {
        db_path: paths.db.display().to_string(),
        status: "ok".to_string(),
        fts_rebuilt,
        fts_skipped_functional,
        fts_unhealthy,
        fts_rows_indexed,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    #[test]
    fn optimize_response_serializes_required_fields() {
        let resp = OptimizeResponse {
            db_path: "/tmp/graphrag.sqlite".to_string(),
            status: "ok".to_string(),
            fts_rebuilt: false,
            fts_rows_indexed: None,
            fts_skipped_functional: false,
            fts_unhealthy: false,
            elapsed_ms: 5,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["db_path"], "/tmp/graphrag.sqlite");
        assert_eq!(json["elapsed_ms"], 5);
    }

    #[test]
    #[serial]
    fn optimize_auto_inits_when_db_missing() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("missing.sqlite");
        // SAFETY: `#[serial]` guarantees single-threaded execution.
        unsafe {
            std::env::set_var("SQLITE_GRAPHRAG_DB_PATH", db_path.to_str().unwrap());
            std::env::set_var("LOG_LEVEL", "error");
        }

        let args = OptimizeArgs {
            json: false,
            db: Some(db_path.to_string_lossy().into_owned()),
            skip_fts: false,
            fts_skip_when_functional: true,
            fts_dry_run: false,
            fts_progress: 30,
            yes: true,
        };
        let result = run(args);
        assert!(
            result.is_ok(),
            "auto-init must succeed and PRAGMA optimize must run on the fresh database, got {result:?}"
        );
        assert!(
            db_path.exists(),
            "auto-init must create the database file at {}",
            db_path.display()
        );
        // SAFETY: `#[serial]` guarantees single-threaded execution.
        unsafe {
            std::env::remove_var("SQLITE_GRAPHRAG_DB_PATH");
            std::env::remove_var("LOG_LEVEL");
        }
    }

    #[test]
    fn optimize_response_status_ok_fixo() {
        let resp = OptimizeResponse {
            db_path: "/qualquer/caminho".to_string(),
            status: "ok".to_string(),
            fts_rebuilt: false,
            fts_rows_indexed: None,
            fts_skipped_functional: false,
            fts_unhealthy: false,
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["status"], "ok", "status deve ser sempre 'ok'");
    }

    #[test]
    fn optimize_response_serializes_all_fields() {
        let resp = OptimizeResponse {
            db_path: "/data/x.sqlite".into(),
            status: "ok".into(),
            fts_rebuilt: true,
            fts_rows_indexed: Some(0),
            fts_skipped_functional: false,
            fts_unhealthy: true,
            elapsed_ms: 120,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["db_path"], "/data/x.sqlite");
        assert_eq!(v["status"], "ok");
        assert_eq!(v["fts_rebuilt"], true);
        assert_eq!(v["fts_skipped_functional"], false);
        assert_eq!(v["fts_unhealthy"], true);
        assert_eq!(v["elapsed_ms"], 120u64);
    }

    #[test]
    fn optimize_response_includes_fts_flags() {
        // G36: operator must be able to distinguish (a) rebuilt, (b) skipped-healthy,
        // (c) skipped-by-flag from (d) attempted-but-failed. The response
        // exposes fts_rebuilt, fts_skipped_functional, fts_unhealthy booleans.
        let resp = OptimizeResponse {
            db_path: "/x".into(),
            status: "ok".into(),
            fts_rebuilt: true,
            fts_rows_indexed: Some(0),
            fts_skipped_functional: false,
            fts_unhealthy: true,
            elapsed_ms: 1,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["fts_rebuilt"], true);
        assert_eq!(v["fts_skipped_functional"], false);
        assert_eq!(v["fts_unhealthy"], true);
    }
}
