//! SQLite PRAGMA helpers applied at connection open and on each transaction.

use crate::errors::AppError;
use rusqlite::Connection;

pub fn apply_init_pragmas(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch("PRAGMA auto_vacuum = INCREMENTAL;")?;
    apply_connection_pragmas(conn)?;
    let mode: String = conn.query_row("PRAGMA journal_mode = WAL;", [], |r| r.get(0))?;
    if mode != "wal" {
        tracing::warn!(mode = %mode, "journal_mode did not switch to WAL");
    }
    conn.execute_batch(&format!(
        "PRAGMA wal_autocheckpoint = {};",
        crate::constants::WAL_AUTOCHECKPOINT_PAGES
    ))?;
    Ok(())
}

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
    Ok(())
}
