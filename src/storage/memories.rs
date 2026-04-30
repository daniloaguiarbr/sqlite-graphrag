//! Persistence layer for the `memories` table and its vector companion.
//!
//! Functions here encapsulate every SQL statement touching `memories`,
//! `vec_memories` and the FTS5 `fts_memories` shadow table. Callers receive
//! typed [`MemoryRow`] or [`NewMemory`] values and never build SQL strings.

use crate::embedder::f32_to_bytes;
use crate::errors::AppError;
use crate::storage::utils::with_busy_retry;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Input payload for inserting or updating a memory.
///
/// `body_hash` must be the BLAKE3 digest of `body`. The `metadata` field is
/// stored as a TEXT column containing JSON.
#[derive(Debug, Serialize, Deserialize)]
pub struct NewMemory {
    pub namespace: String,
    pub name: String,
    pub memory_type: String,
    pub description: String,
    pub body: String,
    pub body_hash: String,
    pub session_id: Option<String>,
    pub source: String,
    pub metadata: serde_json::Value,
}

/// Fully materialized row from the `memories` table.
///
/// Returned by [`read_by_name`], [`read_full`], [`list`] and [`fts_search`].
/// The `metadata` field is kept as a JSON string to avoid double parsing.
#[derive(Debug, Serialize)]
pub struct MemoryRow {
    pub id: i64,
    pub namespace: String,
    pub name: String,
    pub memory_type: String,
    pub description: String,
    pub body: String,
    pub body_hash: String,
    pub session_id: Option<String>,
    pub source: String,
    pub metadata: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Finds a live memory by `(namespace, name)` and returns key metadata.
///
/// # Arguments
///
/// - `conn` — open SQLite connection configured with the project pragmas.
/// - `namespace` — resolved namespace for the lookup.
/// - `name` — kebab-case memory name.
///
/// # Returns
///
/// `Ok(Some((id, updated_at, max_version)))` when the memory exists and is
/// not soft-deleted, `Ok(None)` otherwise.
///
/// # Errors
///
/// Returns `Err(AppError::Database)` on any `rusqlite` failure.
pub fn find_by_name(
    conn: &Connection,
    namespace: &str,
    name: &str,
) -> Result<Option<(i64, i64, i64)>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT m.id, m.updated_at, COALESCE(MAX(v.version), 0)
         FROM memories m
         LEFT JOIN memory_versions v ON v.memory_id = m.id
         WHERE m.namespace = ?1 AND m.name = ?2 AND m.deleted_at IS NULL
         GROUP BY m.id",
    )?;
    let result = stmt.query_row(params![namespace, name], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
        ))
    });
    match result {
        Ok(row) => Ok(Some(row)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e)),
    }
}

/// Looks up a live memory by exact `body_hash` within a namespace.
///
/// Used during `remember` to short-circuit semantic duplicates before
/// spending an embedding call.
///
/// # Returns
///
/// `Ok(Some(id))` when a live memory with the same hash exists,
/// `Ok(None)` otherwise.
///
/// # Errors
///
/// Returns `Err(AppError::Database)` on any `rusqlite` failure.
pub fn find_by_hash(
    conn: &Connection,
    namespace: &str,
    body_hash: &str,
) -> Result<Option<i64>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT id FROM memories WHERE namespace = ?1 AND body_hash = ?2 AND deleted_at IS NULL",
    )?;
    match stmt.query_row(params![namespace, body_hash], |r| r.get(0)) {
        Ok(id) => Ok(Some(id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e)),
    }
}

/// Inserts a new row into the `memories` table.
///
/// # Arguments
///
/// - `conn` — active SQLite connection, typically inside a transaction.
/// - `m` — validated payload including `body_hash` and serialized metadata.
///
/// # Returns
///
/// The `rowid` assigned to the newly inserted memory.
///
/// # Errors
///
/// Returns `Err(AppError::Database)` on insertion failure and
/// `Err(AppError::Json)` if metadata serialization fails.
pub fn insert(conn: &Connection, m: &NewMemory) -> Result<i64, AppError> {
    conn.execute(
        "INSERT INTO memories (namespace, name, type, description, body, body_hash, session_id, source, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            m.namespace, m.name, m.memory_type, m.description, m.body,
            m.body_hash, m.session_id, m.source,
            serde_json::to_string(&m.metadata)?
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Updates an existing memory optionally guarded by optimistic concurrency.
///
/// When `expected_updated_at` is `Some(ts)` the row is only updated if its
/// current `updated_at` equals `ts`. This protects concurrent `edit` calls
/// from silently clobbering each other.
///
/// # Returns
///
/// `Ok(true)` when exactly one row was updated, `Ok(false)` when the
/// optimistic check failed or the memory does not exist.
///
/// # Errors
///
/// Returns `Err(AppError::Database)` on any `rusqlite` failure.
pub fn update(
    conn: &Connection,
    id: i64,
    m: &NewMemory,
    expected_updated_at: Option<i64>,
) -> Result<bool, AppError> {
    let affected = if let Some(ts) = expected_updated_at {
        conn.execute(
            "UPDATE memories SET type=?2, description=?3, body=?4, body_hash=?5,
             session_id=?6, source=?7, metadata=?8
             WHERE id=?1 AND updated_at=?9 AND deleted_at IS NULL",
            params![
                id,
                m.memory_type,
                m.description,
                m.body,
                m.body_hash,
                m.session_id,
                m.source,
                serde_json::to_string(&m.metadata)?,
                ts
            ],
        )?
    } else {
        conn.execute(
            "UPDATE memories SET type=?2, description=?3, body=?4, body_hash=?5,
             session_id=?6, source=?7, metadata=?8
             WHERE id=?1 AND deleted_at IS NULL",
            params![
                id,
                m.memory_type,
                m.description,
                m.body,
                m.body_hash,
                m.session_id,
                m.source,
                serde_json::to_string(&m.metadata)?
            ],
        )?
    };
    Ok(affected == 1)
}

/// Replaces the vector row for a memory in `vec_memories`.
///
/// `sqlite-vec` virtual tables do not implement `INSERT OR REPLACE`, so the
/// existing row is deleted first and a fresh vector is inserted. Callers
/// must pass an `embedding` with length [`crate::constants::EMBEDDING_DIM`].
///
/// # Errors
///
/// Returns `Err(AppError::Database)` on any `rusqlite` failure.
pub fn upsert_vec(
    conn: &Connection,
    memory_id: i64,
    namespace: &str,
    memory_type: &str,
    embedding: &[f32],
    name: &str,
    snippet: &str,
) -> Result<(), AppError> {
    // sqlite-vec virtual tables do not support INSERT OR REPLACE semantics.
    // Must delete the existing row first, then insert.  Both statements are
    // wrapped in `with_busy_retry` because WAL-mode concurrent writers can
    // cause SQLITE_BUSY on vec0 virtual table writes.
    let embedding_bytes = f32_to_bytes(embedding);
    with_busy_retry(|| {
        conn.execute(
            "DELETE FROM vec_memories WHERE memory_id = ?1",
            params![memory_id],
        )?;
        conn.execute(
            "INSERT INTO vec_memories(memory_id, namespace, type, embedding, name, snippet)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                memory_id,
                namespace,
                memory_type,
                &embedding_bytes,
                name,
                snippet
            ],
        )?;
        Ok(())
    })
}

/// Deletes the vector row for `memory_id` from `vec_memories`.
///
/// Called during `forget` and `purge` to keep the vector table consistent
/// with the logical state of `memories`.
///
/// # Errors
///
/// Returns `Err(AppError::Database)` on any `rusqlite` failure.
pub fn delete_vec(conn: &Connection, memory_id: i64) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM vec_memories WHERE memory_id = ?1",
        params![memory_id],
    )?;
    Ok(())
}

/// Fetches a live memory by `(namespace, name)` and returns all columns.
///
/// # Returns
///
/// `Ok(Some(row))` when found, `Ok(None)` when missing or soft-deleted.
///
/// # Errors
///
/// Returns `Err(AppError::Database)` on any `rusqlite` failure.
pub fn read_by_name(
    conn: &Connection,
    namespace: &str,
    name: &str,
) -> Result<Option<MemoryRow>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, namespace, name, type, description, body, body_hash,
                session_id, source, metadata, created_at, updated_at
         FROM memories WHERE namespace=?1 AND name=?2 AND deleted_at IS NULL",
    )?;
    match stmt.query_row(params![namespace, name], |r| {
        Ok(MemoryRow {
            id: r.get(0)?,
            namespace: r.get(1)?,
            name: r.get(2)?,
            memory_type: r.get(3)?,
            description: r.get(4)?,
            body: r.get(5)?,
            body_hash: r.get(6)?,
            session_id: r.get(7)?,
            source: r.get(8)?,
            metadata: r.get(9)?,
            created_at: r.get(10)?,
            updated_at: r.get(11)?,
        })
    }) {
        Ok(m) => Ok(Some(m)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e)),
    }
}

/// Soft-deletes a memory by setting `deleted_at = unixepoch()`.
///
/// Versions and chunks are preserved so `restore` can undo the operation
/// until a subsequent `purge` reclaims the storage permanently.
///
/// # Returns
///
/// `Ok(true)` when a live memory was soft-deleted, `Ok(false)` when no
/// matching live row existed.
///
/// # Errors
///
/// Returns `Err(AppError::Database)` on any `rusqlite` failure.
pub fn soft_delete(conn: &Connection, namespace: &str, name: &str) -> Result<bool, AppError> {
    let affected = conn.execute(
        "UPDATE memories SET deleted_at = unixepoch() WHERE namespace=?1 AND name=?2 AND deleted_at IS NULL",
        params![namespace, name],
    )?;
    Ok(affected == 1)
}

/// Lists live memories in a namespace ordered by `updated_at` descending.
///
/// # Arguments
///
/// - `memory_type` — optional filter on the `type` column.
/// - `limit` / `offset` — standard pagination controls in rows.
///
/// # Errors
///
/// Returns `Err(AppError::Database)` on any `rusqlite` failure.
pub fn list(
    conn: &Connection,
    namespace: &str,
    memory_type: Option<&str>,
    limit: usize,
    offset: usize,
    include_deleted: bool,
) -> Result<Vec<MemoryRow>, AppError> {
    let deleted_clause = if include_deleted {
        ""
    } else {
        " AND deleted_at IS NULL"
    };
    if let Some(mt) = memory_type {
        let sql = format!(
            "SELECT id, namespace, name, type, description, body, body_hash,
                    session_id, source, metadata, created_at, updated_at
             FROM memories WHERE namespace=?1 AND type=?2{deleted_clause}
             ORDER BY updated_at DESC LIMIT ?3 OFFSET ?4"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![namespace, mt, limit as i64, offset as i64], |r| {
                Ok(MemoryRow {
                    id: r.get(0)?,
                    namespace: r.get(1)?,
                    name: r.get(2)?,
                    memory_type: r.get(3)?,
                    description: r.get(4)?,
                    body: r.get(5)?,
                    body_hash: r.get(6)?,
                    session_id: r.get(7)?,
                    source: r.get(8)?,
                    metadata: r.get(9)?,
                    created_at: r.get(10)?,
                    updated_at: r.get(11)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    } else {
        let sql = format!(
            "SELECT id, namespace, name, type, description, body, body_hash,
                    session_id, source, metadata, created_at, updated_at
             FROM memories WHERE namespace=?1{deleted_clause}
             ORDER BY updated_at DESC LIMIT ?2 OFFSET ?3"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![namespace, limit as i64, offset as i64], |r| {
                Ok(MemoryRow {
                    id: r.get(0)?,
                    namespace: r.get(1)?,
                    name: r.get(2)?,
                    memory_type: r.get(3)?,
                    description: r.get(4)?,
                    body: r.get(5)?,
                    body_hash: r.get(6)?,
                    session_id: r.get(7)?,
                    source: r.get(8)?,
                    metadata: r.get(9)?,
                    created_at: r.get(10)?,
                    updated_at: r.get(11)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

/// Runs a KNN search over `vec_memories`, optionally restricted to namespaces.
///
/// # Arguments
///
/// - `embedding` — query vector of length [`crate::constants::EMBEDDING_DIM`].
/// - `namespaces` — namespaces to search. Empty slice means "all namespaces".
/// - `memory_type` — optional filter on the `type` column.
/// - `k` — maximum number of hits to return.
///
/// # Returns
///
/// A vector of `(memory_id, distance)` pairs sorted by ascending distance.
///
/// # Errors
///
/// Returns `Err(AppError::Database)` on any `rusqlite` failure.
pub fn knn_search(
    conn: &Connection,
    embedding: &[f32],
    namespaces: &[String],
    memory_type: Option<&str>,
    k: usize,
) -> Result<Vec<(i64, f32)>, AppError> {
    let bytes = f32_to_bytes(embedding);

    match namespaces.len() {
        0 => {
            // No namespace filter — search all namespaces.
            if let Some(mt) = memory_type {
                let mut stmt = conn.prepare(
                    "SELECT memory_id, distance FROM vec_memories \
                     WHERE embedding MATCH ?1 AND type = ?2 \
                     ORDER BY distance LIMIT ?3",
                )?;
                let rows = stmt
                    .query_map(params![bytes, mt, k as i64], |r| {
                        Ok((r.get::<_, i64>(0)?, r.get::<_, f32>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            } else {
                let mut stmt = conn.prepare(
                    "SELECT memory_id, distance FROM vec_memories \
                     WHERE embedding MATCH ?1 \
                     ORDER BY distance LIMIT ?2",
                )?;
                let rows = stmt
                    .query_map(params![bytes, k as i64], |r| {
                        Ok((r.get::<_, i64>(0)?, r.get::<_, f32>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            }
        }
        1 => {
            // Fast single-namespace path (preserved from previous implementation).
            let ns = &namespaces[0];
            if let Some(mt) = memory_type {
                let mut stmt = conn.prepare(
                    "SELECT memory_id, distance FROM vec_memories \
                     WHERE embedding MATCH ?1 AND namespace = ?2 AND type = ?3 \
                     ORDER BY distance LIMIT ?4",
                )?;
                let rows = stmt
                    .query_map(params![bytes, ns, mt, k as i64], |r| {
                        Ok((r.get::<_, i64>(0)?, r.get::<_, f32>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            } else {
                let mut stmt = conn.prepare(
                    "SELECT memory_id, distance FROM vec_memories \
                     WHERE embedding MATCH ?1 AND namespace = ?2 \
                     ORDER BY distance LIMIT ?3",
                )?;
                let rows = stmt
                    .query_map(params![bytes, ns, k as i64], |r| {
                        Ok((r.get::<_, i64>(0)?, r.get::<_, f32>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            }
        }
        _ => {
            // Multiple explicit namespaces: build IN clause with positional placeholders.
            // rusqlite does not support array binding, so we generate "?,?,..." manually.
            let placeholders = (0..namespaces.len())
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            if let Some(mt) = memory_type {
                let query = format!(
                    "SELECT memory_id, distance FROM vec_memories \
                     WHERE embedding MATCH ? AND type = ? AND namespace IN ({placeholders}) \
                     ORDER BY distance LIMIT ?"
                );
                let mut stmt = conn.prepare(&query)?;
                // Params: [bytes, mt, ns0, ns1, ..., k]
                let mut raw_params: Vec<Box<dyn rusqlite::ToSql>> =
                    vec![Box::new(bytes), Box::new(mt.to_string())];
                for ns in namespaces {
                    raw_params.push(Box::new(ns.clone()));
                }
                raw_params.push(Box::new(k as i64));
                let param_refs: Vec<&dyn rusqlite::ToSql> =
                    raw_params.iter().map(|b| b.as_ref()).collect();
                let rows = stmt
                    .query_map(param_refs.as_slice(), |r| {
                        Ok((r.get::<_, i64>(0)?, r.get::<_, f32>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            } else {
                let query = format!(
                    "SELECT memory_id, distance FROM vec_memories \
                     WHERE embedding MATCH ? AND namespace IN ({placeholders}) \
                     ORDER BY distance LIMIT ?"
                );
                let mut stmt = conn.prepare(&query)?;
                // Params: [bytes, ns0, ns1, ..., k]
                let mut raw_params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(bytes)];
                for ns in namespaces {
                    raw_params.push(Box::new(ns.clone()));
                }
                raw_params.push(Box::new(k as i64));
                let param_refs: Vec<&dyn rusqlite::ToSql> =
                    raw_params.iter().map(|b| b.as_ref()).collect();
                let rows = stmt
                    .query_map(param_refs.as_slice(), |r| {
                        Ok((r.get::<_, i64>(0)?, r.get::<_, f32>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            }
        }
    }
}

/// Fetches a live memory by primary key and returns all columns.
///
/// Mirrors [`read_by_name`] but keyed on `rowid` for use after a KNN search.
///
/// # Errors
///
/// Returns `Err(AppError::Database)` on any `rusqlite` failure.
pub fn read_full(conn: &Connection, memory_id: i64) -> Result<Option<MemoryRow>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, namespace, name, type, description, body, body_hash,
                session_id, source, metadata, created_at, updated_at
         FROM memories WHERE id=?1 AND deleted_at IS NULL",
    )?;
    match stmt.query_row(params![memory_id], |r| {
        Ok(MemoryRow {
            id: r.get(0)?,
            namespace: r.get(1)?,
            name: r.get(2)?,
            memory_type: r.get(3)?,
            description: r.get(4)?,
            body: r.get(5)?,
            body_hash: r.get(6)?,
            session_id: r.get(7)?,
            source: r.get(8)?,
            metadata: r.get(9)?,
            created_at: r.get(10)?,
            updated_at: r.get(11)?,
        })
    }) {
        Ok(m) => Ok(Some(m)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e)),
    }
}

/// Fetches all memory_ids in a namespace that are soft-deleted and whose
/// `deleted_at` is older than `before_ts` (unix epoch seconds).
///
/// Used by `purge` to collect stale rows for permanent deletion.
///
/// # Errors
///
/// Returns `Err(AppError::Database)` on any `rusqlite` failure.
pub fn list_deleted_before(
    conn: &Connection,
    namespace: &str,
    before_ts: i64,
) -> Result<Vec<i64>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT id FROM memories WHERE namespace = ?1 AND deleted_at IS NOT NULL AND deleted_at < ?2",
    )?;
    let ids = stmt
        .query_map(params![namespace, before_ts], |r| r.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}

/// Executes a prefix-matching FTS5 search against `fts_memories`.
///
/// The supplied `query` is suffixed with `*` to enable prefix matching, then
/// joined back to `memories` to materialize full rows filtered by namespace.
///
/// # Errors
///
/// Returns `Err(AppError::Database)` on any `rusqlite` failure.
pub fn fts_search(
    conn: &Connection,
    query: &str,
    namespace: &str,
    memory_type: Option<&str>,
    limit: usize,
) -> Result<Vec<MemoryRow>, AppError> {
    let fts_query = format!("{query}*");
    if let Some(mt) = memory_type {
        let mut stmt = conn.prepare(
            "SELECT m.id, m.namespace, m.name, m.type, m.description, m.body, m.body_hash,
                    m.session_id, m.source, m.metadata, m.created_at, m.updated_at
             FROM fts_memories fts
             JOIN memories m ON m.id = fts.rowid
             WHERE fts_memories MATCH ?1 AND m.namespace = ?2 AND m.type = ?3 AND m.deleted_at IS NULL
             ORDER BY rank LIMIT ?4",
        )?;
        let rows = stmt
            .query_map(params![fts_query, namespace, mt, limit as i64], |r| {
                Ok(MemoryRow {
                    id: r.get(0)?,
                    namespace: r.get(1)?,
                    name: r.get(2)?,
                    memory_type: r.get(3)?,
                    description: r.get(4)?,
                    body: r.get(5)?,
                    body_hash: r.get(6)?,
                    session_id: r.get(7)?,
                    source: r.get(8)?,
                    metadata: r.get(9)?,
                    created_at: r.get(10)?,
                    updated_at: r.get(11)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    } else {
        let mut stmt = conn.prepare(
            "SELECT m.id, m.namespace, m.name, m.type, m.description, m.body, m.body_hash,
                    m.session_id, m.source, m.metadata, m.created_at, m.updated_at
             FROM fts_memories fts
             JOIN memories m ON m.id = fts.rowid
             WHERE fts_memories MATCH ?1 AND m.namespace = ?2 AND m.deleted_at IS NULL
             ORDER BY rank LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![fts_query, namespace, limit as i64], |r| {
                Ok(MemoryRow {
                    id: r.get(0)?,
                    namespace: r.get(1)?,
                    name: r.get(2)?,
                    memory_type: r.get(3)?,
                    description: r.get(4)?,
                    body: r.get(5)?,
                    body_hash: r.get(6)?,
                    session_id: r.get(7)?,
                    source: r.get(8)?,
                    metadata: r.get(9)?,
                    created_at: r.get(10)?,
                    updated_at: r.get(11)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn setup_conn() -> Result<Connection, Box<dyn std::error::Error>> {
        crate::storage::connection::register_vec_extension();
        let mut conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA temp_store = MEMORY;",
        )?;
        crate::migrations::runner().run(&mut conn)?;
        Ok(conn)
    }

    fn new_memory(name: &str) -> NewMemory {
        NewMemory {
            namespace: "global".to_string(),
            name: name.to_string(),
            memory_type: "user".to_string(),
            description: "descricao de teste".to_string(),
            body: "test memory body".to_string(),
            body_hash: format!("hash-{name}"),
            session_id: None,
            source: "agent".to_string(),
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn insert_and_find_by_name_return_id() -> TestResult {
        let conn = setup_conn()?;
        let m = new_memory("mem-alpha");
        let id = insert(&conn, &m)?;
        assert!(id > 0);

        let found = find_by_name(&conn, "global", "mem-alpha")?;
        assert!(found.is_some());
        let (found_id, _, _) = found.ok_or("mem-alpha should exist")?;
        assert_eq!(found_id, id);
        Ok(())
    }

    #[test]
    fn find_by_name_returns_none_when_not_found() -> TestResult {
        let conn = setup_conn()?;
        let result = find_by_name(&conn, "global", "inexistente")?;
        assert!(result.is_none());
        Ok(())
    }

    #[test]
    fn find_by_hash_returns_correct_id() -> TestResult {
        let conn = setup_conn()?;
        let m = new_memory("mem-hash");
        let id = insert(&conn, &m)?;

        let found = find_by_hash(&conn, "global", "hash-mem-hash")?;
        assert_eq!(found, Some(id));
        Ok(())
    }

    #[test]
    fn find_by_hash_returns_none_when_hash_not_found() -> TestResult {
        let conn = setup_conn()?;
        let result = find_by_hash(&conn, "global", "hash-inexistente")?;
        assert!(result.is_none());
        Ok(())
    }

    #[test]
    fn find_by_hash_ignores_different_namespace() -> TestResult {
        let conn = setup_conn()?;
        let m = new_memory("mem-ns");
        insert(&conn, &m)?;

        let result = find_by_hash(&conn, "outro-namespace", "hash-mem-ns")?;
        assert!(result.is_none());
        Ok(())
    }

    #[test]
    fn read_by_name_returns_full_memory() -> TestResult {
        let conn = setup_conn()?;
        let m = new_memory("mem-read");
        let id = insert(&conn, &m)?;

        let row = read_by_name(&conn, "global", "mem-read")?.ok_or("mem-read should exist")?;
        assert_eq!(row.id, id);
        assert_eq!(row.name, "mem-read");
        assert_eq!(row.memory_type, "user");
        assert_eq!(row.body, "test memory body");
        assert_eq!(row.namespace, "global");
        Ok(())
    }

    #[test]
    fn read_by_name_returns_none_for_missing() -> TestResult {
        let conn = setup_conn()?;
        let result = read_by_name(&conn, "global", "nao-existe")?;
        assert!(result.is_none());
        Ok(())
    }

    #[test]
    fn read_full_by_id_returns_memory() -> TestResult {
        let conn = setup_conn()?;
        let m = new_memory("mem-full");
        let id = insert(&conn, &m)?;

        let row = read_full(&conn, id)?.ok_or("mem-full should exist")?;
        assert_eq!(row.id, id);
        assert_eq!(row.name, "mem-full");
        Ok(())
    }

    #[test]
    fn read_full_returns_none_for_missing_id() -> TestResult {
        let conn = setup_conn()?;
        let result = read_full(&conn, 9999)?;
        assert!(result.is_none());
        Ok(())
    }

    #[test]
    fn update_without_optimism_modifies_fields() -> TestResult {
        let conn = setup_conn()?;
        let m = new_memory("mem-upd");
        let id = insert(&conn, &m)?;

        let mut m2 = new_memory("mem-upd");
        m2.body = "updated body".to_string();
        m2.body_hash = "hash-novo".to_string();
        let ok = update(&conn, id, &m2, None)?;
        assert!(ok);

        let row = read_full(&conn, id)?.ok_or("mem-upd should exist")?;
        assert_eq!(row.body, "updated body");
        assert_eq!(row.body_hash, "hash-novo");
        Ok(())
    }

    #[test]
    fn update_with_correct_expected_updated_at_succeeds() -> TestResult {
        let conn = setup_conn()?;
        let m = new_memory("mem-opt");
        let id = insert(&conn, &m)?;

        let (_, updated_at, _) =
            find_by_name(&conn, "global", "mem-opt")?.ok_or("mem-opt should exist")?;

        let mut m2 = new_memory("mem-opt");
        m2.body = "optimistic body".to_string();
        m2.body_hash = "hash-optimistic".to_string();
        let ok = update(&conn, id, &m2, Some(updated_at))?;
        assert!(ok);

        let row = read_full(&conn, id)?.ok_or("mem-opt should exist after update")?;
        assert_eq!(row.body, "optimistic body");
        Ok(())
    }

    #[test]
    fn update_with_wrong_expected_updated_at_returns_false() -> TestResult {
        let conn = setup_conn()?;
        let m = new_memory("mem-conflict");
        let id = insert(&conn, &m)?;

        let mut m2 = new_memory("mem-conflict");
        m2.body = "must not appear".to_string();
        m2.body_hash = "hash-x".to_string();
        let ok = update(&conn, id, &m2, Some(0))?;
        assert!(!ok);

        let row = read_full(&conn, id)?.ok_or("mem-conflict should exist")?;
        assert_eq!(row.body, "test memory body");
        Ok(())
    }

    #[test]
    fn update_missing_id_returns_false() -> TestResult {
        let conn = setup_conn()?;
        let m = new_memory("fantasma");
        let ok = update(&conn, 9999, &m, None)?;
        assert!(!ok);
        Ok(())
    }

    #[test]
    fn soft_delete_marks_deleted_at() -> TestResult {
        let conn = setup_conn()?;
        let m = new_memory("mem-del");
        insert(&conn, &m)?;

        let ok = soft_delete(&conn, "global", "mem-del")?;
        assert!(ok);

        let result = find_by_name(&conn, "global", "mem-del")?;
        assert!(result.is_none());

        let result_read = read_by_name(&conn, "global", "mem-del")?;
        assert!(result_read.is_none());
        Ok(())
    }

    #[test]
    fn soft_delete_returns_false_when_not_found() -> TestResult {
        let conn = setup_conn()?;
        let ok = soft_delete(&conn, "global", "nao-existe")?;
        assert!(!ok);
        Ok(())
    }

    #[test]
    fn double_soft_delete_returns_false_on_second_call() -> TestResult {
        let conn = setup_conn()?;
        let m = new_memory("mem-del2");
        insert(&conn, &m)?;

        soft_delete(&conn, "global", "mem-del2")?;
        let ok = soft_delete(&conn, "global", "mem-del2")?;
        assert!(!ok);
        Ok(())
    }

    #[test]
    fn list_returns_memories_from_namespace() -> TestResult {
        let conn = setup_conn()?;
        insert(&conn, &new_memory("mem-list-a"))?;
        insert(&conn, &new_memory("mem-list-b"))?;

        let rows = list(&conn, "global", None, 10, 0, false)?;
        assert!(rows.len() >= 2);
        let nomes: Vec<_> = rows.iter().map(|r| r.name.as_str()).collect();
        assert!(nomes.contains(&"mem-list-a"));
        assert!(nomes.contains(&"mem-list-b"));
        Ok(())
    }

    #[test]
    fn list_with_type_filter_returns_only_correct_type() -> TestResult {
        let conn = setup_conn()?;
        insert(&conn, &new_memory("mem-user"))?;

        let mut m2 = new_memory("mem-feedback");
        m2.memory_type = "feedback".to_string();
        insert(&conn, &m2)?;

        let rows_user = list(&conn, "global", Some("user"), 10, 0, false)?;
        assert!(rows_user.iter().all(|r| r.memory_type == "user"));

        let rows_fb = list(&conn, "global", Some("feedback"), 10, 0, false)?;
        assert!(rows_fb.iter().all(|r| r.memory_type == "feedback"));
        Ok(())
    }

    #[test]
    fn list_exclui_soft_deleted() -> TestResult {
        let conn = setup_conn()?;
        let m = new_memory("mem-excluida");
        insert(&conn, &m)?;
        soft_delete(&conn, "global", "mem-excluida")?;

        let rows = list(&conn, "global", None, 10, 0, false)?;
        assert!(rows.iter().all(|r| r.name != "mem-excluida"));
        Ok(())
    }

    #[test]
    fn list_pagination_works() -> TestResult {
        let conn = setup_conn()?;
        for i in 0..5 {
            insert(&conn, &new_memory(&format!("mem-pag-{i}")))?;
        }

        let pagina1 = list(&conn, "global", None, 2, 0, false)?;
        let pagina2 = list(&conn, "global", None, 2, 2, false)?;
        assert!(pagina1.len() <= 2);
        assert!(pagina2.len() <= 2);
        if !pagina1.is_empty() && !pagina2.is_empty() {
            assert_ne!(pagina1[0].id, pagina2[0].id);
        }
        Ok(())
    }

    #[test]
    fn upsert_vec_and_delete_vec_work() -> TestResult {
        let conn = setup_conn()?;
        let m = new_memory("mem-vec");
        let id = insert(&conn, &m)?;

        let embedding: Vec<f32> = vec![0.1; 384];
        upsert_vec(
            &conn, id, "global", "user", &embedding, "mem-vec", "snippet",
        )?;

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM vec_memories WHERE memory_id = ?1",
            params![id],
            |r| r.get(0),
        )?;
        assert_eq!(count, 1);

        delete_vec(&conn, id)?;

        let count_after: i64 = conn.query_row(
            "SELECT COUNT(*) FROM vec_memories WHERE memory_id = ?1",
            params![id],
            |r| r.get(0),
        )?;
        assert_eq!(count_after, 0);
        Ok(())
    }

    #[test]
    fn upsert_vec_replaces_existing_vector() -> TestResult {
        let conn = setup_conn()?;
        let m = new_memory("mem-vec-upsert");
        let id = insert(&conn, &m)?;

        let emb1: Vec<f32> = vec![0.1; 384];
        upsert_vec(&conn, id, "global", "user", &emb1, "mem-vec-upsert", "s1")?;

        let emb2: Vec<f32> = vec![0.9; 384];
        upsert_vec(&conn, id, "global", "user", &emb2, "mem-vec-upsert", "s2")?;

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM vec_memories WHERE memory_id = ?1",
            params![id],
            |r| r.get(0),
        )?;
        assert_eq!(count, 1);
        Ok(())
    }

    #[test]
    fn knn_search_returns_results_by_distance() -> TestResult {
        let conn = setup_conn()?;

        // emb_a: predominantemente positivo — cosseno alto com query [1.0; 384]
        let ma = new_memory("mem-knn-a");
        let id_a = insert(&conn, &ma)?;
        let emb_a: Vec<f32> = vec![1.0; 384];
        upsert_vec(&conn, id_a, "global", "user", &emb_a, "mem-knn-a", "s")?;

        // emb_b: predominantemente negativo — cosseno baixo com query [1.0; 384]
        let mb = new_memory("mem-knn-b");
        let id_b = insert(&conn, &mb)?;
        let emb_b: Vec<f32> = vec![-1.0; 384];
        upsert_vec(&conn, id_b, "global", "user", &emb_b, "mem-knn-b", "s")?;

        let query: Vec<f32> = vec![1.0; 384];
        let results = knn_search(&conn, &query, &["global".to_string()], None, 2)?;
        assert!(!results.is_empty());
        assert_eq!(results[0].0, id_a);
        Ok(())
    }

    #[test]
    fn knn_search_with_type_filter_restricts_result() -> TestResult {
        let conn = setup_conn()?;

        let ma = new_memory("mem-knn-tipo-user");
        let id_a = insert(&conn, &ma)?;
        let emb: Vec<f32> = vec![1.0; 384];
        upsert_vec(
            &conn,
            id_a,
            "global",
            "user",
            &emb,
            "mem-knn-tipo-user",
            "s",
        )?;

        let mut mb = new_memory("mem-knn-tipo-fb");
        mb.memory_type = "feedback".to_string();
        let id_b = insert(&conn, &mb)?;
        upsert_vec(
            &conn,
            id_b,
            "global",
            "feedback",
            &emb,
            "mem-knn-tipo-fb",
            "s",
        )?;

        let query: Vec<f32> = vec![1.0; 384];
        let results_user = knn_search(&conn, &query, &["global".to_string()], Some("user"), 5)?;
        assert!(results_user.iter().all(|(id, _)| *id == id_a));

        let results_fb = knn_search(&conn, &query, &["global".to_string()], Some("feedback"), 5)?;
        assert!(results_fb.iter().all(|(id, _)| *id == id_b));
        Ok(())
    }

    #[test]
    fn fts_search_finds_by_prefix_in_body() -> TestResult {
        let conn = setup_conn()?;
        let mut m = new_memory("mem-fts");
        m.body = "linguagem de programacao rust".to_string();
        insert(&conn, &m)?;

        conn.execute_batch(
            "INSERT INTO fts_memories(rowid, name, description, body)
             SELECT id, name, description, body FROM memories WHERE deleted_at IS NULL",
        )?;

        let rows = fts_search(&conn, "programacao", "global", None, 10)?;
        assert!(!rows.is_empty());
        assert!(rows.iter().any(|r| r.name == "mem-fts"));
        Ok(())
    }

    #[test]
    fn fts_search_with_type_filter() -> TestResult {
        let conn = setup_conn()?;
        let mut m = new_memory("mem-fts-tipo");
        m.body = "linguagem especial para filtro".to_string();
        insert(&conn, &m)?;

        let mut m2 = new_memory("mem-fts-feedback");
        m2.memory_type = "feedback".to_string();
        m2.body = "linguagem especial para filtro".to_string();
        insert(&conn, &m2)?;

        conn.execute_batch(
            "INSERT INTO fts_memories(rowid, name, description, body)
             SELECT id, name, description, body FROM memories WHERE deleted_at IS NULL",
        )?;

        let rows_user = fts_search(&conn, "especial", "global", Some("user"), 10)?;
        assert!(rows_user.iter().all(|r| r.memory_type == "user"));

        let rows_fb = fts_search(&conn, "especial", "global", Some("feedback"), 10)?;
        assert!(rows_fb.iter().all(|r| r.memory_type == "feedback"));
        Ok(())
    }

    #[test]
    fn fts_search_excludes_deleted() -> TestResult {
        let conn = setup_conn()?;
        let mut m = new_memory("mem-fts-del");
        m.body = "deleted fts content".to_string();
        insert(&conn, &m)?;

        conn.execute_batch(
            "INSERT INTO fts_memories(rowid, name, description, body)
             SELECT id, name, description, body FROM memories WHERE deleted_at IS NULL",
        )?;

        soft_delete(&conn, "global", "mem-fts-del")?;

        let rows = fts_search(&conn, "deleted", "global", None, 10)?;
        assert!(rows.iter().all(|r| r.name != "mem-fts-del"));
        Ok(())
    }

    #[test]
    fn list_deleted_before_returns_correct_ids() -> TestResult {
        let conn = setup_conn()?;
        let m = new_memory("mem-purge");
        insert(&conn, &m)?;
        soft_delete(&conn, "global", "mem-purge")?;

        let ids = list_deleted_before(&conn, "global", i64::MAX)?;
        assert!(!ids.is_empty());

        let ids_antes = list_deleted_before(&conn, "global", 0)?;
        assert!(ids_antes.is_empty());
        Ok(())
    }

    #[test]
    fn find_by_name_returns_correct_max_version() -> TestResult {
        let conn = setup_conn()?;
        let m = new_memory("mem-ver");
        let id = insert(&conn, &m)?;

        let (_, _, v0) = find_by_name(&conn, "global", "mem-ver")?.ok_or("mem-ver should exist")?;
        assert_eq!(v0, 0);

        conn.execute(
            "INSERT INTO memory_versions (memory_id, version, name, type, description, body, metadata, change_reason)
             VALUES (?1, 1, 'mem-ver', 'user', 'desc', 'body', '{}', 'create')",
            params![id],
        )?;

        let (_, _, v1) =
            find_by_name(&conn, "global", "mem-ver")?.ok_or("mem-ver should exist after insert")?;
        assert_eq!(v1, 1);
        Ok(())
    }

    #[test]
    fn insert_com_metadata_json() -> TestResult {
        let conn = setup_conn()?;
        let mut m = new_memory("mem-meta");
        m.metadata = serde_json::json!({"chave": "valor", "numero": 42});
        let id = insert(&conn, &m)?;

        let row = read_full(&conn, id)?.ok_or("mem-meta should exist")?;
        let meta: serde_json::Value = serde_json::from_str(&row.metadata)?;
        assert_eq!(meta["chave"], "valor");
        assert_eq!(meta["numero"], 42);
        Ok(())
    }

    #[test]
    fn insert_com_session_id() -> TestResult {
        let conn = setup_conn()?;
        let mut m = new_memory("mem-session");
        m.session_id = Some("sessao-xyz".to_string());
        let id = insert(&conn, &m)?;

        let row = read_full(&conn, id)?.ok_or("mem-session should exist")?;
        assert_eq!(row.session_id, Some("sessao-xyz".to_string()));
        Ok(())
    }

    #[test]
    fn delete_vec_for_nonexistent_id_does_not_fail() -> TestResult {
        let conn = setup_conn()?;
        let result = delete_vec(&conn, 99999);
        assert!(result.is_ok());
        Ok(())
    }
}
