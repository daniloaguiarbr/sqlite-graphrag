//! Version history storage for memory records.
//!
//! Manages the `memory_versions` table: inserts a new version snapshot on
//! every update so the `restore` command can roll back to any prior body.

use crate::errors::AppError;
use rusqlite::{params, Connection};

#[allow(clippy::too_many_arguments)]
pub fn insert_version(
    conn: &Connection,
    memory_id: i64,
    version: i64,
    name: &str,
    memory_type: &str,
    description: &str,
    body: &str,
    metadata: &str,
    changed_by: Option<&str>,
    change_reason: &str,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO memory_versions
         (memory_id, version, name, type, description, body, metadata, changed_by, change_reason)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            memory_id,
            version,
            name,
            memory_type,
            description,
            body,
            metadata,
            changed_by,
            change_reason
        ],
    )?;
    Ok(())
}

pub fn next_version(conn: &Connection, memory_id: i64) -> Result<i64, AppError> {
    let v: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) + 1 FROM memory_versions WHERE memory_id = ?1",
        params![memory_id],
        |r| r.get(0),
    )?;
    Ok(v)
}
