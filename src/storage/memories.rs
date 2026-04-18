//! Persistence layer for the `memories` table and its vector companion.
//!
//! Functions here encapsulate every SQL statement touching `memories`,
//! `vec_memories` and the FTS5 `fts_memories` shadow table. Callers receive
//! typed [`MemoryRow`] or [`NewMemory`] values and never build SQL strings.

use crate::embedder::f32_to_bytes;
use crate::errors::AppError;
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
    // Must delete the existing row first, then insert.
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
            f32_to_bytes(embedding),
            name,
            snippet
        ],
    )?;
    Ok(())
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
) -> Result<Vec<MemoryRow>, AppError> {
    if let Some(mt) = memory_type {
        let mut stmt = conn.prepare(
            "SELECT id, namespace, name, type, description, body, body_hash,
                    session_id, source, metadata, created_at, updated_at
             FROM memories WHERE namespace=?1 AND type=?2 AND deleted_at IS NULL
             ORDER BY updated_at DESC LIMIT ?3 OFFSET ?4",
        )?;
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
        let mut stmt = conn.prepare(
            "SELECT id, namespace, name, type, description, body, body_hash,
                    session_id, source, metadata, created_at, updated_at
             FROM memories WHERE namespace=?1 AND deleted_at IS NULL
             ORDER BY updated_at DESC LIMIT ?2 OFFSET ?3",
        )?;
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

/// Runs a KNN search over `vec_memories` restricted to a namespace.
///
/// # Arguments
///
/// - `embedding` — query vector of length [`crate::constants::EMBEDDING_DIM`].
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
    namespace: &str,
    memory_type: Option<&str>,
    k: usize,
) -> Result<Vec<(i64, f32)>, AppError> {
    let bytes = f32_to_bytes(embedding);
    if let Some(mt) = memory_type {
        let mut stmt = conn.prepare(
            "SELECT memory_id, distance FROM vec_memories
             WHERE embedding MATCH ?1 AND namespace = ?2 AND type = ?3
             ORDER BY distance LIMIT ?4",
        )?;
        let rows = stmt
            .query_map(params![bytes, namespace, mt, k as i64], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, f32>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    } else {
        let mut stmt = conn.prepare(
            "SELECT memory_id, distance FROM vec_memories
             WHERE embedding MATCH ?1 AND namespace = ?2
             ORDER BY distance LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![bytes, namespace, k as i64], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, f32>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
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
mod testes {
    use super::*;
    use rusqlite::Connection;

    fn setup_conn() -> Connection {
        crate::storage::connection::register_vec_extension();
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA temp_store = MEMORY;",
        )
        .unwrap();
        crate::migrations::runner().run(&mut conn).unwrap();
        conn
    }

    fn nova_memoria(name: &str) -> NewMemory {
        NewMemory {
            namespace: "global".to_string(),
            name: name.to_string(),
            memory_type: "user".to_string(),
            description: "descricao de teste".to_string(),
            body: "corpo da memoria de teste".to_string(),
            body_hash: format!("hash-{name}"),
            session_id: None,
            source: "agent".to_string(),
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn insert_e_find_by_name_retornam_id() {
        let conn = setup_conn();
        let m = nova_memoria("mem-alpha");
        let id = insert(&conn, &m).unwrap();
        assert!(id > 0);

        let found = find_by_name(&conn, "global", "mem-alpha").unwrap();
        assert!(found.is_some());
        let (found_id, _, _) = found.unwrap();
        assert_eq!(found_id, id);
    }

    #[test]
    fn find_by_name_retorna_none_quando_nao_existe() {
        let conn = setup_conn();
        let result = find_by_name(&conn, "global", "inexistente").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn find_by_hash_retorna_id_correto() {
        let conn = setup_conn();
        let m = nova_memoria("mem-hash");
        let id = insert(&conn, &m).unwrap();

        let found = find_by_hash(&conn, "global", "hash-mem-hash").unwrap();
        assert_eq!(found, Some(id));
    }

    #[test]
    fn find_by_hash_retorna_none_quando_hash_nao_existe() {
        let conn = setup_conn();
        let result = find_by_hash(&conn, "global", "hash-inexistente").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn find_by_hash_ignora_namespace_diferente() {
        let conn = setup_conn();
        let m = nova_memoria("mem-ns");
        insert(&conn, &m).unwrap();

        let result = find_by_hash(&conn, "outro-namespace", "hash-mem-ns").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn read_by_name_retorna_memoria_completa() {
        let conn = setup_conn();
        let m = nova_memoria("mem-read");
        let id = insert(&conn, &m).unwrap();

        let row = read_by_name(&conn, "global", "mem-read").unwrap().unwrap();
        assert_eq!(row.id, id);
        assert_eq!(row.name, "mem-read");
        assert_eq!(row.memory_type, "user");
        assert_eq!(row.body, "corpo da memoria de teste");
        assert_eq!(row.namespace, "global");
    }

    #[test]
    fn read_by_name_retorna_none_para_ausente() {
        let conn = setup_conn();
        let result = read_by_name(&conn, "global", "nao-existe").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn read_full_por_id_retorna_memoria() {
        let conn = setup_conn();
        let m = nova_memoria("mem-full");
        let id = insert(&conn, &m).unwrap();

        let row = read_full(&conn, id).unwrap().unwrap();
        assert_eq!(row.id, id);
        assert_eq!(row.name, "mem-full");
    }

    #[test]
    fn read_full_retorna_none_para_id_inexistente() {
        let conn = setup_conn();
        let result = read_full(&conn, 9999).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn update_sem_otimismo_modifica_campos() {
        let conn = setup_conn();
        let m = nova_memoria("mem-upd");
        let id = insert(&conn, &m).unwrap();

        let mut m2 = nova_memoria("mem-upd");
        m2.body = "corpo atualizado".to_string();
        m2.body_hash = "hash-novo".to_string();
        let ok = update(&conn, id, &m2, None).unwrap();
        assert!(ok);

        let row = read_full(&conn, id).unwrap().unwrap();
        assert_eq!(row.body, "corpo atualizado");
        assert_eq!(row.body_hash, "hash-novo");
    }

    #[test]
    fn update_com_expected_updated_at_correto_tem_sucesso() {
        let conn = setup_conn();
        let m = nova_memoria("mem-opt");
        let id = insert(&conn, &m).unwrap();

        let (_, updated_at, _) = find_by_name(&conn, "global", "mem-opt").unwrap().unwrap();

        let mut m2 = nova_memoria("mem-opt");
        m2.body = "corpo otimista".to_string();
        m2.body_hash = "hash-otimista".to_string();
        let ok = update(&conn, id, &m2, Some(updated_at)).unwrap();
        assert!(ok);

        let row = read_full(&conn, id).unwrap().unwrap();
        assert_eq!(row.body, "corpo otimista");
    }

    #[test]
    fn update_com_expected_updated_at_errado_retorna_false() {
        let conn = setup_conn();
        let m = nova_memoria("mem-conflict");
        let id = insert(&conn, &m).unwrap();

        let mut m2 = nova_memoria("mem-conflict");
        m2.body = "nao deve aparecer".to_string();
        m2.body_hash = "hash-x".to_string();
        let ok = update(&conn, id, &m2, Some(0)).unwrap();
        assert!(!ok);

        let row = read_full(&conn, id).unwrap().unwrap();
        assert_eq!(row.body, "corpo da memoria de teste");
    }

    #[test]
    fn update_id_inexistente_retorna_false() {
        let conn = setup_conn();
        let m = nova_memoria("fantasma");
        let ok = update(&conn, 9999, &m, None).unwrap();
        assert!(!ok);
    }

    #[test]
    fn soft_delete_marca_deleted_at() {
        let conn = setup_conn();
        let m = nova_memoria("mem-del");
        insert(&conn, &m).unwrap();

        let ok = soft_delete(&conn, "global", "mem-del").unwrap();
        assert!(ok);

        let result = find_by_name(&conn, "global", "mem-del").unwrap();
        assert!(result.is_none());

        let result_read = read_by_name(&conn, "global", "mem-del").unwrap();
        assert!(result_read.is_none());
    }

    #[test]
    fn soft_delete_retorna_false_quando_nao_existe() {
        let conn = setup_conn();
        let ok = soft_delete(&conn, "global", "nao-existe").unwrap();
        assert!(!ok);
    }

    #[test]
    fn soft_delete_duplo_retorna_false_na_segunda_vez() {
        let conn = setup_conn();
        let m = nova_memoria("mem-del2");
        insert(&conn, &m).unwrap();

        soft_delete(&conn, "global", "mem-del2").unwrap();
        let ok = soft_delete(&conn, "global", "mem-del2").unwrap();
        assert!(!ok);
    }

    #[test]
    fn list_retorna_memorias_do_namespace() {
        let conn = setup_conn();
        insert(&conn, &nova_memoria("mem-list-a")).unwrap();
        insert(&conn, &nova_memoria("mem-list-b")).unwrap();

        let rows = list(&conn, "global", None, 10, 0).unwrap();
        assert!(rows.len() >= 2);
        let nomes: Vec<_> = rows.iter().map(|r| r.name.as_str()).collect();
        assert!(nomes.contains(&"mem-list-a"));
        assert!(nomes.contains(&"mem-list-b"));
    }

    #[test]
    fn list_com_filtro_de_tipo_retorna_apenas_tipo_correto() {
        let conn = setup_conn();
        insert(&conn, &nova_memoria("mem-user")).unwrap();

        let mut m2 = nova_memoria("mem-feedback");
        m2.memory_type = "feedback".to_string();
        insert(&conn, &m2).unwrap();

        let rows_user = list(&conn, "global", Some("user"), 10, 0).unwrap();
        assert!(rows_user.iter().all(|r| r.memory_type == "user"));

        let rows_fb = list(&conn, "global", Some("feedback"), 10, 0).unwrap();
        assert!(rows_fb.iter().all(|r| r.memory_type == "feedback"));
    }

    #[test]
    fn list_exclui_soft_deleted() {
        let conn = setup_conn();
        let m = nova_memoria("mem-excluida");
        insert(&conn, &m).unwrap();
        soft_delete(&conn, "global", "mem-excluida").unwrap();

        let rows = list(&conn, "global", None, 10, 0).unwrap();
        assert!(rows.iter().all(|r| r.name != "mem-excluida"));
    }

    #[test]
    fn list_paginacao_funciona() {
        let conn = setup_conn();
        for i in 0..5 {
            insert(&conn, &nova_memoria(&format!("mem-pag-{i}"))).unwrap();
        }

        let pagina1 = list(&conn, "global", None, 2, 0).unwrap();
        let pagina2 = list(&conn, "global", None, 2, 2).unwrap();
        assert!(pagina1.len() <= 2);
        assert!(pagina2.len() <= 2);
        if !pagina1.is_empty() && !pagina2.is_empty() {
            assert_ne!(pagina1[0].id, pagina2[0].id);
        }
    }

    #[test]
    fn upsert_vec_e_delete_vec_funcionam() {
        let conn = setup_conn();
        let m = nova_memoria("mem-vec");
        let id = insert(&conn, &m).unwrap();

        let embedding: Vec<f32> = vec![0.1; 384];
        upsert_vec(
            &conn, id, "global", "user", &embedding, "mem-vec", "snippet",
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM vec_memories WHERE memory_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        delete_vec(&conn, id).unwrap();

        let count_after: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM vec_memories WHERE memory_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count_after, 0);
    }

    #[test]
    fn upsert_vec_substitui_vetor_existente() {
        let conn = setup_conn();
        let m = nova_memoria("mem-vec-upsert");
        let id = insert(&conn, &m).unwrap();

        let emb1: Vec<f32> = vec![0.1; 384];
        upsert_vec(&conn, id, "global", "user", &emb1, "mem-vec-upsert", "s1").unwrap();

        let emb2: Vec<f32> = vec![0.9; 384];
        upsert_vec(&conn, id, "global", "user", &emb2, "mem-vec-upsert", "s2").unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM vec_memories WHERE memory_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn knn_search_retorna_resultados_por_distancia() {
        let conn = setup_conn();

        // emb_a: predominantemente positivo — cosseno alto com query [1.0; 384]
        let ma = nova_memoria("mem-knn-a");
        let id_a = insert(&conn, &ma).unwrap();
        let emb_a: Vec<f32> = vec![1.0; 384];
        upsert_vec(&conn, id_a, "global", "user", &emb_a, "mem-knn-a", "s").unwrap();

        // emb_b: predominantemente negativo — cosseno baixo com query [1.0; 384]
        let mb = nova_memoria("mem-knn-b");
        let id_b = insert(&conn, &mb).unwrap();
        let emb_b: Vec<f32> = vec![-1.0; 384];
        upsert_vec(&conn, id_b, "global", "user", &emb_b, "mem-knn-b", "s").unwrap();

        let query: Vec<f32> = vec![1.0; 384];
        let results = knn_search(&conn, &query, "global", None, 2).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].0, id_a);
    }

    #[test]
    fn knn_search_com_filtro_de_tipo_restringe_resultado() {
        let conn = setup_conn();

        let ma = nova_memoria("mem-knn-tipo-user");
        let id_a = insert(&conn, &ma).unwrap();
        let emb: Vec<f32> = vec![1.0; 384];
        upsert_vec(
            &conn,
            id_a,
            "global",
            "user",
            &emb,
            "mem-knn-tipo-user",
            "s",
        )
        .unwrap();

        let mut mb = nova_memoria("mem-knn-tipo-fb");
        mb.memory_type = "feedback".to_string();
        let id_b = insert(&conn, &mb).unwrap();
        upsert_vec(
            &conn,
            id_b,
            "global",
            "feedback",
            &emb,
            "mem-knn-tipo-fb",
            "s",
        )
        .unwrap();

        let query: Vec<f32> = vec![1.0; 384];
        let results_user = knn_search(&conn, &query, "global", Some("user"), 5).unwrap();
        assert!(results_user.iter().all(|(id, _)| *id == id_a));

        let results_fb = knn_search(&conn, &query, "global", Some("feedback"), 5).unwrap();
        assert!(results_fb.iter().all(|(id, _)| *id == id_b));
    }

    #[test]
    fn fts_search_encontra_por_prefixo_no_body() {
        let conn = setup_conn();
        let mut m = nova_memoria("mem-fts");
        m.body = "linguagem de programacao rust".to_string();
        insert(&conn, &m).unwrap();

        conn.execute_batch(
            "INSERT INTO fts_memories(rowid, name, description, body)
             SELECT id, name, description, body FROM memories WHERE deleted_at IS NULL",
        )
        .unwrap();

        let rows = fts_search(&conn, "programacao", "global", None, 10).unwrap();
        assert!(!rows.is_empty());
        assert!(rows.iter().any(|r| r.name == "mem-fts"));
    }

    #[test]
    fn fts_search_com_filtro_de_tipo() {
        let conn = setup_conn();
        let mut m = nova_memoria("mem-fts-tipo");
        m.body = "linguagem especial para filtro".to_string();
        insert(&conn, &m).unwrap();

        let mut m2 = nova_memoria("mem-fts-feedback");
        m2.memory_type = "feedback".to_string();
        m2.body = "linguagem especial para filtro".to_string();
        insert(&conn, &m2).unwrap();

        conn.execute_batch(
            "INSERT INTO fts_memories(rowid, name, description, body)
             SELECT id, name, description, body FROM memories WHERE deleted_at IS NULL",
        )
        .unwrap();

        let rows_user = fts_search(&conn, "especial", "global", Some("user"), 10).unwrap();
        assert!(rows_user.iter().all(|r| r.memory_type == "user"));

        let rows_fb = fts_search(&conn, "especial", "global", Some("feedback"), 10).unwrap();
        assert!(rows_fb.iter().all(|r| r.memory_type == "feedback"));
    }

    #[test]
    fn fts_search_nao_retorna_deletados() {
        let conn = setup_conn();
        let mut m = nova_memoria("mem-fts-del");
        m.body = "conteudo deletado fts".to_string();
        insert(&conn, &m).unwrap();

        conn.execute_batch(
            "INSERT INTO fts_memories(rowid, name, description, body)
             SELECT id, name, description, body FROM memories WHERE deleted_at IS NULL",
        )
        .unwrap();

        soft_delete(&conn, "global", "mem-fts-del").unwrap();

        let rows = fts_search(&conn, "deletado", "global", None, 10).unwrap();
        assert!(rows.iter().all(|r| r.name != "mem-fts-del"));
    }

    #[test]
    fn list_deleted_before_retorna_ids_corretos() {
        let conn = setup_conn();
        let m = nova_memoria("mem-purge");
        insert(&conn, &m).unwrap();
        soft_delete(&conn, "global", "mem-purge").unwrap();

        let ids = list_deleted_before(&conn, "global", i64::MAX).unwrap();
        assert!(!ids.is_empty());

        let ids_antes = list_deleted_before(&conn, "global", 0).unwrap();
        assert!(ids_antes.is_empty());
    }

    #[test]
    fn find_by_name_retorna_max_version_correto() {
        let conn = setup_conn();
        let m = nova_memoria("mem-ver");
        let id = insert(&conn, &m).unwrap();

        let (_, _, v0) = find_by_name(&conn, "global", "mem-ver").unwrap().unwrap();
        assert_eq!(v0, 0);

        conn.execute(
            "INSERT INTO memory_versions (memory_id, version, name, type, description, body, metadata, change_reason)
             VALUES (?1, 1, 'mem-ver', 'user', 'desc', 'body', '{}', 'create')",
            params![id],
        )
        .unwrap();

        let (_, _, v1) = find_by_name(&conn, "global", "mem-ver").unwrap().unwrap();
        assert_eq!(v1, 1);
    }

    #[test]
    fn insert_com_metadata_json() {
        let conn = setup_conn();
        let mut m = nova_memoria("mem-meta");
        m.metadata = serde_json::json!({"chave": "valor", "numero": 42});
        let id = insert(&conn, &m).unwrap();

        let row = read_full(&conn, id).unwrap().unwrap();
        let meta: serde_json::Value = serde_json::from_str(&row.metadata).unwrap();
        assert_eq!(meta["chave"], "valor");
        assert_eq!(meta["numero"], 42);
    }

    #[test]
    fn insert_com_session_id() {
        let conn = setup_conn();
        let mut m = nova_memoria("mem-session");
        m.session_id = Some("sessao-xyz".to_string());
        let id = insert(&conn, &m).unwrap();

        let row = read_full(&conn, id).unwrap().unwrap();
        assert_eq!(row.session_id, Some("sessao-xyz".to_string()));
    }

    #[test]
    fn delete_vec_em_id_inexistente_nao_falha() {
        let conn = setup_conn();
        let result = delete_vec(&conn, 99999);
        assert!(result.is_ok());
    }
}
