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
    Ok(conn)
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
        "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('dim', '384')",
        [],
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
    Ok(conn)
}
