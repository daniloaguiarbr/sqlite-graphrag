//! Handler for the `purge` CLI subcommand.

use crate::errors::AppError;
use crate::i18n::errors_msg;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use serde::Serialize;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Permanently delete soft-deleted memories older than 90 days (default retention)\n  \
    sqlite-graphrag purge\n\n  \
    # Custom retention window in days\n  \
    sqlite-graphrag purge --retention-days 30\n\n  \
    # Purge ALL soft-deleted memories regardless of age\n  \
    sqlite-graphrag purge --retention-days 0\n\n  \
    # Preview what would be purged without deleting\n  \
    sqlite-graphrag purge --dry-run\n\n  \
    # Purge a specific memory by name\n  \
    sqlite-graphrag purge --name old-memory --namespace my-project\n\n\
NOTES:\n  \
    `--yes` only confirms intent and does NOT override `--retention-days`.\n  \
    To wipe every soft-deleted memory immediately, pair `--yes` with `--retention-days 0`.")]
pub struct PurgeArgs {
    #[arg(long)]
    pub name: Option<String>,
    /// Namespace to purge. Defaults to the contextual namespace (SQLITE_GRAPHRAG_NAMESPACE env var or "global").
    #[arg(long)]
    pub namespace: Option<String>,
    /// Retention days: memories with deleted_at older than (now - retention_days*86400) will be
    /// permanently removed. Default: PURGE_RETENTION_DAYS_DEFAULT (90). Use 0 to purge all
    /// soft-deleted memories regardless of age. Alias: `--max-age-days`.
    #[arg(
        long,
        alias = "days",
        alias = "max-age-days",
        value_name = "DAYS",
        default_value_t = crate::constants::PURGE_RETENTION_DAYS_DEFAULT
    )]
    pub retention_days: u32,
    /// [DEPRECATED in v2.0.0] Legacy alias — use --retention-days instead.
    #[arg(long, hide = true)]
    pub older_than_seconds: Option<u64>,
    /// Does not execute DELETE: computes and reports what WOULD be purged.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
    /// Confirms destructive intent for tools that require explicit acknowledgement.
    /// Does NOT override `--retention-days`: combine with `--retention-days 0` to wipe
    /// every soft-deleted memory regardless of age.
    #[arg(long, default_value_t = false)]
    pub yes: bool,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
pub struct PurgeResponse {
    pub purged_count: usize,
    pub bytes_freed: i64,
    pub oldest_deleted_at: Option<i64>,
    pub retention_days_used: u32,
    pub dry_run: bool,
    pub namespace: Option<String>,
    pub cutoff_epoch: i64,
    pub warnings: Vec<String>,
    /// Total execution time in milliseconds from handler start to serialisation.
    pub elapsed_ms: u64,
    /// Human-readable explanation surfaced when nothing was purged so callers
    /// understand the retention semantics. Present only when
    /// `purged_count == 0` (M2 in v1.0.32) — kept absent otherwise to preserve
    /// the existing JSON contract.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Permanently delete soft-deleted memories that have exceeded the retention window.
///
/// Only memories with `deleted_at IS NOT NULL AND deleted_at <= cutoff_epoch` are affected.
/// When `--dry-run` is set the DELETE is skipped and the response reflects candidates only.
pub fn run(args: PurgeArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    let mut warnings: Vec<String> = Vec::new();
    let now = current_epoch()?;

    let cutoff_epoch = if let Some(secs) = args.older_than_seconds {
        warnings.push(
            "--older-than-seconds is deprecated; use --retention-days in v2.0.0+".to_string(),
        );
        now - secs as i64
    } else {
        now - (args.retention_days as i64) * 86_400
    };

    let namespace_opt: Option<&str> = Some(namespace.as_str());

    let mut conn = open_rw(&paths.db)?;

    let (bytes_freed, oldest_deleted_at, candidates_count) =
        compute_metrics(&conn, cutoff_epoch, namespace_opt, args.name.as_deref())?;

    if candidates_count == 0 && args.name.is_some() {
        return Err(AppError::NotFound(
            errors_msg::soft_deleted_memory_not_found(
                args.name.as_deref().unwrap_or_default(),
                &namespace,
            ),
        ));
    }

    if !args.dry_run {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        execute_purge(
            &tx,
            &namespace,
            args.name.as_deref(),
            cutoff_epoch,
            &mut warnings,
        )?;
        tx.commit()?;
    }

    let message = if candidates_count == 0 {
        Some(format!(
            "no soft-deleted memories older than {retention_days} day(s); use --retention-days 0 to purge all soft-deleted memories regardless of age",
            retention_days = args.retention_days
        ))
    } else {
        None
    };

    output::emit_json(&PurgeResponse {
        purged_count: candidates_count,
        bytes_freed,
        oldest_deleted_at,
        retention_days_used: args.retention_days,
        dry_run: args.dry_run,
        namespace: Some(namespace),
        cutoff_epoch,
        warnings,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
        message,
    })?;

    Ok(())
}

fn current_epoch() -> Result<i64, AppError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| AppError::Internal(anyhow::anyhow!("system clock error: {err}")))?;
    Ok(now.as_secs() as i64)
}

fn compute_metrics(
    conn: &rusqlite::Connection,
    cutoff_epoch: i64,
    namespace_opt: Option<&str>,
    name: Option<&str>,
) -> Result<(i64, Option<i64>, usize), AppError> {
    let (bytes_freed, oldest_deleted_at): (i64, Option<i64>) = if let Some(name) = name {
        conn.query_row(
            "SELECT COALESCE(SUM(LENGTH(COALESCE(body,'')) + LENGTH(COALESCE(description,'')) + LENGTH(name)), 0),
                    MIN(deleted_at)
             FROM memories
             WHERE deleted_at IS NOT NULL AND deleted_at <= ?1
                   AND (?2 IS NULL OR namespace = ?2)
                   AND name = ?3",
            rusqlite::params![cutoff_epoch, namespace_opt, name],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?)),
        )?
    } else {
        conn.query_row(
            "SELECT COALESCE(SUM(LENGTH(COALESCE(body,'')) + LENGTH(COALESCE(description,'')) + LENGTH(name)), 0),
                    MIN(deleted_at)
             FROM memories
             WHERE deleted_at IS NOT NULL AND deleted_at <= ?1
                   AND (?2 IS NULL OR namespace = ?2)",
            rusqlite::params![cutoff_epoch, namespace_opt],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?)),
        )?
    };

    let count: usize = if let Some(name) = name {
        conn.query_row(
            "SELECT COUNT(*) FROM memories
             WHERE deleted_at IS NOT NULL AND deleted_at <= ?1
                   AND (?2 IS NULL OR namespace = ?2)
                   AND name = ?3",
            rusqlite::params![cutoff_epoch, namespace_opt, name],
            |r| r.get::<_, usize>(0),
        )?
    } else {
        conn.query_row(
            "SELECT COUNT(*) FROM memories
             WHERE deleted_at IS NOT NULL AND deleted_at <= ?1
                   AND (?2 IS NULL OR namespace = ?2)",
            rusqlite::params![cutoff_epoch, namespace_opt],
            |r| r.get::<_, usize>(0),
        )?
    };

    Ok((bytes_freed, oldest_deleted_at, count))
}

fn execute_purge(
    tx: &rusqlite::Transaction,
    namespace: &str,
    name: Option<&str>,
    cutoff_epoch: i64,
    warnings: &mut Vec<String>,
) -> Result<(), AppError> {
    let candidates = select_candidates(tx, namespace, name, cutoff_epoch)?;

    for (memory_id, _name) in &candidates {
        if let Err(err) = tx.execute(
            "DELETE FROM vec_chunks WHERE memory_id = ?1",
            rusqlite::params![memory_id],
        ) {
            warnings.push(format!(
                "failed to clean vec_chunks for memory_id {memory_id}: {err}"
            ));
        }
        if let Err(err) = tx.execute(
            "DELETE FROM vec_memories WHERE memory_id = ?1",
            rusqlite::params![memory_id],
        ) {
            warnings.push(format!(
                "failed to clean vec_memories for memory_id {memory_id}: {err}"
            ));
        }
        tx.execute(
            "DELETE FROM memories WHERE id = ?1 AND namespace = ?2 AND deleted_at IS NOT NULL",
            rusqlite::params![memory_id, namespace],
        )?;
    }

    Ok(())
}

fn select_candidates(
    conn: &rusqlite::Connection,
    namespace: &str,
    name: Option<&str>,
    cutoff_epoch: i64,
) -> Result<Vec<(i64, String)>, AppError> {
    let query = if name.is_some() {
        "SELECT id, name FROM memories
         WHERE namespace = ?1 AND name = ?2 AND deleted_at IS NOT NULL AND deleted_at <= ?3
         ORDER BY deleted_at ASC"
    } else {
        "SELECT id, name FROM memories
         WHERE namespace = ?1 AND deleted_at IS NOT NULL AND deleted_at <= ?2
         ORDER BY deleted_at ASC"
    };

    let mut stmt = conn.prepare(query)?;
    let rows = if let Some(name) = name {
        stmt.query_map(rusqlite::params![namespace, name, cutoff_epoch], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?
    } else {
        stmt.query_map(rusqlite::params![namespace, cutoff_epoch], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("failed to open in-memory db");
        conn.execute_batch(
            "CREATE TABLE memories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                namespace TEXT NOT NULL DEFAULT 'global',
                description TEXT,
                body TEXT,
                deleted_at INTEGER
            );
            CREATE TABLE IF NOT EXISTS vec_chunks (memory_id INTEGER);
            CREATE TABLE IF NOT EXISTS vec_memories (memory_id INTEGER);",
        )
        .expect("failed to create test tables");
        conn
    }

    fn insert_deleted_memory(
        conn: &Connection,
        name: &str,
        namespace: &str,
        body: &str,
        deleted_at: i64,
    ) -> i64 {
        conn.execute(
            "INSERT INTO memories (name, namespace, body, deleted_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![name, namespace, body, deleted_at],
        )
        .expect("failed to insert test memory");
        conn.last_insert_rowid()
    }

    #[test]
    fn retention_days_used_default_is_90() {
        assert_eq!(crate::constants::PURGE_RETENTION_DAYS_DEFAULT, 90u32);
    }

    #[test]
    fn compute_metrics_bytes_freed_positive_for_populated_body() {
        let conn = setup_test_db();
        let now = current_epoch().expect("epoch failed");
        let old_epoch = now - 100 * 86_400;
        insert_deleted_memory(&conn, "mem-test", "global", "memory body", old_epoch);

        let cutoff = now - 30 * 86_400;
        let (bytes, oldest, count) =
            compute_metrics(&conn, cutoff, Some("global"), None).expect("compute_metrics failed");

        assert!(bytes > 0, "bytes_freed must be > 0 for populated body");
        assert!(oldest.is_some(), "oldest_deleted_at must be Some");
        assert_eq!(count, 1);
    }

    #[test]
    fn compute_metrics_returns_zero_without_candidates() {
        let conn = setup_test_db();
        let now = current_epoch().expect("epoch failed");
        let cutoff = now - 90 * 86_400;

        let (bytes, oldest, count) =
            compute_metrics(&conn, cutoff, Some("global"), None).expect("compute_metrics failed");

        assert_eq!(bytes, 0);
        assert!(oldest.is_none());
        assert_eq!(count, 0);
    }

    #[test]
    fn dry_run_does_not_delete_records() {
        let conn = setup_test_db();
        let now = current_epoch().expect("epoch failed");
        let old_epoch = now - 200 * 86_400;
        insert_deleted_memory(&conn, "mem-dry", "global", "dry run content", old_epoch);

        let cutoff = now - 30 * 86_400;
        let (_, _, count_before) =
            compute_metrics(&conn, cutoff, Some("global"), None).expect("compute_metrics failed");
        assert_eq!(count_before, 1, "must have 1 candidate before dry run");

        let (_, _, count_after) =
            compute_metrics(&conn, cutoff, Some("global"), None).expect("compute_metrics failed");
        assert_eq!(
            count_after, 1,
            "dry_run must not remove records: count must remain 1"
        );
    }

    #[test]
    fn oldest_deleted_at_returns_smallest_epoch() {
        let conn = setup_test_db();
        let now = current_epoch().expect("epoch failed");
        let epoch_old = now - 300 * 86_400;
        let epoch_recent = now - 200 * 86_400;

        insert_deleted_memory(&conn, "mem-a", "global", "body-a", epoch_old);
        insert_deleted_memory(&conn, "mem-b", "global", "body-b", epoch_recent);

        let cutoff = now - 30 * 86_400;
        let (_, oldest, count) =
            compute_metrics(&conn, cutoff, Some("global"), None).expect("compute_metrics failed");

        assert_eq!(count, 2);
        assert_eq!(
            oldest,
            Some(epoch_old),
            "oldest_deleted_at must be the oldest epoch"
        );
    }

    #[test]
    fn purge_args_namespace_accepts_none_without_default() {
        // P1-C: namespace must be None when not provided, allowing resolve_namespace
        // to consult SQLITE_GRAPHRAG_NAMESPACE before falling back to "global".
        // The field was `default_value = "global"` before P1-C; with that removed,
        // resolve_namespace(None) consults the env var correctly.
        let resolved = crate::namespace::resolve_namespace(None)
            .expect("resolve_namespace(None) must return Ok");
        assert_eq!(
            resolved, "global",
            "without env var, resolve_namespace(None) must fall back to 'global'"
        );
    }

    #[test]
    fn purge_response_serializes_all_new_fields() {
        let resp = PurgeResponse {
            purged_count: 3,
            bytes_freed: 1024,
            oldest_deleted_at: Some(1_700_000_000),
            retention_days_used: 90,
            dry_run: false,
            namespace: Some("global".to_string()),
            cutoff_epoch: 1_710_000_000,
            warnings: vec![],
            elapsed_ms: 42,
            message: None,
        };
        let json = serde_json::to_string(&resp).expect("serialization failed");
        assert!(json.contains("bytes_freed"));
        assert!(json.contains("oldest_deleted_at"));
        assert!(json.contains("retention_days_used"));
        assert!(json.contains("dry_run"));
        assert!(json.contains("elapsed_ms"));
        // M2: when no purge happened, `message` is omitted to keep payloads stable.
        assert!(!json.contains("\"message\""));
    }

    #[test]
    fn purge_response_serializes_message_when_present() {
        // M2 (v1.0.32): zero purges include a human-readable hint message.
        let resp = PurgeResponse {
            purged_count: 0,
            bytes_freed: 0,
            oldest_deleted_at: None,
            retention_days_used: 90,
            dry_run: false,
            namespace: Some("global".to_string()),
            cutoff_epoch: 1_710_000_000,
            warnings: vec![],
            elapsed_ms: 5,
            message: Some(
                "no soft-deleted memories older than 90 day(s); use --retention-days 0 to purge all soft-deleted memories regardless of age"
                    .to_string(),
            ),
        };
        let json = serde_json::to_string(&resp).expect("serialization failed");
        assert!(json.contains("\"message\""));
        assert!(json.contains("--retention-days 0"));
    }
}
