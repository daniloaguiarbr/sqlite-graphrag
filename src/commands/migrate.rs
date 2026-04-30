//! Handler for the `migrate` CLI subcommand.

use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use rusqlite::OptionalExtension;
use serde::Serialize;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Apply pending schema migrations\n  \
    sqlite-graphrag migrate\n\n  \
    # Show already-applied migrations without applying new ones\n  \
    sqlite-graphrag migrate --status\n\n  \
    # Migrate a database at a custom path\n  \
    sqlite-graphrag migrate --db /path/to/graphrag.sqlite")]
pub struct MigrateArgs {
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
    /// Explicit JSON flag. Accepted as a no-op because output is already JSON by default.
    #[arg(long, default_value_t = false)]
    pub json: bool,
    /// Show already applied migrations without applying new ones.
    #[arg(long, default_value_t = false)]
    pub status: bool,
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
struct MigrationEntry {
    version: i64,
    name: String,
    applied_on: Option<String>,
}

pub fn run(args: MigrateArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let _ = args.json; // --json is a no-op because output is already JSON by default
    let paths = AppPaths::resolve(args.db.as_deref())?;
    paths.ensure_dirs()?;

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
    let mut stmt = conn.prepare(
        "SELECT version, name, applied_on FROM refinery_schema_history ORDER BY version ASC",
    )?;
    let entries = stmt
        .query_map([], |r| {
            Ok(MigrationEntry {
                version: r.get(0)?,
                name: r.get(1)?,
                applied_on: r.get(2)?,
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
        // Without refinery_schema_history table, SQLite returns Unknown (code 1) -> AppError::Database
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
        // Table exists but is empty -> QueryReturnedNoRows -> 0
        let version = latest_schema_version(&conn).unwrap();
        assert_eq!(version, 0u32);
    }
}
