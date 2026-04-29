//! SQLite connection setup with PRAGMAs and 0600 permissions.
//!
//! Opens (or creates) the database file, loads the `sqlite-vec` extension,
//! applies WAL/journal PRAGMAs, and enforces 0600 file permissions on Unix.

use crate::errors::AppError;
use crate::pragmas::apply_connection_pragmas;
use rusqlite::Connection;
use sqlite_vec::sqlite3_vec_init;
use std::path::Path;

/// Register sqlite-vec GLOBALLY before any connection is opened.
/// Must be called once at program start.
pub fn register_vec_extension() {
    // SAFETY: sqlite3_auto_extension is a C FFI function that registers a callback
    // invoked when SQLite opens any new connection. Soundness assumptions:
    // 1. `sqlite3_vec_init` has the exact ABI signature `extern "C" fn(...) -> i32`
    //    expected by SQLite's auto-extension API (verified by sqlite-vec crate).
    // 2. The transmute from `*const ()` to the expected fn pointer is valid because
    //    both have identical layout on supported platforms (Linux, macOS, Windows).
    // 3. This function is only called once at program start (asserted by callers
    //    in main.rs:80 before any connection is opened).
    #[allow(clippy::missing_transmute_annotations)]
    unsafe {
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite3_vec_init as *const (),
        )));
    }
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
