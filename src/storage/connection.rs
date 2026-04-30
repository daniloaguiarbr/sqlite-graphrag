//! SQLite connection setup with PRAGMAs and 0600 permissions.
//!
//! Opens (or creates) the database file, loads the `sqlite-vec` extension,
//! applies WAL/journal PRAGMAs, and enforces 0600 file permissions on Unix.

use crate::errors::AppError;
use crate::paths::AppPaths;
use crate::pragmas::{apply_connection_pragmas, apply_init_pragmas, ensure_wal_mode};
use rusqlite::Connection;
use sqlite_vec::sqlite3_vec_init;
use std::path::Path;
use std::sync::OnceLock;

static VEC_EXTENSION_REGISTERED: OnceLock<()> = OnceLock::new();

/// Register sqlite-vec GLOBALLY before any connection is opened.
///
/// Idempotent: subsequent calls are no-ops thanks to `OnceLock`. Safe to invoke from
/// both the binary entry point (`main.rs`) and library helpers like `ensure_db_ready`
/// so unit tests that exercise CRUD handlers do not need to pre-register the extension.
pub fn register_vec_extension() {
    VEC_EXTENSION_REGISTERED.get_or_init(|| {
        // SAFETY: sqlite3_auto_extension is a C FFI function that registers a callback
        // invoked when SQLite opens any new connection. Soundness assumptions:
        // 1. `sqlite3_vec_init` has the exact ABI signature `extern "C" fn(...) -> i32`
        //    expected by SQLite's auto-extension API (verified by sqlite-vec crate).
        // 2. The transmute from `*const ()` to the expected fn pointer is valid because
        //    both have identical layout on supported platforms (Linux, macOS, Windows).
        // 3. `OnceLock::get_or_init` guarantees this closure runs at most once across
        //    all threads; the auto-extension list is mutated exactly one time.
        #[allow(clippy::missing_transmute_annotations)]
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite3_vec_init as *const (),
            )));
        }
    });
}

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
        tracing::info!(
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
            tracing::warn!(
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
/// (e.g. multi-user /tmp, Dropbox, NFS). No-op on Windows. Failures are silent to avoid
/// blocking the operation when the process does not own the file (e.g. read-only mount).
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
}

pub fn open_ro(path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    Ok(conn)
}
