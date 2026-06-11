//! SQLite connection setup with PRAGMAs and 0600 permissions.
//!
//! v1.0.76: opens (or creates) the database file. The `sqlite-vec` extension
//! was REMOVED; vector similarity is now computed in pure Rust over the
//! `memory_embeddings(memory_id, embedding BLOB, source)` table. WAL/journal
//! PRAGMAs and 0600 file permissions on Unix are unchanged.

use crate::errors::AppError;
use crate::paths::AppPaths;
use crate::pragmas::{apply_connection_pragmas, apply_init_pragmas, ensure_wal_mode};
use rusqlite::Connection;
use std::path::Path;

/// v1.0.76: no-op stub. Kept for source compatibility with callers that
/// still call `register_vec_extension()` during auto-init. The actual
/// extension registration is gone; the function is now a marker that
/// the LLM-only build does not need any vector extension.
pub fn register_vec_extension() {}

pub fn open_rw(path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open(path)?;
    apply_connection_pragmas(&conn)?;
    apply_secure_permissions(path);
    adopt_embedding_dim(&conn);
    Ok(conn)
}

/// G42/S1 follow-up (G43): adopts the dimensionality recorded in
/// `schema_meta.dim` for this process, so EVERY command that opens the
/// database — not only the `ensure_db_ready` auto-init path — produces
/// and queries vectors of the database dimensionality. Pre-G43 the
/// adoption only ran in `ensure_db_ready`, which `remember` / `edit` /
/// `recall` / `hybrid-search` never call; those commands silently used
/// the compiled default (64) against pre-v1.0.79 384-dim databases,
/// writing mixed-dim embeddings that cosine-score 0.0 against each
/// other.
///
/// Read-only and best-effort by design: a virgin database without
/// `schema_meta` is a no-op (the table is created and persisted later
/// by `ensure_schema` / `ensure_db_ready`). The env/flag override
/// always wins and is handled inside `constants::embedding_dim`.
fn adopt_embedding_dim(conn: &Connection) {
    if crate::constants::embedding_dim_from_env().is_some() {
        return;
    }
    if let Ok(value) = conn.query_row(
        "SELECT value FROM schema_meta WHERE key = 'dim'",
        [],
        |row| row.get::<_, String>(0),
    ) {
        if let Ok(dim) = value.parse::<usize>() {
            crate::constants::set_active_embedding_dim(dim);
        }
    }
}

pub fn ensure_schema(conn: &mut Connection) -> Result<(), AppError> {
    crate::migrations::runner()
        .run(conn)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("migration failed: {e}")))?;
    conn.execute_batch(&format!(
        "PRAGMA user_version = {};",
        crate::constants::SCHEMA_USER_VERSION
    ))?;
    Ok(())
}

/// Ensures the database file exists and the schema is at the current version.
///
/// Behavior:
/// - DB does not exist: creates the file, applies init PRAGMAs, runs all migrations,
///   sets `PRAGMA user_version`, and populates `schema_meta` with default values.
///   Emits `tracing::info!` on creation.
/// - DB exists with `user_version` below `SCHEMA_USER_VERSION`: runs the remaining
///   migrations and updates `user_version`. Emits `tracing::warn!` on auto-migration.
/// - DB exists with `user_version` equal to `SCHEMA_USER_VERSION`: no-op.
///
/// This helper unifies the auto-init contract across CRUD handlers so users can run
/// any subcommand on a fresh directory without invoking `init` first. Idempotent
/// and safe to call before every handler that needs a ready database.
pub fn ensure_db_ready(paths: &AppPaths) -> Result<(), AppError> {
    register_vec_extension();
    paths.ensure_dirs()?;

    let db_existed = paths.db.exists();

    if !db_existed {
        tracing::info!(target: "storage",
            path = %paths.db.display(),
            schema_version = crate::constants::CURRENT_SCHEMA_VERSION,
            "creating database (auto-init)"
        );
    }

    let mut conn = open_rw(&paths.db)?;

    if !db_existed {
        apply_init_pragmas(&conn)?;
    }

    let current_user_version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap_or(0);
    let target_user_version = crate::constants::SCHEMA_USER_VERSION;

    if current_user_version < target_user_version {
        if db_existed {
            tracing::warn!(target: "storage",
                from = current_user_version,
                to = target_user_version,
                path = %paths.db.display(),
                "auto-migrating database schema"
            );
        }
        crate::migrations::runner()
            .run(&mut conn)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("auto-migration failed: {e}")))?;
        conn.execute_batch(&format!("PRAGMA user_version = {target_user_version};"))?;

        if !db_existed {
            insert_default_schema_meta(&conn)?;
        }

        // Defensive re-assertion: refinery's migration runner may open internal
        // handles that revert journal_mode to delete on some platforms. Re-apply
        // WAL after migrations to guarantee the documented contract holds for
        // every command that goes through the auto-init path.
        ensure_wal_mode(&conn)?;
    }

    // G41 repair: if V013 is in history but embedding tables are missing,
    // execute V013 SQL directly. Runs unconditionally because databases
    // corrupted by G41 already have user_version=50 and skip the block above.
    crate::commands::migrate::ensure_v013_tables_exist(&conn)?;

    // G42/S1 (v1.0.79): synchronise the active embedding dimensionality
    // with the database. Existing databases keep their recorded `dim`
    // (e.g. 384 from pre-v1.0.79); an explicit env/flag override is
    // persisted back so `health --json` reports the truth. This is an
    // UPDATE of an existing `schema_meta` key — ZERO schema change.
    sync_embedding_dim_meta(&conn)?;

    Ok(())
}

/// G42/S1: two-way sync between `schema_meta.dim` and the process-wide
/// active embedding dimensionality.
///
/// - env/flag override set → persist it into `schema_meta.dim`;
/// - no override → adopt the database value via
///   [`crate::constants::set_active_embedding_dim`] so old 384-dim
///   databases keep producing and querying 384-dim vectors;
/// - key missing (legacy/corrupt meta) → write the resolved default.
fn sync_embedding_dim_meta(conn: &Connection) -> Result<(), AppError> {
    let db_dim: Option<usize> = conn
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'dim'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|v| v.parse::<usize>().ok());

    if let Some(env_dim) = crate::constants::embedding_dim_from_env() {
        if db_dim != Some(env_dim) {
            conn.execute(
                "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('dim', ?1)",
                rusqlite::params![env_dim.to_string()],
            )?;
        }
        return Ok(());
    }

    match db_dim {
        Some(dim) => crate::constants::set_active_embedding_dim(dim),
        None => {
            conn.execute(
                "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('dim', ?1)",
                rusqlite::params![crate::constants::embedding_dim().to_string()],
            )?;
        }
    }
    Ok(())
}

fn insert_default_schema_meta(conn: &Connection) -> Result<(), AppError> {
    conn.execute(
        "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('schema_version', ?1)",
        rusqlite::params![crate::constants::CURRENT_SCHEMA_VERSION.to_string()],
    )?;
    conn.execute(
        "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('model', 'multilingual-e5-small')",
        [],
    )?;
    conn.execute(
        "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('dim', ?1)",
        rusqlite::params![crate::constants::embedding_dim().to_string()],
    )?;
    conn.execute(
        "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('created_at', CAST(unixepoch() AS TEXT))",
        [],
    )?;
    conn.execute(
        "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('sqlite-graphrag_version', ?1)",
        rusqlite::params![crate::constants::SQLITE_GRAPHRAG_VERSION],
    )?;
    Ok(())
}

/// Applies 600 permissions (owner read/write only) to the SQLite file and its WAL/SHM
/// companion files on Unix to prevent leaking private memories in shared directories
/// (e.g. multi-user /tmp, Dropbox, NFS). On Windows, NTFS DACL default is private-to-user
/// so explicit permission setting is unnecessary; a debug log records the skip. Failures
/// are silent to avoid blocking the operation when the process does not own the file
/// (e.g. read-only mount).
#[allow(unused_variables)]
fn apply_secure_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let candidates = [
            path.to_path_buf(),
            path.with_extension(format!(
                "{}-wal",
                path.extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("sqlite")
            )),
            path.with_extension(format!(
                "{}-shm",
                path.extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("sqlite")
            )),
        ];
        for file in candidates.iter() {
            if file.exists() {
                if let Ok(meta) = std::fs::metadata(file) {
                    let mut perms = meta.permissions();
                    perms.set_mode(0o600);
                    let _ = std::fs::set_permissions(file, perms);
                }
            }
        }
    }
    #[cfg(windows)]
    {
        tracing::debug!(target: "storage",
            path = %path.display(),
            "skipping Unix mode 0o600 on Windows; NTFS DACL default is private-to-user"
        );
    }
}

pub fn open_ro(path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    // G43: read-only commands (`recall`, `hybrid-search`) embed the QUERY
    // text, so they must adopt the database dimensionality too.
    adopt_embedding_dim(&conn);
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// G43 regression: `open_rw` must adopt `schema_meta.dim` so EVERY
    /// command (not only the `ensure_db_ready` auto-init path) produces
    /// vectors of the database dimensionality. Pre-G43, `remember` /
    /// `edit` / `recall` / `hybrid-search` used the compiled default
    /// against pre-v1.0.79 384-dim databases, silently writing
    /// mixed-dim embeddings that cosine-score 0.0 against each other.
    #[test]
    #[serial_test::serial(env)]
    fn open_rw_adopts_schema_meta_dim() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("g43.sqlite");
        {
            let conn = Connection::open(&db).expect("create seed db");
            conn.execute_batch(
                "CREATE TABLE schema_meta (key TEXT PRIMARY KEY, value TEXT);
                 INSERT INTO schema_meta VALUES ('dim', '128');",
            )
            .expect("seed schema_meta");
        }
        std::env::remove_var("SQLITE_GRAPHRAG_EMBEDDING_DIM");
        let _conn = open_rw(&db).expect("open_rw");
        let adopted = crate::constants::embedding_dim();
        // Restore the process-wide default before asserting so a failure
        // does not leak 128 into parallel tests.
        crate::constants::set_active_embedding_dim(crate::constants::DEFAULT_EMBEDDING_DIM);
        assert_eq!(adopted, 128, "open_rw must adopt the recorded db dim (G43)");
    }

    /// G43 regression: `open_ro` (used by `recall` / `hybrid-search` to
    /// embed the QUERY text) must adopt the database dim too.
    #[test]
    #[serial_test::serial(env)]
    fn open_ro_adopts_schema_meta_dim() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("g43-ro.sqlite");
        {
            let conn = Connection::open(&db).expect("create seed db");
            conn.execute_batch(
                "CREATE TABLE schema_meta (key TEXT PRIMARY KEY, value TEXT);
                 INSERT INTO schema_meta VALUES ('dim', '256');",
            )
            .expect("seed schema_meta");
        }
        std::env::remove_var("SQLITE_GRAPHRAG_EMBEDDING_DIM");
        let _conn = open_ro(&db).expect("open_ro");
        let adopted = crate::constants::embedding_dim();
        crate::constants::set_active_embedding_dim(crate::constants::DEFAULT_EMBEDDING_DIM);
        assert_eq!(adopted, 256, "open_ro must adopt the recorded db dim (G43)");
    }

    /// G43: the env override always wins over the recorded database dim
    /// (precedence contract of `constants::embedding_dim`).
    #[test]
    #[serial_test::serial(env)]
    fn env_override_wins_over_schema_meta_dim() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("g43-env.sqlite");
        {
            let conn = Connection::open(&db).expect("create seed db");
            conn.execute_batch(
                "CREATE TABLE schema_meta (key TEXT PRIMARY KEY, value TEXT);
                 INSERT INTO schema_meta VALUES ('dim', '128');",
            )
            .expect("seed schema_meta");
        }
        std::env::set_var("SQLITE_GRAPHRAG_EMBEDDING_DIM", "96");
        let _conn = open_rw(&db).expect("open_rw");
        let adopted = crate::constants::embedding_dim();
        std::env::remove_var("SQLITE_GRAPHRAG_EMBEDDING_DIM");
        crate::constants::set_active_embedding_dim(crate::constants::DEFAULT_EMBEDDING_DIM);
        assert_eq!(adopted, 96, "env override must win over the db dim (G43)");
    }

    /// G43: a virgin database without `schema_meta` must open cleanly
    /// (best-effort adoption is a no-op, never an error).
    #[test]
    #[serial_test::serial(env)]
    fn open_rw_on_virgin_db_is_a_noop() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("g43-virgin.sqlite");
        std::env::remove_var("SQLITE_GRAPHRAG_EMBEDDING_DIM");
        crate::constants::set_active_embedding_dim(crate::constants::DEFAULT_EMBEDDING_DIM);
        let _conn = open_rw(&db).expect("open_rw on virgin db must not fail");
        assert_eq!(
            crate::constants::embedding_dim(),
            crate::constants::DEFAULT_EMBEDDING_DIM,
            "virgin db must keep the compiled default (G43)"
        );
    }
}
