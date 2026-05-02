//! SQLite PRAGMA helpers applied at connection open and on each transaction.

use crate::errors::AppError;
use rusqlite::Connection;

/// Applies one-time PRAGMAs on a freshly opened connection (e.g. `auto_vacuum`).
///
/// Calls [`apply_connection_pragmas`] internally and then sets `wal_autocheckpoint`.
/// Must be called once per database file, not once per connection.
///
/// # Errors
/// Returns `Err` when any PRAGMA execution fails.
pub fn apply_init_pragmas(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch("PRAGMA auto_vacuum = INCREMENTAL;")?;
    apply_connection_pragmas(conn)?;
    conn.execute_batch(&format!(
        "PRAGMA wal_autocheckpoint = {};",
        crate::constants::WAL_AUTOCHECKPOINT_PAGES
    ))?;
    Ok(())
}

/// Re-asserts `PRAGMA journal_mode = WAL` after operations that may revert it
/// (notably refinery-driven migrations, which can open internal handles that
/// reset the journal mode in some scenarios). Idempotent and cheap; emits
/// `tracing::warn!` if WAL fails to engage so degraded behaviour is observable.
pub fn ensure_wal_mode(conn: &Connection) -> Result<(), AppError> {
    let mode: String = conn.query_row("PRAGMA journal_mode = WAL;", [], |r| r.get(0))?;
    if mode != "wal" {
        tracing::warn!(mode = %mode, "journal_mode did not switch to WAL after re-assertion");
    }
    Ok(())
}

/// Applies per-connection PRAGMAs: synchronous, foreign keys, busy timeout, cache, mmap, WAL.
///
/// Safe to call on every new connection; all settings are idempotent.
///
/// # Errors
/// Returns `Err` when any PRAGMA execution fails.
pub fn apply_connection_pragmas(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(&format!(
        "PRAGMA synchronous   = NORMAL;
         PRAGMA foreign_keys  = ON;
         PRAGMA busy_timeout  = {busy};
         PRAGMA cache_size    = {cache};
         PRAGMA temp_store    = MEMORY;
         PRAGMA mmap_size     = {mmap};",
        busy = crate::constants::BUSY_TIMEOUT_MILLIS,
        cache = crate::constants::CACHE_SIZE_KB,
        mmap = crate::constants::MMAP_SIZE_BYTES,
    ))?;
    let mode: String = conn.query_row("PRAGMA journal_mode = WAL;", [], |r| r.get(0))?;
    if mode != "wal" {
        tracing::warn!(mode = %mode, "journal_mode did not switch to WAL");
    }
    Ok(())
}
