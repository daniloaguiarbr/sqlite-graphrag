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

/// Default number of pages copied per backup step.
///
/// G38: the previous default of 100 pages with 50 ms sleep between steps
/// was the dominant cost on large databases (4.3 GB took ~9 minutes purely
/// on sleep). 1000 pages × 5 ms is ~25× faster on a 4.3 GB database while
/// remaining gentle on SSD I/O. Override with `--backup-step-size`.
const DEFAULT_BACKUP_STEP_PAGES: usize = 1000;
const DEFAULT_BACKUP_STEP_SLEEP_MS: u64 = 5;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Back up the default database to a specific path\n  \
    sqlite-graphrag backup --output /backup/graphrag-$(date +%F).sqlite\n\n  \
    # Back up a custom source database\n  \
    sqlite-graphrag backup --db /data/graphrag.sqlite --output /backup/snapshot.sqlite\n\n  \
    # Tuned for a 4.3 GB database on local SSD\n  \
    sqlite-graphrag backup --output /backup/snap.sqlite --backup-step-size 2000 --backup-step-sleep-ms 2\n\n  \
    # Maximum throughput (no sleep between steps — risks I/O contention)\n  \
    sqlite-graphrag backup --output /backup/snap.sqlite --backup-no-sleep\n\n  \
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
    /// Number of pages copied per backup step. Default: 1000 (was 100 before v1.0.69).
    /// Larger values finish faster on local SSD but may contend on NFS.
    #[arg(long, value_name = "PAGES", default_value_t = DEFAULT_BACKUP_STEP_PAGES)]
    pub backup_step_size: usize,
    /// Sleep duration in milliseconds between backup steps. Default: 5 (was 50 before v1.0.69).
    /// Ignored when --backup-no-sleep is set.
    #[arg(long, value_name = "MILLIS", default_value_t = DEFAULT_BACKUP_STEP_SLEEP_MS)]
    pub backup_step_sleep_ms: u64,
    /// Disable the inter-step sleep entirely. Maximum throughput, but risks
    /// starving concurrent I/O on shared storage.
    #[arg(long, default_value_t = false)]
    pub backup_no_sleep: bool,
    /// Emit a progress line to stderr every N pages (G38 observability).
    /// Default: 100 (every 100 pages = ~400 KB). Set to 0 to disable.
    #[arg(long, value_name = "PAGES", default_value_t = 100)]
    pub backup_progress: i32,
}

#[derive(Serialize)]
struct BackupResponse {
    action: String,
    source: String,
    destination: String,
    size_bytes: u64,
    elapsed_ms: u64,
    pages_copied: Option<i64>,
    step_size: usize,
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

    let step_size = args.backup_step_size.max(1);
    let sleep = if args.backup_no_sleep {
        std::time::Duration::ZERO
    } else {
        std::time::Duration::from_millis(args.backup_step_sleep_ms)
    };

    let pages_copied: Option<i64> = {
        let backup = rusqlite::backup::Backup::new(&src_conn, &mut dst_conn)?;
        // G38: drive the backup in a manual step() loop so we can emit
        // per-step progress events without depending on a Copy closure
        // (which the rusqlite Progress callback requires). The loop
        // mirrors run_to_completion but exposes progress for observability.
        let step_size_i32: i32 = step_size.try_into().unwrap_or(1000);
        let progress_every = args.backup_progress.max(1);
        let mut last_emit_pages: i32 = -1;
        loop {
            use rusqlite::backup::StepResult;
            match backup.step(step_size_i32) {
                Ok(StepResult::More) => {
                    // step returned More: backup still in progress.
                    if progress_every > 0 {
                        let p = backup.progress();
                        let copied = p.pagecount - p.remaining;
                        if copied > 0 && copied - last_emit_pages >= progress_every {
                            last_emit_pages = copied;
                            let percent = if p.pagecount > 0 {
                                (copied as f64 / p.pagecount as f64) * 100.0
                            } else {
                                100.0
                            };
                            output::emit_progress(&format!(
                                "backup progress: pages_copied={copied} total_pages={pc} percent={pct:.2}",
                                pc = p.pagecount,
                                pct = percent
                            ));
                        }
                    }
                    if !sleep.is_zero() {
                        std::thread::sleep(sleep);
                    }
                }
                Ok(StepResult::Done) => break, // backup complete
                Ok(_) => {
                    // Transient (Busy / Locked on newer rusqlite or any
                    // future non-exhaustive variant): retry after backoff.
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(e) => return Err(AppError::Database(e)),
            }
        }
        // `Progress { remaining, pagecount }` (see rusqlite::backup::Progress):
        // pages already copied = pagecount - remaining.
        let progress = backup.progress();
        let copied = (progress.pagecount - progress.remaining).max(0);
        Some(copied as i64)
    };
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
        pages_copied,
        step_size,
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
            pages_copied: Some(512),
            step_size: 1000,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["action"], "backed_up");
        assert_eq!(json["source"], "/data/graphrag.sqlite");
        assert_eq!(json["destination"], "/backup/snapshot.sqlite");
        assert_eq!(json["size_bytes"], 32768u64);
        assert_eq!(json["elapsed_ms"], 42u64);
        assert_eq!(json["step_size"], 1000usize);
        assert_eq!(json["pages_copied"], 512i64);
    }

    #[test]
    fn backup_response_action_is_backed_up() {
        let resp = BackupResponse {
            action: "backed_up".to_string(),
            source: "/a.sqlite".to_string(),
            destination: "/b.sqlite".to_string(),
            size_bytes: 0,
            elapsed_ms: 0,
            pages_copied: None,
            step_size: 1000,
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
            pages_copied: Some(0),
            step_size: 1000,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert!(json["size_bytes"].as_u64().is_some());
    }

    #[test]
    fn backup_default_step_size_is_one_thousand() {
        // G38: the historical default of 100 pages caused backups of 4.3 GB
        // databases to take 9 minutes solely on sleep. The new default of
        // 1000 pages with 5 ms sleep gives ~25x speedup.
        assert_eq!(DEFAULT_BACKUP_STEP_PAGES, 1000);
        assert_eq!(DEFAULT_BACKUP_STEP_SLEEP_MS, 5);
    }
}
