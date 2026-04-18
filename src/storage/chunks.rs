// src/storage/chunks.rs
// Chunk storage for bodies exceeding 512 tokens E5 limit

use crate::embedder::f32_to_bytes;
use crate::errors::AppError;
use rusqlite::{params, Connection};

#[derive(Debug, Clone)]
pub struct Chunk {
    pub memory_id: i64,
    pub chunk_idx: i32,
    pub chunk_text: String,
    pub start_offset: i32,
    pub end_offset: i32,
    pub token_count: i32,
}

pub fn insert_chunks(conn: &Connection, chunks: &[Chunk]) -> Result<(), AppError> {
    for chunk in chunks {
        conn.execute(
            "INSERT INTO memory_chunks (memory_id, chunk_idx, chunk_text, start_offset, end_offset, token_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                chunk.memory_id,
                chunk.chunk_idx,
                chunk.chunk_text,
                chunk.start_offset,
                chunk.end_offset,
                chunk.token_count,
            ],
        )?;
    }
    Ok(())
}

pub fn upsert_chunk_vec(
    conn: &Connection,
    _rowid: i64,
    memory_id: i64,
    chunk_idx: i32,
    embedding: &[f32],
) -> Result<(), AppError> {
    conn.execute(
        "INSERT OR REPLACE INTO vec_chunks(rowid, memory_id, chunk_idx, embedding)
         VALUES (
             (SELECT id FROM memory_chunks WHERE memory_id = ?1 AND chunk_idx = ?2),
             ?1, ?2, ?3
         )",
        params![memory_id, chunk_idx, f32_to_bytes(embedding)],
    )?;
    Ok(())
}

pub fn delete_chunks(conn: &Connection, memory_id: i64) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM memory_chunks WHERE memory_id = ?1",
        params![memory_id],
    )?;
    Ok(())
}

pub fn knn_search_chunks(
    conn: &Connection,
    embedding: &[f32],
    k: usize,
) -> Result<Vec<(i64, i32, f32)>, AppError> {
    let bytes = f32_to_bytes(embedding);
    let mut stmt = conn.prepare(
        "SELECT memory_id, chunk_idx, distance FROM vec_chunks
         WHERE embedding MATCH ?1
         ORDER BY distance LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![bytes, k as i64], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i32>(1)?,
                r.get::<_, f32>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get_chunks_by_memory(conn: &Connection, memory_id: i64) -> Result<Vec<Chunk>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT memory_id, chunk_idx, chunk_text, start_offset, end_offset, token_count
         FROM memory_chunks WHERE memory_id = ?1 ORDER BY chunk_idx",
    )?;
    let rows = stmt
        .query_map(params![memory_id], |r| {
            Ok(Chunk {
                memory_id: r.get(0)?,
                chunk_idx: r.get(1)?,
                chunk_text: r.get(2)?,
                start_offset: r.get(3)?,
                end_offset: r.get(4)?,
                token_count: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::EMBEDDING_DIM;
    use crate::storage::connection::register_vec_extension;
    use rusqlite::Connection;
    use tempfile::TempDir;

    fn setup_db() -> (TempDir, Connection) {
        register_vec_extension();
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let mut conn = Connection::open(&db_path).unwrap();
        crate::migrations::runner().run(&mut conn).unwrap();
        (tmp, conn)
    }

    fn insert_memory(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO memories (namespace, name, type, description, body, body_hash)
             VALUES ('global', 'test-mem', 'user', 'desc', 'body', 'hash1')",
            [],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn test_insert_chunks_vazia_ok() {
        let (_tmp, conn) = setup_db();
        let resultado = insert_chunks(&conn, &[]);
        assert!(resultado.is_ok());
    }

    #[test]
    fn test_insert_chunks_e_get_por_memory() {
        let (_tmp, conn) = setup_db();
        let memory_id = insert_memory(&conn);

        let chunks = vec![
            Chunk {
                memory_id,
                chunk_idx: 0,
                chunk_text: "primeiro chunk".to_string(),
                start_offset: 0,
                end_offset: 14,
                token_count: 3,
            },
            Chunk {
                memory_id,
                chunk_idx: 1,
                chunk_text: "segundo chunk".to_string(),
                start_offset: 15,
                end_offset: 28,
                token_count: 3,
            },
        ];

        insert_chunks(&conn, &chunks).unwrap();

        let recuperados = get_chunks_by_memory(&conn, memory_id).unwrap();
        assert_eq!(recuperados.len(), 2);
        assert_eq!(recuperados[0].chunk_idx, 0);
        assert_eq!(recuperados[0].chunk_text, "primeiro chunk");
        assert_eq!(recuperados[0].start_offset, 0);
        assert_eq!(recuperados[0].end_offset, 14);
        assert_eq!(recuperados[0].token_count, 3);
        assert_eq!(recuperados[1].chunk_idx, 1);
        assert_eq!(recuperados[1].chunk_text, "segundo chunk");
    }

    #[test]
    fn test_get_chunks_memory_inexistente_retorna_vazio() {
        let (_tmp, conn) = setup_db();
        let resultado = get_chunks_by_memory(&conn, 9999).unwrap();
        assert!(resultado.is_empty());
    }

    #[test]
    fn test_delete_chunks_remove_todos() {
        let (_tmp, conn) = setup_db();
        let memory_id = insert_memory(&conn);

        let chunks = vec![
            Chunk {
                memory_id,
                chunk_idx: 0,
                chunk_text: "chunk a".to_string(),
                start_offset: 0,
                end_offset: 7,
                token_count: 2,
            },
            Chunk {
                memory_id,
                chunk_idx: 1,
                chunk_text: "chunk b".to_string(),
                start_offset: 8,
                end_offset: 15,
                token_count: 2,
            },
        ];
        insert_chunks(&conn, &chunks).unwrap();

        delete_chunks(&conn, memory_id).unwrap();

        let recuperados = get_chunks_by_memory(&conn, memory_id).unwrap();
        assert!(recuperados.is_empty());
    }

    #[test]
    fn test_delete_chunks_memory_sem_chunks_ok() {
        let (_tmp, conn) = setup_db();
        let resultado = delete_chunks(&conn, 9999);
        assert!(resultado.is_ok());
    }

    #[test]
    fn test_get_chunks_ordenados_por_chunk_idx() {
        let (_tmp, conn) = setup_db();
        let memory_id = insert_memory(&conn);

        let chunks = vec![
            Chunk {
                memory_id,
                chunk_idx: 2,
                chunk_text: "terceiro".to_string(),
                start_offset: 20,
                end_offset: 28,
                token_count: 1,
            },
            Chunk {
                memory_id,
                chunk_idx: 0,
                chunk_text: "primeiro".to_string(),
                start_offset: 0,
                end_offset: 8,
                token_count: 1,
            },
            Chunk {
                memory_id,
                chunk_idx: 1,
                chunk_text: "segundo".to_string(),
                start_offset: 9,
                end_offset: 16,
                token_count: 1,
            },
        ];
        insert_chunks(&conn, &chunks).unwrap();

        let recuperados = get_chunks_by_memory(&conn, memory_id).unwrap();
        assert_eq!(recuperados.len(), 3);
        assert_eq!(recuperados[0].chunk_idx, 0);
        assert_eq!(recuperados[1].chunk_idx, 1);
        assert_eq!(recuperados[2].chunk_idx, 2);
    }

    #[test]
    fn test_upsert_chunk_vec_e_knn_search() {
        let (_tmp, conn) = setup_db();
        let memory_id = insert_memory(&conn);

        let chunk = Chunk {
            memory_id,
            chunk_idx: 0,
            chunk_text: "embedding test".to_string(),
            start_offset: 0,
            end_offset: 14,
            token_count: 2,
        };
        insert_chunks(&conn, &[chunk]).unwrap();

        let mut embedding = vec![0.0f32; EMBEDDING_DIM];
        embedding[0] = 1.0;

        let chunk_id: i64 = conn
            .query_row(
                "SELECT id FROM memory_chunks WHERE memory_id = ?1 AND chunk_idx = 0",
                params![memory_id],
                |r| r.get(0),
            )
            .unwrap();

        upsert_chunk_vec(&conn, chunk_id, memory_id, 0, &embedding).unwrap();

        let resultados = knn_search_chunks(&conn, &embedding, 1).unwrap();
        assert_eq!(resultados.len(), 1);
        assert_eq!(resultados[0].0, memory_id);
        assert_eq!(resultados[0].1, 0);
    }

    #[test]
    fn test_knn_search_chunks_sem_dados_retorna_vazio() {
        let (_tmp, conn) = setup_db();
        let embedding = vec![0.0f32; EMBEDDING_DIM];
        let resultado = knn_search_chunks(&conn, &embedding, 5).unwrap();
        assert!(resultado.is_empty());
    }

    #[test]
    fn test_insert_chunks_fk_invalida_falha() {
        let (_tmp, conn) = setup_db();
        let chunk = Chunk {
            memory_id: 99999,
            chunk_idx: 0,
            chunk_text: "sem pai".to_string(),
            start_offset: 0,
            end_offset: 7,
            token_count: 1,
        };
        let resultado = insert_chunks(&conn, &[chunk]);
        assert!(resultado.is_err());
    }
}
