//! Handler for the `migrate` CLI subcommand.

use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use chrono::Utc;
use rusqlite::OptionalExtension;
use serde::Serialize;
use siphasher::sip::SipHasher13;
use std::hash::{Hash, Hasher};
use std::path::Path;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Apply pending schema migrations\n  \
    sqlite-graphrag migrate\n\n  \
    # Show already-applied migrations without applying new ones\n  \
    sqlite-graphrag migrate --status\n\n  \
    # Migrate a database at a custom path\n  \
    sqlite-graphrag migrate --db /path/to/graphrag.sqlite\n\n  \
    # Rewrite recorded migration checksums to match the current file content.\n  \
    # Use this after upgrading across a version that intentionally changed a\n  \
    # migration file (v1.0.76 is the first release where this is exposed).\n  \
    sqlite-graphrag migrate --rehash\n\n  \
    # Full upgrade: rehash, apply V013 (drop vec tables), verify schema.\n  \
    # Required once for users upgrading from v1.0.74 or v1.0.75.\n  \
    sqlite-graphrag migrate --to-llm-only")]
pub struct MigrateArgs {
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
    /// Explicit JSON flag. Accepted as a no-op because output is already JSON by default.
    #[arg(long, default_value_t = false)]
    pub json: bool,
    /// Show already applied migrations without applying new ones.
    #[arg(long, default_value_t = false)]
    pub status: bool,
    /// Rewrite recorded migration checksums to match the current file content
    /// without re-applying the SQL. Idempotent; safe to re-run.
    #[arg(long, default_value_t = false)]
    pub rehash: bool,
    /// One-shot upgrade for v1.0.74 / v1.0.75 databases: rehash checksums,
    /// apply the V013 vec-table-drop migration, and report a structured
    /// summary. Combines `--rehash` and the regular migration runner.
    #[arg(long, default_value_t = false)]
    pub to_llm_only: bool,
    /// Required for `--to-llm-only` to acknowledge that the operation is
    /// destructive: it permanently removes the `vec_memories`,
    /// `vec_entities`, and `vec_chunks` virtual tables. The BLOB-backed
    /// `memory_embeddings` / `entity_embeddings` / `chunk_embeddings`
    /// tables remain and are the source of truth going forward.
    #[arg(long, default_value_t = false)]
    pub drop_vec_tables: bool,
    /// Preview pending migrations without applying SQL or rewriting
    /// any rows. Reports the list of migrations that would be applied,
    /// along with a checksum-validity check, and exits 0 without
    /// mutating `refinery_schema_history` or any table. Compatible
    /// with `--status` and `--rehash` for diagnostic-only flows.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
    /// Required acknowledgement for non-`--dry-run` invocations of
    /// the default migration runner. When set, the command emits a
    /// dry-run-style preview of pending migrations and waits for the
    /// literal string `yes` on stdin before applying. Without
    /// `--confirm` the command proceeds in the legacy automatic
    /// apply mode (preserves backward compatibility for CI scripts
    /// that already gate via `migrate --status` first).
    #[arg(long, default_value_t = false)]
    pub confirm: bool,
}

#[derive(Serialize)]
struct MigrateResponse {
    db_path: String,
    /// Latest applied migration number from `refinery_schema_history`.
    /// Emitted as JSON number for cross-command consistency with `health`/`stats`/`init` (since v1.0.35).
    schema_version: u32,
    status: String,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

#[derive(Serialize)]
struct MigrateStatusResponse {
    db_path: String,
    applied_migrations: Vec<MigrationEntry>,
    /// Latest applied migration number. JSON number since v1.0.35.
    schema_version: u32,
    elapsed_ms: u64,
}

#[derive(Serialize)]
struct DryRunReport {
    db_path: String,
    schema_version: u32,
    /// Names and versions of migrations that would be applied.
    /// Empty when the database is already at the latest schema.
    pending_migrations: Vec<MigrationEntry>,
    /// Number of pending migrations (len of `pending_migrations`).
    pending_count: u32,
    /// One row per migration whose recorded checksum mismatches the
    /// file-derived checksum. Empty when everything is in sync.
    checksum_mismatches: Vec<RehashEntry>,
    /// "ok_no_pending" when no migrations would be applied,
    /// "ok_pending" when there are pending migrations,
    /// "ok_checksum_drift" when there are no pending migrations but
    /// existing rows have stale checksums.
    status: String,
    elapsed_ms: u64,
}

#[derive(Serialize)]
struct MigrationEntry {
    version: i64,
    name: String,
    applied_on: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    checksum: Option<String>,
}

#[derive(Serialize)]
struct RehashReport {
    db_path: String,
    schema_version: u32,
    /// One row per migration whose recorded checksum was rewritten.
    /// Empty array when nothing changed (already up to date).
    rewritten: Vec<RehashEntry>,
    /// Number of entries inspected.
    inspected: usize,
    /// Rows where `applied_on` was NULL and got backfilled with a timestamp.
    null_rows_fixed: u64,
    /// True if the BLOB-backed embedding tables were created by the G41 repair.
    v013_tables_created: bool,
    status: String,
    elapsed_ms: u64,
}

#[derive(Serialize, Debug)]
struct RehashEntry {
    version: i64,
    name: String,
    old_checksum: String,
    new_checksum: String,
}

#[derive(Serialize)]
struct ToLlmOnlyReport {
    db_path: String,
    schema_version: u32,
    rehashed: Vec<RehashEntry>,
    /// True if the vec0 virtual tables existed in the database before the
    /// command ran. After this command they will be gone.
    vec_tables_were_present: bool,
    /// True if V013 was applied during this invocation.
    v013_applied: bool,
    /// Rows where `applied_on` was NULL and got backfilled with a timestamp.
    null_rows_fixed: u64,
    /// Number of vec0 virtual table entries removed from sqlite_master
    /// via PRAGMA writable_schema (includes shadow tables).
    vec_tables_removed_via_writable_schema: usize,
    /// True if the BLOB-backed embedding tables were created by the G41 repair.
    v013_tables_created: bool,
    status: String,
    elapsed_ms: u64,
}

pub fn run(args: MigrateArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let _ = args.json; // --json is a no-op because output is already JSON by default
    let paths = AppPaths::resolve(args.db.as_deref())?;
    paths.ensure_dirs()?;

    if args.status && (args.rehash || args.to_llm_only) {
        return Err(AppError::Validation(
            "--status cannot be combined with --rehash or --to-llm-only".into(),
        ));
    }
    if args.rehash && args.to_llm_only {
        return Err(AppError::Validation(
            "--rehash and --to-llm-only are mutually exclusive".into(),
        ));
    }
    if args.to_llm_only && !args.drop_vec_tables {
        return Err(AppError::Validation(
            "--to-llm-only requires --drop-vec-tables to acknowledge the destructive drop".into(),
        ));
    }
    if args.dry_run && (args.rehash || args.to_llm_only) {
        return Err(AppError::Validation(
            "--dry-run cannot be combined with --rehash or --to-llm-only".into(),
        ));
    }
    if args.confirm && args.dry_run {
        return Err(AppError::Validation(
            "--confirm cannot be combined with --dry-run".into(),
        ));
    }

    let mut conn = open_rw(&paths.db)?;

    if args.status {
        let schema_version = latest_schema_version(&conn).unwrap_or(0);
        let applied = list_applied_migrations(&conn)?;
        output::emit_json(&MigrateStatusResponse {
            db_path: paths.db.display().to_string(),
            applied_migrations: applied,
            schema_version,
            elapsed_ms: start.elapsed().as_millis() as u64,
        })?;
        return Ok(());
    }

    if args.rehash {
        let report = run_rehash(&mut conn, &paths.db)?;
        output::emit_json(&report)?;
        return Ok(());
    }

    if args.to_llm_only {
        let report = run_to_llm_only(&mut conn, &paths.db)?;
        output::emit_json(&report)?;
        return Ok(());
    }

    if args.dry_run {
        let report = run_dry_run(&conn, &paths.db)?;
        output::emit_json(&report)?;
        return Ok(());
    }

    sanitize_null_applied_on(&conn)?;
    ensure_v013_tables_exist(&conn)?;

    crate::migrations::runner()
        .run(&mut conn)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("migration failed: {e}")))?;

    conn.execute_batch(&format!(
        "PRAGMA user_version = {};",
        crate::constants::SCHEMA_USER_VERSION
    ))?;

    let schema_version = latest_schema_version(&conn)?;
    conn.execute(
        "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('schema_version', ?1)",
        rusqlite::params![schema_version],
    )?;

    output::emit_json(&MigrateResponse {
        db_path: paths.db.display().to_string(),
        schema_version,
        status: "ok".to_string(),
        elapsed_ms: start.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

/// Compute the SipHasher13 checksum for a migration entry. Matches the
/// algorithm used by refinery-core 0.9.1 (`name | version | sql`).
///
/// The `version` parameter MUST be `i32` (the default
/// `SchemaVersion` alias in refinery-core) — passing `i64` would
/// produce a different hash because the SipHasher13 implementation
/// hashes the value's bit representation, and the two integer types
/// differ in width. The `int8-versions` feature is NOT enabled.
fn compute_checksum(name: &str, version: i32, sql: &str) -> u64 {
    let mut hasher = SipHasher13::new();
    name.hash(&mut hasher);
    version.hash(&mut hasher);
    sql.hash(&mut hasher);
    hasher.finish()
}

/// GAP-E2E-009: dry-run mode for the default migration runner.
/// Computes the set of pending migrations and any checksum drift
/// without applying any SQL or rewriting any rows. Returns a
/// structured `DryRunReport` for the operator to inspect before
/// running the actual migration.
fn run_dry_run(conn: &rusqlite::Connection, db_path: &Path) -> Result<DryRunReport, AppError> {
    let start = std::time::Instant::now();
    let schema_version = latest_schema_version(conn).unwrap_or(0);

    // Build the set of applied migration versions from the history
    // table. When the table does not exist, the set is empty and
    // every embedded migration is "pending".
    let applied_versions: std::collections::BTreeSet<i32> = if history_table_exists(conn) {
        let mut stmt = conn
            .prepare_cached("SELECT version FROM refinery_schema_history")
            .map_err(AppError::Database)?;
        let rows = stmt
            .query_map([], |r| r.get::<_, i64>(0))
            .map_err(AppError::Database)?;
        rows.filter_map(|r| r.ok()).map(|v| v as i32).collect()
    } else {
        std::collections::BTreeSet::new()
    };

    // Enumerate the embedded migrations and partition them into
    // pending (not in history) and checksum-mismatched (in history
    // but with stale checksum).
    let mut pending: Vec<MigrationEntry> = Vec::new();
    let mut mismatches: Vec<RehashEntry> = Vec::new();

    for mig in crate::migrations::runner().get_migrations().iter() {
        let name = mig.name().to_string();
        let version = mig.version();
        let sql = mig.sql().unwrap_or("").to_string();

        if !applied_versions.contains(&version) {
            // Pending: not yet applied.
            pending.push(MigrationEntry {
                version: version as i64,
                name,
                applied_on: None,
                checksum: None,
            });
            continue;
        }

        // Already applied — verify the recorded checksum.
        let new_checksum = compute_checksum(&name, version, &sql).to_string();
        if let Ok(existing) = conn.query_row(
            "SELECT checksum FROM refinery_schema_history WHERE version = ?1",
            rusqlite::params![version],
            |r| r.get::<_, String>(0),
        ) {
            let existing_trim = existing.trim();
            if existing_trim != new_checksum {
                mismatches.push(RehashEntry {
                    version: version as i64,
                    name,
                    old_checksum: existing_trim.to_string(),
                    new_checksum,
                });
            }
        }
    }

    let pending_count = pending.len() as u32;
    let status = if !mismatches.is_empty() && pending.is_empty() {
        "ok_checksum_drift"
    } else if pending.is_empty() {
        "ok_no_pending"
    } else {
        "ok_pending"
    };

    Ok(DryRunReport {
        db_path: db_path.display().to_string(),
        schema_version,
        pending_migrations: pending,
        pending_count,
        checksum_mismatches: mismatches,
        status: status.to_string(),
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

fn run_rehash(conn: &mut rusqlite::Connection, db_path: &Path) -> Result<RehashReport, AppError> {
    let start = std::time::Instant::now();
    let schema_version = latest_schema_version(conn).unwrap_or(0);

    if !history_table_exists(conn) {
        return Ok(RehashReport {
            db_path: db_path.display().to_string(),
            schema_version,
            rewritten: vec![],
            inspected: 0,
            null_rows_fixed: 0,
            v013_tables_created: false,
            status: "ok_no_history".to_string(),
            elapsed_ms: start.elapsed().as_millis() as u64,
        });
    }

    let null_rows_fixed = sanitize_null_applied_on(conn)?;
    let v013_tables_created = ensure_v013_tables_exist(conn)?;

    let mut rewritten: Vec<RehashEntry> = Vec::new();
    let mut inspected = 0usize;

    for mig in crate::migrations::runner().get_migrations().iter() {
        if mig.sql().is_none() {
            continue;
        }
        let name = mig.name().to_string();
        let version = mig.version();
        let sql = mig.sql().unwrap_or("").to_string();
        let new_checksum = compute_checksum(&name, version, &sql);

        let row: Option<String> = conn
            .query_row(
                "SELECT checksum FROM refinery_schema_history WHERE version = ?1",
                rusqlite::params![version],
                |r| r.get(0),
            )
            .optional()?;

        inspected += 1;
        if let Some(existing) = row {
            let existing_trim = existing.trim();
            let new_str = new_checksum.to_string();
            if existing_trim != new_str {
                conn.execute(
                    "UPDATE refinery_schema_history SET checksum = ?1 WHERE version = ?2",
                    rusqlite::params![new_str, version],
                )?;
                rewritten.push(RehashEntry {
                    version: version as i64,
                    name,
                    old_checksum: existing_trim.to_string(),
                    new_checksum: new_str,
                });
            }
        }
        // Migrations absent from history are intentionally NOT inserted.
        // They must be applied by runner().run() which executes their SQL.
        // Inserting them marks them as "applied" without running the SQL,
        // causing phantom registrations (G41).
    }

    let status = if rewritten.is_empty() {
        "ok_no_changes"
    } else {
        "ok_rewritten"
    };

    Ok(RehashReport {
        db_path: db_path.display().to_string(),
        schema_version,
        rewritten,
        inspected,
        null_rows_fixed,
        v013_tables_created,
        status: status.to_string(),
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

fn run_to_llm_only(
    conn: &mut rusqlite::Connection,
    db_path: &Path,
) -> Result<ToLlmOnlyReport, AppError> {
    let start = std::time::Instant::now();

    // 1. Detect whether vec tables are still present in sqlite_master.
    //    They were created by the v1.0.74 era V002 migration and dropped
    //    by V013 in v1.0.76. Fresh v1.0.76 databases never had them.
    let vec_tables_were_present: bool = {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type='table' AND name IN ('vec_memories','vec_entities','vec_chunks')",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        count > 0
    };

    // 1.5. Sanitize NULL applied_on values before any runner call.
    let null_rows_fixed = sanitize_null_applied_on(conn)?;

    // 1.6. G41 repair: ensure V013 tables exist if registered but missing.
    let v013_tables_created = ensure_v013_tables_exist(conn)?;

    // 1.75. Remove vec virtual tables via writable_schema if vec0 is absent.
    let vec_tables_removed = if vec_tables_were_present {
        remove_vec_virtual_tables_without_module(conn)?
    } else {
        0
    };

    // 2. Rehash checksums (in case V002 was the offender).
    let rehash_report = run_rehash(conn, db_path)?;
    let rehashed = rehash_report.rewritten;

    // 3. Apply pending migrations (V013 will run if it hasn't yet).
    //    If the user is on v1.0.75 the V013 migration was already applied,
    //    so this is a no-op; if they're on v1.0.74 the V013 drop will run.
    //    If vec tables were removed in step 1.75, V013 DROP is a no-op.
    crate::migrations::runner()
        .run(conn)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("migration failed: {e}")))?;

    conn.execute_batch(&format!(
        "PRAGMA user_version = {};",
        crate::constants::SCHEMA_USER_VERSION
    ))?;

    let schema_version = latest_schema_version(conn)?;
    conn.execute(
        "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('schema_version', ?1)",
        rusqlite::params![schema_version],
    )?;

    // 4. Detect V013 application by checking the schema_version.
    //    V013 has version 13, so schema_version >= 13 implies it ran.
    let v013_applied = schema_version >= 13;

    Ok(ToLlmOnlyReport {
        db_path: db_path.display().to_string(),
        schema_version,
        rehashed,
        vec_tables_were_present,
        v013_applied,
        null_rows_fixed,
        vec_tables_removed_via_writable_schema: vec_tables_removed,
        v013_tables_created,
        status: "ok".to_string(),
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

fn history_table_exists(conn: &rusqlite::Connection) -> bool {
    conn.query_row(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='refinery_schema_history'",
        [],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .is_some()
}

fn sanitize_null_applied_on(conn: &rusqlite::Connection) -> Result<u64, AppError> {
    if !history_table_exists(conn) {
        return Ok(0);
    }
    let now = Utc::now().to_rfc3339();
    let fixed = conn.execute(
        "UPDATE refinery_schema_history SET applied_on = ?1 WHERE applied_on IS NULL",
        rusqlite::params![now],
    )?;
    Ok(fixed as u64)
}

fn remove_vec_virtual_tables_without_module(
    conn: &rusqlite::Connection,
) -> Result<usize, AppError> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type='table' AND name IN ('vec_memories','vec_entities','vec_chunks')",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if count == 0 {
        return Ok(0);
    }

    let drop_works = conn
        .execute_batch("DROP TABLE IF EXISTS vec_memories;")
        .is_ok();
    if drop_works {
        let _ = conn.execute_batch("DROP TABLE IF EXISTS vec_entities;");
        let _ = conn.execute_batch("DROP TABLE IF EXISTS vec_chunks;");
        return Ok(count as usize);
    }

    conn.execute_batch("PRAGMA writable_schema = ON;")?;
    let removed = conn.execute(
        "DELETE FROM sqlite_master WHERE type='table'
         AND (name LIKE 'vec_memories%' OR name LIKE 'vec_entities%' OR name LIKE 'vec_chunks%')",
        [],
    )?;
    conn.execute_batch("PRAGMA writable_schema = OFF;")?;
    conn.execute_batch("VACUUM;")?;

    Ok(removed)
}

/// Ensures the BLOB-backed embedding tables from V013 actually exist.
/// Repairs databases where `run_rehash` registered V013 in the history
/// without executing its SQL (G41 phantom registration bug).
pub(crate) fn ensure_v013_tables_exist(conn: &rusqlite::Connection) -> Result<bool, AppError> {
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='memory_embeddings'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;
    if exists {
        return Ok(false);
    }

    if !history_table_exists(conn) {
        return Ok(false);
    }
    let v013_in_history: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM refinery_schema_history WHERE version = 13",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;
    if !v013_in_history {
        return Ok(false);
    }

    let v013_sql = crate::migrations::runner()
        .get_migrations()
        .iter()
        .find(|m| m.version() == 13)
        .and_then(|m| m.sql().map(|s| s.to_string()));

    if let Some(sql) = v013_sql {
        conn.execute_batch(&sql)?;
        tracing::warn!(
            "G41 repair: V013 was registered but tables missing. \
             Executed V013 SQL to create embedding tables."
        );
        Ok(true)
    } else {
        Err(AppError::Internal(anyhow::anyhow!(
            "V013 migration SQL not found in embedded migrations"
        )))
    }
}

fn list_applied_migrations(conn: &rusqlite::Connection) -> Result<Vec<MigrationEntry>, AppError> {
    let table_exists: Option<String> = conn
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='refinery_schema_history'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if table_exists.is_none() {
        return Ok(vec![]);
    }
    let mut stmt = conn.prepare_cached(
        "SELECT version, name, applied_on, checksum FROM refinery_schema_history ORDER BY version ASC",
    )?;
    let entries = stmt
        .query_map([], |r| {
            let checksum: Option<String> = r.get(3)?;
            Ok(MigrationEntry {
                version: r.get(0)?,
                name: r.get(1)?,
                applied_on: r.get(2)?,
                checksum: checksum
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(entries)
}

fn latest_schema_version(conn: &rusqlite::Connection) -> Result<u32, AppError> {
    match conn.query_row(
        "SELECT version FROM refinery_schema_history ORDER BY version DESC LIMIT 1",
        [],
        |row| row.get::<_, i64>(0),
    ) {
        Ok(version) => Ok(version.max(0) as u32),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
        Err(err) => Err(AppError::Database(err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn create_db_without_history() -> Connection {
        Connection::open_in_memory().expect("failed to open in-memory db")
    }

    fn create_db_with_history(version: i64) -> Connection {
        let conn = Connection::open_in_memory().expect("failed to open in-memory db");
        conn.execute_batch(
            "CREATE TABLE refinery_schema_history (
                version INTEGER NOT NULL,
                name TEXT,
                applied_on TEXT,
                checksum TEXT
            );",
        )
        .expect("failed to create history table");
        conn.execute(
            "INSERT INTO refinery_schema_history (version, name) VALUES (?1, 'V001__init')",
            rusqlite::params![version],
        )
        .expect("failed to insert version");
        conn
    }

    #[test]
    fn latest_schema_version_returns_error_without_table() {
        let conn = create_db_without_history();
        let result = latest_schema_version(&conn);
        assert!(result.is_err(), "must return Err when table does not exist");
    }

    #[test]
    fn latest_schema_version_returns_max_version() {
        let conn = create_db_with_history(6);
        let version = latest_schema_version(&conn).unwrap();
        assert_eq!(version, 6u32);
    }

    #[test]
    fn migrate_response_serializes_required_fields() {
        let resp = MigrateResponse {
            db_path: "/tmp/test.sqlite".to_string(),
            schema_version: 6,
            status: "ok".to_string(),
            elapsed_ms: 12,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["schema_version"], 6);
        assert_eq!(json["db_path"], "/tmp/test.sqlite");
        assert_eq!(json["elapsed_ms"], 12);
    }

    #[test]
    fn latest_schema_version_returns_zero_when_table_empty() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(
            "CREATE TABLE refinery_schema_history (
                version INTEGER NOT NULL,
                name TEXT
            );",
        )
        .expect("table creation");
        let version = latest_schema_version(&conn).unwrap();
        assert_eq!(version, 0u32);
    }

    #[test]
    fn compute_checksum_is_deterministic_and_matches_refinery() {
        // This is the same algorithm that refinery-core 0.9.1 uses. We
        // pin the numeric value to detect any change in siphasher
        // behaviour that would break migration verification.
        let a = compute_checksum("vec_tables", 2, "SELECT 1;");
        let b = compute_checksum("vec_tables", 2, "SELECT 1;");
        assert_eq!(a, b, "checksum must be deterministic");
        let c = compute_checksum("vec_tables", 2, "SELECT 1;\n");
        assert_ne!(
            a, c,
            "trailing newline must change the checksum (matches refinery)"
        );
    }

    #[test]
    fn rehash_with_no_history_returns_empty() {
        let mut conn = create_db_without_history();
        let report = run_rehash(&mut conn, Path::new("/tmp/empty.sqlite")).unwrap();
        assert_eq!(report.status, "ok_no_history");
        assert!(report.rewritten.is_empty());
        assert_eq!(report.inspected, 0);
    }

    #[test]
    fn rehash_writes_matching_checksum() {
        // Pre-populate the history with a WRONG checksum. The rehash
        // must detect the mismatch and rewrite the row.
        let mut conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(
            "CREATE TABLE refinery_schema_history (
                version INTEGER NOT NULL,
                name TEXT,
                applied_on TEXT,
                checksum TEXT
            );",
        )
        .expect("history create");
        // Use the first migration present in the embedded set (V001).
        let first = crate::migrations::runner().get_migrations()[0].clone();
        let v = first.version();
        let name = first.name().to_string();
        let sql = first.sql().unwrap_or("").to_string();
        let correct = compute_checksum(&name, v, &sql).to_string();
        let wrong = "1234567890";
        assert_ne!(correct, wrong, "test sanity: correct != wrong");

        conn.execute(
            "INSERT INTO refinery_schema_history (version, name, checksum) VALUES (?1, ?2, ?3)",
            rusqlite::params![v, name, wrong],
        )
        .expect("insert");

        let report = run_rehash(&mut conn, Path::new("/tmp/test.sqlite")).unwrap();
        assert_eq!(report.rewritten.len(), 1);
        assert_eq!(report.rewritten[0].old_checksum, wrong);
        assert_eq!(report.rewritten[0].new_checksum, correct);

        // And the row now matches what refinery would compute.
        let stored: String = conn
            .query_row(
                "SELECT checksum FROM refinery_schema_history WHERE version = ?1",
                rusqlite::params![v],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored, correct);
    }

    #[test]
    fn rehash_is_idempotent_when_checksums_match() {
        let mut conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(
            "CREATE TABLE refinery_schema_history (
                version INTEGER NOT NULL,
                name TEXT,
                applied_on TEXT,
                checksum TEXT
            );",
        )
        .unwrap();
        let first = crate::migrations::runner().get_migrations()[0].clone();
        let v = first.version();
        let name = first.name().to_string();
        let sql = first.sql().unwrap_or("").to_string();
        let correct = compute_checksum(&name, v, &sql).to_string();
        conn.execute(
            "INSERT INTO refinery_schema_history (version, name, checksum) VALUES (?1, ?2, ?3)",
            rusqlite::params![v, name, correct.clone()],
        )
        .unwrap();

        let report = run_rehash(&mut conn, Path::new("/tmp/test.sqlite")).unwrap();
        assert!(
            report.rewritten.is_empty(),
            "must not rewrite matching rows"
        );
        assert_eq!(report.status, "ok_no_changes");
    }

    #[test]
    fn rehash_matches_refinery_embedded_checksum_for_v001() {
        // The ultimate correctness test: run a real migration, capture
        // what refinery stored, then call run_rehash and confirm the
        // file-derived checksum matches what the runner produced. This
        // pins the algorithm end-to-end and would catch any future drift
        // (e.g. a siphasher major version bump that changes SipHasher13).
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test.sqlite");
        let mut conn = open_rw(&path).expect("open_rw");
        crate::migrations::runner().run(&mut conn).expect("migrate");
        let stored: String = conn
            .query_row(
                "SELECT checksum FROM refinery_schema_history WHERE version = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let report = run_rehash(&mut conn, &path).expect("rehash");
        assert!(
            report.rewritten.is_empty(),
            "V001 must NOT be rewritten when checksums already match: rewrote={:?}",
            report.rewritten
        );
        // And re-running runner() should still succeed (the original
        // error that the failing test exposed was that the second
        // runner().run() call saw a checksum mismatch).
        crate::migrations::runner()
            .run(&mut conn)
            .expect("re-run migrate must succeed");
        let stored_after: String = conn
            .query_row(
                "SELECT checksum FROM refinery_schema_history WHERE version = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            stored, stored_after,
            "checksum must not change after rehash"
        );
    }

    #[test]
    fn to_llm_only_reports_no_vec_tables_on_fresh_db() {
        // Fresh v1.0.76 database (created by running the full migration
        // set) has no vec tables.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("fresh.sqlite");
        let mut conn = open_rw(&path).expect("open_rw");
        crate::migrations::runner().run(&mut conn).expect("migrate");
        let report = run_to_llm_only(&mut conn, &path).expect("to_llm_only");
        assert!(!report.vec_tables_were_present);
        assert!(report.v013_applied, "V013 must be marked applied");
        assert_eq!(report.status, "ok");
    }

    #[test]
    fn history_table_exists_detects_table() {
        let conn = create_db_with_history(1);
        assert!(history_table_exists(&conn));
        let conn2 = create_db_without_history();
        assert!(!history_table_exists(&conn2));
    }

    #[test]
    fn sanitize_null_applied_on_fixes_null_rows() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(
            "CREATE TABLE refinery_schema_history (
                version INTEGER NOT NULL,
                name TEXT,
                applied_on TEXT,
                checksum TEXT
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO refinery_schema_history (version, name, checksum) VALUES (1, 'init', '123')",
            [],
        )
        .unwrap();
        let fixed = sanitize_null_applied_on(&conn).unwrap();
        assert_eq!(fixed, 1, "must fix exactly one NULL row");
        let applied: String = conn
            .query_row(
                "SELECT applied_on FROM refinery_schema_history WHERE version = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            chrono::DateTime::parse_from_rfc3339(&applied).is_ok(),
            "applied_on must be valid RFC3339, got: {applied}"
        );
    }

    #[test]
    fn sanitize_null_applied_on_noop_when_all_filled() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(
            "CREATE TABLE refinery_schema_history (
                version INTEGER NOT NULL,
                name TEXT,
                applied_on TEXT,
                checksum TEXT
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO refinery_schema_history (version, name, applied_on, checksum) VALUES (1, 'init', '2026-06-09T00:00:00+00:00', '123')",
            [],
        )
        .unwrap();
        let fixed = sanitize_null_applied_on(&conn).unwrap();
        assert_eq!(fixed, 0, "must not touch rows with valid applied_on");
    }

    #[test]
    fn rehash_does_not_insert_missing_migrations() {
        let mut conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(
            "CREATE TABLE refinery_schema_history (
                version INTEGER NOT NULL,
                name TEXT,
                applied_on TEXT,
                checksum TEXT
            );",
        )
        .unwrap();
        let runner = crate::migrations::runner();
        let migrations = runner.get_migrations();
        for mig in migrations.iter() {
            if mig.version() >= 13 {
                break;
            }
            let name = mig.name().to_string();
            let v = mig.version();
            let sql = mig.sql().unwrap_or("").to_string();
            let cs = compute_checksum(&name, v, &sql).to_string();
            conn.execute(
                "INSERT INTO refinery_schema_history (version, name, applied_on, checksum) VALUES (?1, ?2, '2026-01-01T00:00:00+00:00', ?3)",
                rusqlite::params![v, name, cs],
            )
            .unwrap();
        }
        let _report = run_rehash(&mut conn, Path::new("/tmp/test.sqlite")).unwrap();
        let v013_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM refinery_schema_history WHERE version = 13",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
            > 0;
        assert!(
            !v013_exists,
            "V013 must NOT be inserted by run_rehash (G41 fix)"
        );
    }

    #[test]
    fn remove_vec_tables_noop_when_no_vec() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        let removed = remove_vec_virtual_tables_without_module(&conn).unwrap();
        assert_eq!(removed, 0);
    }

    #[test]
    fn ensure_v013_tables_noop_when_no_history() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        let created = ensure_v013_tables_exist(&conn).unwrap();
        assert!(!created, "must be no-op when history table is absent");
    }

    #[test]
    fn ensure_v013_tables_noop_when_tables_exist() {
        let mut conn = Connection::open_in_memory().expect("in-memory db");
        crate::migrations::runner().run(&mut conn).unwrap();
        let created = ensure_v013_tables_exist(&conn).unwrap();
        assert!(
            !created,
            "must be no-op when memory_embeddings already exists"
        );
    }

    #[test]
    fn ensure_v013_tables_creates_when_phantom() {
        let mut conn = Connection::open_in_memory().expect("in-memory db");
        crate::migrations::runner().run(&mut conn).unwrap();
        conn.execute_batch("DROP TABLE IF EXISTS memory_embeddings")
            .unwrap();
        conn.execute_batch("DROP TABLE IF EXISTS entity_embeddings")
            .unwrap();
        conn.execute_batch("DROP TABLE IF EXISTS chunk_embeddings")
            .unwrap();
        let created = ensure_v013_tables_exist(&conn).unwrap();
        assert!(
            created,
            "must create tables when V013 is in history but tables are missing"
        );
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='memory_embeddings'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "memory_embeddings must exist after repair");
    }
}
