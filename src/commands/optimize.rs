//! Handler for the `optimize` CLI subcommand.

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
    # Optimize via SQLITE_GRAPHRAG_DB_PATH env var\n  \
    SQLITE_GRAPHRAG_DB_PATH=/data/graphrag.sqlite sqlite-graphrag optimize")]
pub struct OptimizeArgs {
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct OptimizeResponse {
    db_path: String,
    status: String,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: OptimizeArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    let conn = open_rw(&paths.db)?;
    conn.execute_batch("PRAGMA optimize;")?;

    output::emit_json(&OptimizeResponse {
        db_path: paths.db.display().to_string(),
        status: "ok".to_string(),
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
    fn optimize_response_serializa_campos_obrigatorios() {
        let resp = OptimizeResponse {
            db_path: "/tmp/graphrag.sqlite".to_string(),
            status: "ok".to_string(),
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
            db: Some(db_path.to_string_lossy().to_string()),
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
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["status"], "ok", "status deve ser sempre 'ok'");
    }
}
