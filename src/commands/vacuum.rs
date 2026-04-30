//! Handler for the `vacuum` CLI subcommand.

use crate::errors::AppError;
use crate::output;
use crate::output::JsonOutputFormat;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use serde::Serialize;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Run VACUUM after WAL checkpoint (default)\n  \
    sqlite-graphrag vacuum\n\n  \
    # Vacuum a database at a custom path\n  \
    sqlite-graphrag vacuum --db /path/to/graphrag.sqlite\n\n  \
    # Vacuum via SQLITE_GRAPHRAG_DB_PATH env var\n  \
    SQLITE_GRAPHRAG_DB_PATH=/data/graphrag.sqlite sqlite-graphrag vacuum")]
pub struct VacuumArgs {
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    /// Run a WAL checkpoint before and after `VACUUM`.
    #[arg(long, default_value_t = true)]
    pub checkpoint: bool,
    /// Output format.
    #[arg(long, value_enum, default_value_t = JsonOutputFormat::Json)]
    pub format: JsonOutputFormat,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct VacuumResponse {
    db_path: String,
    size_before_bytes: u64,
    size_after_bytes: u64,
    /// Bytes reclaimed by VACUUM (size_before_bytes - size_after_bytes), saturating to zero.
    /// Derived field added in v1.0.34 so callers do not have to compute the delta themselves.
    reclaimed_bytes: u64,
    status: String,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: VacuumArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let _ = args.format;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    let size_before_bytes = std::fs::metadata(&paths.db)
        .map(|meta| meta.len())
        .unwrap_or(0);
    let conn = open_rw(&paths.db)?;
    if args.checkpoint {
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    }
    conn.execute_batch("VACUUM;")?;
    if args.checkpoint {
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    }
    drop(conn);
    let size_after_bytes = std::fs::metadata(&paths.db)
        .map(|meta| meta.len())
        .unwrap_or(0);

    output::emit_json(&VacuumResponse {
        db_path: paths.db.display().to_string(),
        size_before_bytes,
        size_after_bytes,
        reclaimed_bytes: size_before_bytes.saturating_sub(size_after_bytes),
        status: "ok".to_string(),
        elapsed_ms: start.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vacuum_response_serializes_all_fields() {
        let resp = VacuumResponse {
            db_path: "/home/user/.local/share/sqlite-graphrag/db.sqlite".to_string(),
            size_before_bytes: 32768,
            size_after_bytes: 16384,
            reclaimed_bytes: 16384,
            status: "ok".to_string(),
            elapsed_ms: 55,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(
            json["db_path"],
            "/home/user/.local/share/sqlite-graphrag/db.sqlite"
        );
        assert_eq!(json["size_before_bytes"], 32768u64);
        assert_eq!(json["size_after_bytes"], 16384u64);
        assert_eq!(json["reclaimed_bytes"], 16384u64);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["elapsed_ms"], 55u64);
    }

    #[test]
    fn vacuum_response_size_after_less_than_or_equal_to_before() {
        let resp = VacuumResponse {
            db_path: "/data/db.sqlite".to_string(),
            size_before_bytes: 65536,
            size_after_bytes: 32768,
            reclaimed_bytes: 32768,
            status: "ok".to_string(),
            elapsed_ms: 100,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        let before = json["size_before_bytes"].as_u64().unwrap();
        let after = json["size_after_bytes"].as_u64().unwrap();
        let reclaimed = json["reclaimed_bytes"].as_u64().unwrap();
        assert!(
            after <= before,
            "size_after_bytes must be <= size_before_bytes after VACUUM"
        );
        assert_eq!(
            reclaimed,
            before - after,
            "reclaimed_bytes must equal size_before_bytes - size_after_bytes"
        );
    }

    #[test]
    fn vacuum_response_status_ok() {
        let resp = VacuumResponse {
            db_path: "/data/db.sqlite".to_string(),
            size_before_bytes: 0,
            size_after_bytes: 0,
            reclaimed_bytes: 0,
            status: "ok".to_string(),
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["status"], "ok");
    }

    #[test]
    fn vacuum_response_elapsed_ms_present_and_non_negative() {
        let resp = VacuumResponse {
            db_path: "/data/db.sqlite".to_string(),
            size_before_bytes: 1024,
            size_after_bytes: 1024,
            reclaimed_bytes: 0,
            status: "ok".to_string(),
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert!(
            json.get("elapsed_ms").is_some(),
            "elapsed_ms field must be present"
        );
        assert!(
            json["elapsed_ms"].as_u64().is_some(),
            "elapsed_ms must be a non-negative integer"
        );
    }
}
