use crate::errors::AppError;
use rusqlite::Connection;

/// URL extraída do corpo de uma memória.
pub struct MemoryUrl {
    pub url: String,
    pub offset: Option<i64>,
}

/// Insere uma URL na tabela `memory_urls`. Ignora duplicatas silenciosamente.
pub fn insert_url(conn: &Connection, memory_id: i64, entry: &MemoryUrl) -> Result<(), AppError> {
    conn.execute(
        "INSERT OR IGNORE INTO memory_urls (memory_id, url, url_offset) VALUES (?1, ?2, ?3)",
        rusqlite::params![memory_id, entry.url, entry.offset],
    )?;
    Ok(())
}

/// Insere múltiplas URLs para uma memória. Retorna a quantidade inserida (duplicatas ignoradas).
/// Erros individuais são logados como warn e não propagados — caminho não crítico.
pub fn insert_urls(conn: &Connection, memory_id: i64, urls: &[MemoryUrl]) -> usize {
    let mut inserted = 0usize;
    for entry in urls {
        match insert_url(conn, memory_id, entry) {
            Ok(()) => {
                let changed = conn.changes();
                if changed > 0 {
                    inserted += 1;
                }
            }
            Err(e) => {
                tracing::warn!("falha ao persistir url '{}': {e:#}", entry.url);
            }
        }
    }
    inserted
}

/// Lista todas as URLs associadas a uma memória.
pub fn list_by_memory(conn: &Connection, memory_id: i64) -> Result<Vec<MemoryUrl>, AppError> {
    let mut stmt =
        conn.prepare("SELECT url, url_offset FROM memory_urls WHERE memory_id = ?1 ORDER BY id")?;
    let rows = stmt.query_map(rusqlite::params![memory_id], |row| {
        Ok(MemoryUrl {
            url: row.get(0)?,
            offset: row.get(1)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Remove todas as URLs de uma memória.
pub fn delete_by_memory(conn: &Connection, memory_id: i64) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM memory_urls WHERE memory_id = ?1",
        rusqlite::params![memory_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;
    use rusqlite::Connection;
    use tempfile::TempDir;

    type Resultado = Result<(), Box<dyn std::error::Error>>;

    fn setup_db() -> Result<(TempDir, Connection), Box<dyn std::error::Error>> {
        crate::storage::connection::register_vec_extension();
        let tmp = TempDir::new()?;
        let db_path = tmp.path().join("test.db");
        let mut conn = Connection::open(&db_path)?;
        crate::migrations::runner().run(&mut conn)?;
        Ok((tmp, conn))
    }

    fn insert_test_memory(conn: &Connection) -> Result<i64, Box<dyn std::error::Error>> {
        conn.execute(
            "INSERT INTO memories (name, type, description, body, body_hash) VALUES ('mem', 'user', 'desc', 'body', 'hash')",
            [],
        )?;
        Ok(conn.last_insert_rowid())
    }

    #[test]
    fn insert_url_persiste_e_list_retorna() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let mem_id = insert_test_memory(&conn)?;

        insert_url(
            &conn,
            mem_id,
            &MemoryUrl {
                url: "https://example.com/page".to_string(),
                offset: Some(5),
            },
        )?;

        let urls = list_by_memory(&conn, mem_id)?;
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].url, "https://example.com/page");
        assert_eq!(urls[0].offset, Some(5));
        Ok(())
    }

    #[test]
    fn insert_url_duplicata_ignorada() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let mem_id = insert_test_memory(&conn)?;

        let entry = MemoryUrl {
            url: "https://example.com/dup".to_string(),
            offset: None,
        };
        insert_url(&conn, mem_id, &entry)?;
        insert_url(&conn, mem_id, &entry)?;

        let urls = list_by_memory(&conn, mem_id)?;
        assert_eq!(urls.len(), 1, "duplicata deve ser ignorada");
        Ok(())
    }

    #[test]
    fn insert_urls_retorna_contagem_inseridas() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let mem_id = insert_test_memory(&conn)?;

        let batch = vec![
            MemoryUrl {
                url: "https://alpha.example.com".to_string(),
                offset: Some(0),
            },
            MemoryUrl {
                url: "https://beta.example.com".to_string(),
                offset: Some(10),
            },
            MemoryUrl {
                url: "https://alpha.example.com".to_string(),
                offset: Some(0),
            },
        ];
        let count = insert_urls(&conn, mem_id, &batch);
        assert_eq!(count, 2, "apenas 2 únicas devem ser inseridas");
        Ok(())
    }

    #[test]
    fn delete_by_memory_remove_todas_urls() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let mem_id = insert_test_memory(&conn)?;

        insert_url(
            &conn,
            mem_id,
            &MemoryUrl {
                url: "https://to-delete.example.com".to_string(),
                offset: None,
            },
        )?;
        assert_eq!(list_by_memory(&conn, mem_id)?.len(), 1);

        delete_by_memory(&conn, mem_id)?;
        assert_eq!(list_by_memory(&conn, mem_id)?.len(), 0);
        Ok(())
    }
}
