//! GAP-001 (v1.0.82): DAO para tabela `pending_memories`.
//!
//! Persistência por estágios com checkpoint retomável. Permite ao `remember` retomar
//! do Estágio B (embedding) sem re-validar Estágio A (parse + validate).
//!
//! Status transitions:
//!   validated → embedding_in_progress → embedding_done → committed
//!                                                    ↘ abandoned (manual cleanup)
//!                                                    ↘ failed (max attempts reached)

use rusqlite::{params, Connection};

use crate::errors::AppError;

/// Status enum de uma entrada pending. Mapeia 1:1 para o CHECK constraint
/// da tabela `pending_memories`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingStatus {
    Validated,
    EmbeddingInProgress,
    EmbeddingDone,
    Committed,
    Abandoned,
    Failed,
}

impl PendingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Validated => "validated",
            Self::EmbeddingInProgress => "embedding_in_progress",
            Self::EmbeddingDone => "embedding_done",
            Self::Committed => "committed",
            Self::Abandoned => "abandoned",
            Self::Failed => "failed",
        }
    }
}

/// Representa uma entrada da tabela `pending_memories`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingMemory {
    pub pending_id: i64,
    pub name: String,
    pub namespace: String,
    pub memory_type: String,
    pub description: Option<String>,
    pub body: Vec<u8>,
    pub body_hash: String,
    pub entities_json: Option<String>,
    pub relationships_json: Option<String>,
    pub status: PendingStatus,
    pub embedding: Option<Vec<u8>>,
    pub embedding_dim: Option<i32>,
    pub attempt_count: i32,
    pub last_error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Insere uma nova entrada em `pending_memories` com status `validated`.
///
/// Retorna o `pending_id` gerado.
#[allow(clippy::too_many_arguments)]
pub fn insert_validated(
    conn: &Connection,
    name: &str,
    namespace: &str,
    memory_type: &str,
    description: Option<&str>,
    body: &[u8],
    body_hash: &str,
    entities_json: Option<&str>,
    relationships_json: Option<&str>,
) -> Result<i64, AppError> {
    conn.execute(
        "INSERT INTO pending_memories
            (name, namespace, memory_type, description, body, body_hash,
             entities_json, relationships_json, status, attempt_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'validated', 0)",
        params![
            name,
            namespace,
            memory_type,
            description,
            body,
            body_hash,
            entities_json,
            relationships_json,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Atualiza status para `embedding_in_progress` e incrementa `attempt_count`.
pub fn update_to_embedding_in_progress(conn: &Connection, pending_id: i64) -> Result<(), AppError> {
    conn.execute(
        "UPDATE pending_memories
         SET status = 'embedding_in_progress',
             attempt_count = attempt_count + 1,
             updated_at = unixepoch()
         WHERE pending_id = ?1",
        params![pending_id],
    )?;
    Ok(())
}

/// Atualiza status para `embedding_done` e armazena o embedding BLOB.
pub fn update_to_embedding_done(
    conn: &Connection,
    pending_id: i64,
    embedding: &[u8],
    dim: i32,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE pending_memories
         SET status = 'embedding_done',
             embedding = ?1,
             embedding_dim = ?2,
             updated_at = unixepoch()
         WHERE pending_id = ?3",
        params![embedding, dim, pending_id],
    )?;
    Ok(())
}

/// Marca como `committed` (chamado após Estágio C com sucesso).
pub fn mark_committed(conn: &Connection, pending_id: i64) -> Result<(), AppError> {
    conn.execute(
        "UPDATE pending_memories
         SET status = 'committed',
             updated_at = unixepoch()
         WHERE pending_id = ?1",
        params![pending_id],
    )?;
    Ok(())
}

/// Marca como `failed` com mensagem de erro.
pub fn mark_failed(conn: &Connection, pending_id: i64, error: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE pending_memories
         SET status = 'failed',
             last_error = ?1,
             updated_at = unixepoch()
         WHERE pending_id = ?2",
        params![error, pending_id],
    )?;
    Ok(())
}

/// Lista entradas por status, ordenadas por `updated_at` ascendente.
pub fn list_by_status(
    conn: &Connection,
    status: PendingStatus,
    limit: usize,
) -> Result<Vec<PendingMemory>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT pending_id, name, namespace, memory_type, description, body,
                body_hash, entities_json, relationships_json, status,
                embedding, embedding_dim, attempt_count, last_error,
                created_at, updated_at
         FROM pending_memories
         WHERE status = ?1
         ORDER BY updated_at ASC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![status.as_str(), limit as i64], |row| {
        Ok(PendingMemory {
            pending_id: row.get(0)?,
            name: row.get(1)?,
            namespace: row.get(2)?,
            memory_type: row.get(3)?,
            description: row.get(4)?,
            body: row.get(5)?,
            body_hash: row.get(6)?,
            entities_json: row.get(7)?,
            relationships_json: row.get(8)?,
            status: parse_status(&row.get::<_, String>(9)?).map_err(|e| -> rusqlite::Error {
                rusqlite::Error::FromSqlConversionFailure(
                    9,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::other(e.to_string())),
                )
            })?,
            embedding: row.get(10)?,
            embedding_dim: row.get(11)?,
            attempt_count: row.get(12)?,
            last_error: row.get(13)?,
            created_at: row.get(14)?,
            updated_at: row.get(15)?,
        })
    })?;
    let mut pending = Vec::new();
    for row in rows {
        pending.push(row?);
    }
    Ok(pending)
}

/// Busca por `pending_id`.
pub fn find_by_id(conn: &Connection, pending_id: i64) -> Result<Option<PendingMemory>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT pending_id, name, namespace, memory_type, description, body,
                body_hash, entities_json, relationships_json, status,
                embedding, embedding_dim, attempt_count, last_error,
                created_at, updated_at
         FROM pending_memories
         WHERE pending_id = ?1",
    )?;
    let mut rows = stmt.query(params![pending_id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(PendingMemory {
            pending_id: row.get(0)?,
            name: row.get(1)?,
            namespace: row.get(2)?,
            memory_type: row.get(3)?,
            description: row.get(4)?,
            body: row.get(5)?,
            body_hash: row.get(6)?,
            entities_json: row.get(7)?,
            relationships_json: row.get(8)?,
            status: parse_status(row.get::<_, String>(9)?.as_str())?,
            embedding: row.get(10)?,
            embedding_dim: row.get(11)?,
            attempt_count: row.get(12)?,
            last_error: row.get(13)?,
            created_at: row.get(14)?,
            updated_at: row.get(15)?,
        }))
    } else {
        Ok(None)
    }
}

/// Remove entradas `embedding_in_progress` mais velhas que `older_than_secs`.
/// Retorna o número de entradas removidas.
pub fn cleanup_older_than(conn: &Connection, older_than_secs: i64) -> Result<usize, AppError> {
    let cutoff = chrono::Utc::now().timestamp() - older_than_secs;
    let count = conn.execute(
        "DELETE FROM pending_memories
         WHERE status IN ('embedding_in_progress', 'validated', 'failed')
           AND updated_at < ?1",
        params![cutoff],
    )?;
    Ok(count)
}

fn parse_status(s: &str) -> Result<PendingStatus, AppError> {
    match s {
        "validated" => Ok(PendingStatus::Validated),
        "embedding_in_progress" => Ok(PendingStatus::EmbeddingInProgress),
        "embedding_done" => Ok(PendingStatus::EmbeddingDone),
        "committed" => Ok(PendingStatus::Committed),
        "abandoned" => Ok(PendingStatus::Abandoned),
        "failed" => Ok(PendingStatus::Failed),
        other => Err(AppError::Validation(format!(
            "unknown pending_memories status: {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn fresh_db() -> Connection {
        let mut conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("pragma");
        crate::migrations::runner()
            .run(&mut conn)
            .expect("migrations apply");
        conn
    }

    #[test]
    fn insert_validated_returns_pending_id() {
        let conn = fresh_db();
        let id = insert_validated(
            &conn,
            "test-pending",
            "global",
            "note",
            Some("desc"),
            b"body bytes",
            "blake3-hash-here",
            None,
            None,
        )
        .expect("insert");
        assert!(id > 0);
    }

    #[test]
    fn status_transition_validated_to_committed() {
        let conn = fresh_db();
        let id =
            insert_validated(&conn, "x", "global", "note", None, b"b", "h", None, None).unwrap();
        update_to_embedding_in_progress(&conn, id).unwrap();
        let p = find_by_id(&conn, id).unwrap().unwrap();
        assert_eq!(p.status, PendingStatus::EmbeddingInProgress);
        assert_eq!(p.attempt_count, 1);

        // Embedding BLOB é &[u8] little-endian — usar bytes brutos para teste
        let fake_emb: Vec<u8> = vec![0u8; 64 * 4]; // 64 * 4 bytes
        update_to_embedding_done(&conn, id, &fake_emb, 64).unwrap();
        let p = find_by_id(&conn, id).unwrap().unwrap();
        assert_eq!(p.status, PendingStatus::EmbeddingDone);
        assert_eq!(p.embedding_dim, Some(64));

        mark_committed(&conn, id).unwrap();
        let p = find_by_id(&conn, id).unwrap().unwrap();
        assert_eq!(p.status, PendingStatus::Committed);
    }

    #[test]
    fn list_by_status_filters_correctly() {
        let conn = fresh_db();
        let id1 =
            insert_validated(&conn, "a", "global", "note", None, b"b", "h", None, None).unwrap();
        let _id2 =
            insert_validated(&conn, "b", "global", "note", None, b"b", "h", None, None).unwrap();
        mark_committed(&conn, id1).unwrap();
        let validated = list_by_status(&conn, PendingStatus::Validated, 10).unwrap();
        assert_eq!(validated.len(), 1);
        assert_eq!(validated[0].name, "b");
    }

    #[test]
    fn cleanup_older_than_removes_stale() {
        let conn = fresh_db();
        let _id = insert_validated(
            &conn, "stale", "global", "note", None, b"b", "h", None, None,
        )
        .unwrap();
        // Cleanup com cutoff no futuro = remove tudo
        let removed = cleanup_older_than(&conn, -3600).unwrap();
        assert_eq!(removed, 1);
    }

    #[test]
    fn mark_failed_records_error() {
        let conn = fresh_db();
        let id =
            insert_validated(&conn, "f", "global", "note", None, b"b", "h", None, None).unwrap();
        mark_failed(&conn, id, "codex exited with OOM").unwrap();
        let p = find_by_id(&conn, id).unwrap().unwrap();
        assert_eq!(p.status, PendingStatus::Failed);
        assert_eq!(p.last_error.as_deref(), Some("codex exited with OOM"));
    }
}
