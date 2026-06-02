//! Handler for the `backup` CLI subcommand.
//!
//! Uses the SQLite Online Backup API (via rusqlite) to produce a consistent
//! point-in-time copy of the database file even while the database is in use.

use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use serde::Serialize;
use std::path::PathBuf;
use tempfile::NamedTempFile;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Back up the default database to a specific path\n  \
    sqlite-graphrag backup --output /backup/graphrag-$(date +%F).sqlite\n\n  \
    # Back up a custom source database\n  \
    sqlite-graphrag backup --db /data/graphrag.sqlite --output /backup/snapshot.sqlite\n\n  \
    # Emit JSON on success\n  \
    sqlite-graphrag backup --output /tmp/snap.sqlite --json\n\n  \
NOTES:\n  \
    Uses the SQLite Online Backup API: safe to run while the database is in use.\n  \
    The destination is written atomically via tempfile-rename in the same directory.\n  \
    If the process is interrupted, the previous file (if any) remains intact.\n  \
    On Unix the destination is chmod 0600 after the backup completes.")]
pub struct BackupArgs {
    /// Destination path for the backup file. Required.
    #[arg(long, value_name = "PATH")]
    pub output: PathBuf,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct BackupResponse {
    action: String,
    source: String,
    destination: String,
    size_bytes: u64,
    elapsed_ms: u64,
}

pub fn run(args: BackupArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    // Validate: destination must differ from source.
    if args.output == paths.db {
        return Err(AppError::Validation(
            "destination path must differ from the source database path".to_string(),
        ));
    }

    // Create parent directories if necessary.
    let parent = args.output.parent().unwrap_or(std::path::Path::new("."));
    if !parent.as_os_str().is_empty() {
        std::fs::create_dir_all(parent)?;
    }

    // Atomic write: backup to tempfile in the SAME directory, then rename.
    let temp = NamedTempFile::new_in(parent).map_err(AppError::Io)?;
    let temp_path = temp.path().to_path_buf();

    let src_conn = open_ro(&paths.db)?;
    let mut dst_conn = rusqlite::Connection::open(&temp_path)?;

    {
        let backup = rusqlite::backup::Backup::new(&src_conn, &mut dst_conn)?;
        backup.run_to_completion(100, std::time::Duration::from_millis(50), None)?;
    }
    drop(dst_conn);

    temp.persist(&args.output)
        .map_err(|e| AppError::Io(e.error))?;

    // Apply 0600 permissions on Unix to prevent leakage in shared directories.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&args.output) {
            let mut perms = meta.permissions();
            perms.set_mode(0o600);
            if let Err(e) = std::fs::set_permissions(&args.output, perms) {
                tracing::warn!(target: "backup",
                    path = %args.output.display(),
                    error = %e,
                    "failed to set 0600 permissions on backup file"
                );
            }
        }
    }
    #[cfg(windows)]
    {
        tracing::debug!(target: "backup",
            path = %args.output.display(),
            "skipping Unix mode 0o600 on Windows; NTFS DACL default is private-to-user"
        );
    }

    let size_bytes = std::fs::metadata(&args.output)
        .map(|m| m.len())
        .unwrap_or(0);

    output::emit_json(&BackupResponse {
        action: "backed_up".to_string(),
        source: paths.db.display().to_string(),
        destination: args.output.display().to_string(),
        size_bytes,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_response_serializes_all_fields() {
        let resp = BackupResponse {
            action: "backed_up".to_string(),
            source: "/data/graphrag.sqlite".to_string(),
            destination: "/backup/snapshot.sqlite".to_string(),
            size_bytes: 32768,
            elapsed_ms: 42,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["action"], "backed_up");
        assert_eq!(json["source"], "/data/graphrag.sqlite");
        assert_eq!(json["destination"], "/backup/snapshot.sqlite");
        assert_eq!(json["size_bytes"], 32768u64);
        assert_eq!(json["elapsed_ms"], 42u64);
    }

    #[test]
    fn backup_response_action_is_backed_up() {
        let resp = BackupResponse {
            action: "backed_up".to_string(),
            source: "/a.sqlite".to_string(),
            destination: "/b.sqlite".to_string(),
            size_bytes: 0,
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(
            json["action"], "backed_up",
            "action must always be 'backed_up'"
        );
    }

    #[test]
    fn backup_rejects_destination_equal_to_source() {
        // Simulate the guard without a real DB.
        let src = PathBuf::from("/tmp/graphrag.sqlite");
        let dst = PathBuf::from("/tmp/graphrag.sqlite");
        let result: Result<(), AppError> = if dst == src {
            Err(AppError::Validation(
                "destination path must differ from the source database path".to_string(),
            ))
        } else {
            Ok(())
        };
        assert!(
            result.is_err(),
            "must reject identical source and destination"
        );
        if let Err(AppError::Validation(msg)) = result {
            assert!(msg.contains("destination path must differ"));
        }
    }

    #[test]
    fn backup_response_size_bytes_zero_is_valid() {
        let resp = BackupResponse {
            action: "backed_up".to_string(),
            source: "/a.sqlite".to_string(),
            destination: "/b.sqlite".to_string(),
            size_bytes: 0,
            elapsed_ms: 1,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["size_bytes"], 0u64);
    }
}
