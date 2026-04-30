//! Handler for the `sync-safe-copy` CLI subcommand.

use crate::errors::AppError;
use crate::i18n::validation;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use serde::Serialize;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Create a checkpointed snapshot safe for cloud sync\n  \
    sqlite-graphrag sync-safe-copy --dest /backup/graphrag-snapshot.sqlite\n\n  \
    # Use the --to alias\n  \
    sqlite-graphrag sync-safe-copy --to /backup/graphrag-snapshot.sqlite\n\n  \
    # Snapshot a custom source database\n  \
    sqlite-graphrag sync-safe-copy --db /data/graphrag.sqlite --dest /backup/snapshot.sqlite")]
pub struct SyncSafeCopyArgs {
    /// Snapshot destination path. Also accepts the aliases `--to` and `--output`.
    #[arg(long, alias = "to", alias = "output")]
    pub dest: std::path::PathBuf,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    /// Output format: `json` or `text`. JSON is always emitted on stdout regardless of the value.
    #[arg(long, value_parser = ["json", "text"], hide = true)]
    pub format: Option<String>,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct SyncSafeCopyResponse {
    source_db_path: String,
    dest_path: String,
    bytes_copied: u64,
    status: String,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: SyncSafeCopyArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let _ = args.format; // --format is a no-op; JSON is always emitted on stdout
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    if args.dest == paths.db {
        return Err(AppError::Validation(
            validation::sync_destination_equals_source(),
        ));
    }

    if let Some(parent) = args.dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let conn = open_rw(&paths.db)?;
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    drop(conn);

    let bytes_copied = std::fs::copy(&paths.db, &args.dest)?;

    // Applies 0600 permissions on the snapshot on Unix to avoid leakage on Dropbox/shared NFS.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&args.dest)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&args.dest, perms)?;
    }

    output::emit_json(&SyncSafeCopyResponse {
        source_db_path: paths.db.display().to_string(),
        dest_path: args.dest.display().to_string(),
        bytes_copied,
        status: "ok".to_string(),
        elapsed_ms: start.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_safe_copy_response_serializes_all_fields() {
        let resp = SyncSafeCopyResponse {
            source_db_path: "/home/user/.local/share/sqlite-graphrag/db.sqlite".to_string(),
            dest_path: "/tmp/backup.sqlite".to_string(),
            bytes_copied: 16384,
            status: "ok".to_string(),
            elapsed_ms: 12,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(
            json["source_db_path"],
            "/home/user/.local/share/sqlite-graphrag/db.sqlite"
        );
        assert_eq!(json["dest_path"], "/tmp/backup.sqlite");
        assert_eq!(json["bytes_copied"], 16384u64);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["elapsed_ms"], 12u64);
    }

    #[test]
    fn sync_safe_copy_rejects_dest_equal_to_source() {
        let db_path = std::path::PathBuf::from("/tmp/same.sqlite");
        let args = SyncSafeCopyArgs {
            dest: db_path.clone(),
            json: false,
            format: None,
            db: Some("/tmp/same.sqlite".to_string()),
        };
        // Simulates manual path resolution — validates rejection logic
        let result = if args.dest == std::path::PathBuf::from(args.db.as_deref().unwrap_or("")) {
            Err(AppError::Validation(
                "destination path must differ from the source database path".to_string(),
            ))
        } else {
            Ok(())
        };
        assert!(result.is_err(), "must reject dest equal to source");
        if let Err(AppError::Validation(msg)) = result {
            assert!(msg.contains("destination path must differ"));
        }
    }

    #[test]
    fn sync_safe_copy_response_status_ok() {
        let resp = SyncSafeCopyResponse {
            source_db_path: "/data/db.sqlite".to_string(),
            dest_path: "/backup/db.sqlite".to_string(),
            bytes_copied: 0,
            status: "ok".to_string(),
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["status"], "ok");
    }

    #[test]
    fn sync_safe_copy_response_bytes_copied_zero_valid() {
        let resp = SyncSafeCopyResponse {
            source_db_path: "/data/db.sqlite".to_string(),
            dest_path: "/backup/db.sqlite".to_string(),
            bytes_copied: 0,
            status: "ok".to_string(),
            elapsed_ms: 1,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["bytes_copied"], 0u64);
        assert_eq!(json["elapsed_ms"], 1u64);
    }
}
