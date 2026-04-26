//! Persistence layer for entities, relationships and their junction tables.
//!
//! The entity graph mirrors the conceptual content of memories: `entities`
//! holds nodes, `relationships` holds typed edges and `memory_entities` and
//! `memory_relationships` connect each memory to the graph slice it emitted.

use crate::embedder::f32_to_bytes;
use crate::errors::AppError;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Input payload used to upsert a single entity.
///
/// `name` is normalized to kebab-case by the caller. `description` is
/// optional and preserved across upserts when the new value is `None`.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct NewEntity {
    pub name: String,
    #[serde(alias = "type")]
    pub entity_type: String,
    pub description: Option<String>,
}

/// Input payload used to upsert a typed relationship between entities.
///
/// `strength` must lie within `[0.0, 1.0]` and is mapped to the `weight`
/// column of the `relationships` table.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct NewRelationship {
    pub source: String,
    pub target: String,
    pub relation: String,
    pub strength: f64,
    pub description: Option<String>,
}

/// Upserts an entity and returns its primary key.
///
/// Uses `ON CONFLICT(namespace, name)` to keep one row per entity within a
/// namespace, refreshing `type` and `description` opportunistically.
///
/// # Errors
///
/// Returns `Err(AppError::Database)` on any `rusqlite` failure.
pub fn upsert_entity(conn: &Connection, namespace: &str, e: &NewEntity) -> Result<i64, AppError> {
    conn.execute(
        "INSERT INTO entities (namespace, name, type, description)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(namespace, name) DO UPDATE SET
           type        = excluded.type,
           description = COALESCE(excluded.description, entities.description),
           updated_at  = unixepoch()",
        params![namespace, e.name, e.entity_type, e.description],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM entities WHERE namespace = ?1 AND name = ?2",
        params![namespace, e.name],
        |r| r.get(0),
    )?;
    Ok(id)
}

/// Replaces the vector row for an entity in `vec_entities`.
///
/// vec0 virtual tables do not honour `INSERT OR REPLACE` when the primary key
/// already exists — they raise a UNIQUE constraint error instead of silently
/// replacing the row. The workaround is an explicit DELETE before INSERT so
/// that the insert never conflicts. `embedding` must have length
/// [`crate::constants::EMBEDDING_DIM`].
///
/// # Errors
///
/// Returns `Err(AppError::Database)` on any `rusqlite` failure.
pub fn upsert_entity_vec(
    conn: &Connection,
    entity_id: i64,
    namespace: &str,
    entity_type: &str,
    embedding: &[f32],
    name: &str,
) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM vec_entities WHERE entity_id = ?1",
        params![entity_id],
    )?;
    conn.execute(
        "INSERT INTO vec_entities(entity_id, namespace, type, embedding, name)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            entity_id,
            namespace,
            entity_type,
            f32_to_bytes(embedding),
            name
        ],
    )?;
    Ok(())
}

/// Upserts a typed relationship between two entity ids.
///
/// Conflicts on `(source_id, target_id, relation)` refresh `weight` and
/// preserve a non-null `description`. Returns the `rowid` of the stored row.
///
/// # Errors
///
/// Returns `Err(AppError::Database)` on any `rusqlite` failure.
pub fn upsert_relationship(
    conn: &Connection,
    namespace: &str,
    source_id: i64,
    target_id: i64,
    rel: &NewRelationship,
) -> Result<i64, AppError> {
    conn.execute(
        "INSERT INTO relationships (namespace, source_id, target_id, relation, weight, description)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(source_id, target_id, relation) DO UPDATE SET
           weight = excluded.weight,
           description = COALESCE(excluded.description, relationships.description)",
        params![
            namespace,
            source_id,
            target_id,
            rel.relation,
            rel.strength,
            rel.description
        ],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM relationships WHERE source_id=?1 AND target_id=?2 AND relation=?3",
        params![source_id, target_id, rel.relation],
        |r| r.get(0),
    )?;
    Ok(id)
}

pub fn link_memory_entity(
    conn: &Connection,
    memory_id: i64,
    entity_id: i64,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT OR IGNORE INTO memory_entities (memory_id, entity_id) VALUES (?1, ?2)",
        params![memory_id, entity_id],
    )?;
    Ok(())
}

pub fn link_memory_relationship(
    conn: &Connection,
    memory_id: i64,
    rel_id: i64,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT OR IGNORE INTO memory_relationships (memory_id, relationship_id) VALUES (?1, ?2)",
        params![memory_id, rel_id],
    )?;
    Ok(())
}

pub fn increment_degree(conn: &Connection, entity_id: i64) -> Result<(), AppError> {
    conn.execute(
        "UPDATE entities SET degree = degree + 1 WHERE id = ?1",
        params![entity_id],
    )?;
    Ok(())
}

/// Busca a entidade por nome e namespace. Retorna o id se existir.
pub fn find_entity_id(
    conn: &Connection,
    namespace: &str,
    name: &str,
) -> Result<Option<i64>, AppError> {
    let mut stmt =
        conn.prepare_cached("SELECT id FROM entities WHERE namespace = ?1 AND name = ?2")?;
    match stmt.query_row(params![namespace, name], |r| r.get::<_, i64>(0)) {
        Ok(id) => Ok(Some(id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e)),
    }
}

/// Estrutura representando uma relação existente.
#[derive(Debug, Serialize)]
pub struct RelationshipRow {
    pub id: i64,
    pub namespace: String,
    pub source_id: i64,
    pub target_id: i64,
    pub relation: String,
    pub weight: f64,
    pub description: Option<String>,
}

/// Busca uma relação específica por (source_id, target_id, relation).
pub fn find_relationship(
    conn: &Connection,
    source_id: i64,
    target_id: i64,
    relation: &str,
) -> Result<Option<RelationshipRow>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, namespace, source_id, target_id, relation, weight, description
         FROM relationships
         WHERE source_id = ?1 AND target_id = ?2 AND relation = ?3",
    )?;
    match stmt.query_row(params![source_id, target_id, relation], |r| {
        Ok(RelationshipRow {
            id: r.get(0)?,
            namespace: r.get(1)?,
            source_id: r.get(2)?,
            target_id: r.get(3)?,
            relation: r.get(4)?,
            weight: r.get(5)?,
            description: r.get(6)?,
        })
    }) {
        Ok(row) => Ok(Some(row)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e)),
    }
}

/// Cria uma relação se não existir (retorna action="created")
/// ou retorna a relação existente (action="already_exists") com peso atualizado.
pub fn create_or_fetch_relationship(
    conn: &Connection,
    namespace: &str,
    source_id: i64,
    target_id: i64,
    relation: &str,
    weight: f64,
    description: Option<&str>,
) -> Result<(i64, bool), AppError> {
    // Check if it exists first.
    let existing = find_relationship(conn, source_id, target_id, relation)?;
    if let Some(row) = existing {
        return Ok((row.id, false));
    }
    conn.execute(
        "INSERT INTO relationships (namespace, source_id, target_id, relation, weight, description)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            namespace,
            source_id,
            target_id,
            relation,
            weight,
            description
        ],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM relationships WHERE source_id = ?1 AND target_id = ?2 AND relation = ?3",
        params![source_id, target_id, relation],
        |r| r.get(0),
    )?;
    Ok((id, true))
}

/// Remove uma relação pelo id e limpa memory_relationships.
pub fn delete_relationship_by_id(conn: &Connection, relationship_id: i64) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM memory_relationships WHERE relationship_id = ?1",
        params![relationship_id],
    )?;
    conn.execute(
        "DELETE FROM relationships WHERE id = ?1",
        params![relationship_id],
    )?;
    Ok(())
}

/// Recalcula o campo `degree` de uma entidade.
pub fn recalculate_degree(conn: &Connection, entity_id: i64) -> Result<(), AppError> {
    conn.execute(
        "UPDATE entities
         SET degree = (SELECT COUNT(*) FROM relationships
                       WHERE source_id = entities.id OR target_id = entities.id)
         WHERE id = ?1",
        params![entity_id],
    )?;
    Ok(())
}

/// Linha de entidade com dados suficientes para exportação/consulta de grafo.
#[derive(Debug, Serialize, Clone)]
pub struct EntityNode {
    pub id: i64,
    pub name: String,
    pub namespace: String,
    pub kind: String,
}

/// Lista entidades, filtrando por namespace se fornecido.
pub fn list_entities(
    conn: &Connection,
    namespace: Option<&str>,
) -> Result<Vec<EntityNode>, AppError> {
    if let Some(ns) = namespace {
        let mut stmt = conn.prepare(
            "SELECT id, name, namespace, type FROM entities WHERE namespace = ?1 ORDER BY id",
        )?;
        let rows = stmt
            .query_map(params![ns], |r| {
                Ok(EntityNode {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    namespace: r.get(2)?,
                    kind: r.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    } else {
        let mut stmt =
            conn.prepare("SELECT id, name, namespace, type FROM entities ORDER BY namespace, id")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(EntityNode {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    namespace: r.get(2)?,
                    kind: r.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

/// Lista relações filtradas por namespace (das entidades de origem/destino).
pub fn list_relationships_by_namespace(
    conn: &Connection,
    namespace: Option<&str>,
) -> Result<Vec<RelationshipRow>, AppError> {
    if let Some(ns) = namespace {
        let mut stmt = conn.prepare(
            "SELECT r.id, r.namespace, r.source_id, r.target_id, r.relation, r.weight, r.description
             FROM relationships r
             JOIN entities se ON se.id = r.source_id AND se.namespace = ?1
             JOIN entities te ON te.id = r.target_id AND te.namespace = ?1
             ORDER BY r.id",
        )?;
        let rows = stmt
            .query_map(params![ns], |r| {
                Ok(RelationshipRow {
                    id: r.get(0)?,
                    namespace: r.get(1)?,
                    source_id: r.get(2)?,
                    target_id: r.get(3)?,
                    relation: r.get(4)?,
                    weight: r.get(5)?,
                    description: r.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, namespace, source_id, target_id, relation, weight, description
             FROM relationships ORDER BY id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(RelationshipRow {
                    id: r.get(0)?,
                    namespace: r.get(1)?,
                    source_id: r.get(2)?,
                    target_id: r.get(3)?,
                    relation: r.get(4)?,
                    weight: r.get(5)?,
                    description: r.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

/// Localiza entidades órfãs: sem vínculo em memory_entities e sem relações.
pub fn find_orphan_entity_ids(
    conn: &Connection,
    namespace: Option<&str>,
) -> Result<Vec<i64>, AppError> {
    if let Some(ns) = namespace {
        let mut stmt = conn.prepare(
            "SELECT e.id FROM entities e
             WHERE e.namespace = ?1
               AND NOT EXISTS (SELECT 1 FROM memory_entities me WHERE me.entity_id = e.id)
               AND NOT EXISTS (
                   SELECT 1 FROM relationships r
                   WHERE r.source_id = e.id OR r.target_id = e.id
               )",
        )?;
        let ids = stmt
            .query_map(params![ns], |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    } else {
        let mut stmt = conn.prepare(
            "SELECT e.id FROM entities e
             WHERE NOT EXISTS (SELECT 1 FROM memory_entities me WHERE me.entity_id = e.id)
               AND NOT EXISTS (
                   SELECT 1 FROM relationships r
                   WHERE r.source_id = e.id OR r.target_id = e.id
               )",
        )?;
        let ids = stmt
            .query_map([], |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    }
}

/// Deleta entidades e seus vetores associados. Retorna o número de entidades removidas.
pub fn delete_entities_by_ids(conn: &Connection, entity_ids: &[i64]) -> Result<usize, AppError> {
    if entity_ids.is_empty() {
        return Ok(0);
    }
    let mut removed = 0usize;
    for id in entity_ids {
        // vec0 lacks FK CASCADE — clean vec_entities explicitly.
        let _ = conn.execute("DELETE FROM vec_entities WHERE entity_id = ?1", params![id]);
        let affected = conn.execute("DELETE FROM entities WHERE id = ?1", params![id])?;
        removed += affected;
    }
    Ok(removed)
}

pub fn knn_search(
    conn: &Connection,
    embedding: &[f32],
    namespace: &str,
    k: usize,
) -> Result<Vec<(i64, f32)>, AppError> {
    let bytes = f32_to_bytes(embedding);
    let mut stmt = conn.prepare(
        "SELECT entity_id, distance FROM vec_entities
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::EMBEDDING_DIM;
    use crate::storage::connection::register_vec_extension;
    use rusqlite::Connection;
    use tempfile::TempDir;

    type Resultado = Result<(), Box<dyn std::error::Error>>;

    fn setup_db() -> Result<(TempDir, Connection), Box<dyn std::error::Error>> {
        register_vec_extension();
        let tmp = TempDir::new()?;
        let db_path = tmp.path().join("test.db");
        let mut conn = Connection::open(&db_path)?;
        crate::migrations::runner().run(&mut conn)?;
        Ok((tmp, conn))
    }

    fn insert_memory(conn: &Connection) -> Result<i64, Box<dyn std::error::Error>> {
        conn.execute(
            "INSERT INTO memories (namespace, name, type, description, body, body_hash)
             VALUES ('global', 'test-mem', 'user', 'desc', 'body', 'hash1')",
            [],
        )?;
        Ok(conn.last_insert_rowid())
    }

    fn nova_entidade(name: &str) -> NewEntity {
        NewEntity {
            name: name.to_string(),
            entity_type: "project".to_string(),
            description: None,
        }
    }

    fn embedding_zero() -> Vec<f32> {
        vec![0.0f32; EMBEDDING_DIM]
    }

    // ------------------------------------------------------------------ //
    // upsert_entity
    // ------------------------------------------------------------------ //

    #[test]
    fn test_upsert_entity_cria_nova() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let e = nova_entidade("projeto-alpha");
        let id = upsert_entity(&conn, "global", &e)?;
        assert!(id > 0);
        Ok(())
    }

    #[test]
    fn test_upsert_entity_idempotente_retorna_mesmo_id() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let e = nova_entidade("projeto-beta");
        let id1 = upsert_entity(&conn, "global", &e)?;
        let id2 = upsert_entity(&conn, "global", &e)?;
        assert_eq!(id1, id2);
        Ok(())
    }

    #[test]
    fn test_upsert_entity_atualiza_descricao() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let e1 = nova_entidade("projeto-gamma");
        let id1 = upsert_entity(&conn, "global", &e1)?;

        let e2 = NewEntity {
            name: "projeto-gamma".to_string(),
            entity_type: "tool".to_string(),
            description: Some("nova desc".to_string()),
        };
        let id2 = upsert_entity(&conn, "global", &e2)?;
        assert_eq!(id1, id2);

        let desc: Option<String> = conn.query_row(
            "SELECT description FROM entities WHERE id = ?1",
            params![id1],
            |r| r.get(0),
        )?;
        assert_eq!(desc.as_deref(), Some("nova desc"));
        Ok(())
    }

    #[test]
    fn test_upsert_entity_namespaces_diferentes_criam_registros_distintos() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let e = nova_entidade("compartilhada");
        let id1 = upsert_entity(&conn, "ns1", &e)?;
        let id2 = upsert_entity(&conn, "ns2", &e)?;
        assert_ne!(id1, id2);
        Ok(())
    }

    // ------------------------------------------------------------------ //
    // upsert_entity_vec — cobre DELETE+INSERT (branch novo após fix OOM)
    // ------------------------------------------------------------------ //

    #[test]
    fn test_upsert_entity_vec_primeira_vez_sem_conflito() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let e = nova_entidade("vec-nova");
        let entity_id = upsert_entity(&conn, "global", &e)?;
        let emb = embedding_zero();

        let resultado = upsert_entity_vec(&conn, entity_id, "global", "project", &emb, "vec-nova");
        assert!(resultado.is_ok(), "primeira inserção deve ter sucesso");

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM vec_entities WHERE entity_id = ?1",
            params![entity_id],
            |r| r.get(0),
        )?;
        assert_eq!(count, 1, "deve existir exatamente uma linha após inserção");
        Ok(())
    }

    #[test]
    fn test_upsert_entity_vec_segunda_vez_substitui_sem_erro() -> Resultado {
        // Cobre o branch onde DELETE remove a linha existente antes do INSERT.
        let (_tmp, conn) = setup_db()?;
        let e = nova_entidade("vec-existente");
        let entity_id = upsert_entity(&conn, "global", &e)?;
        let emb = embedding_zero();

        upsert_entity_vec(&conn, entity_id, "global", "project", &emb, "vec-existente")?;

        // Segunda chamada: DELETE retorna 1 linha removida, INSERT deve ter sucesso.
        let resultado =
            upsert_entity_vec(&conn, entity_id, "global", "tool", &emb, "vec-existente");
        assert!(
            resultado.is_ok(),
            "segunda inserção (replace) deve ter sucesso: {resultado:?}"
        );

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM vec_entities WHERE entity_id = ?1",
            params![entity_id],
            |r| r.get(0),
        )?;
        assert_eq!(
            count, 1,
            "deve existir exatamente uma linha após substituição"
        );
        Ok(())
    }

    #[test]
    fn test_upsert_entity_vec_multiplas_entidades_independentes() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let emb = embedding_zero();

        for i in 0..3i64 {
            let nome = format!("ent-{i}");
            let e = nova_entidade(&nome);
            let entity_id = upsert_entity(&conn, "global", &e)?;
            upsert_entity_vec(&conn, entity_id, "global", "project", &emb, &nome)?;
        }

        let count: i64 = conn.query_row("SELECT COUNT(*) FROM vec_entities", [], |r| r.get(0))?;
        assert_eq!(count, 3, "deve haver três linhas distintas em vec_entities");
        Ok(())
    }

    // ------------------------------------------------------------------ //
    // find_entity_id
    // ------------------------------------------------------------------ //

    #[test]
    fn test_find_entity_id_existente_retorna_some() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let e = nova_entidade("entidade-busca");
        let id_inserido = upsert_entity(&conn, "global", &e)?;
        let id_encontrado = find_entity_id(&conn, "global", "entidade-busca")?;
        assert_eq!(id_encontrado, Some(id_inserido));
        Ok(())
    }

    #[test]
    fn test_find_entity_id_inexistente_retorna_none() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let id = find_entity_id(&conn, "global", "nao-existe")?;
        assert_eq!(id, None);
        Ok(())
    }

    // ------------------------------------------------------------------ //
    // delete_entities_by_ids
    // ------------------------------------------------------------------ //

    #[test]
    fn test_delete_entities_by_ids_lista_vazia_retorna_zero() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let removidos = delete_entities_by_ids(&conn, &[])?;
        assert_eq!(removidos, 0);
        Ok(())
    }

    #[test]
    fn test_delete_entities_by_ids_remove_entidade_valida() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let e = nova_entidade("para-deletar");
        let entity_id = upsert_entity(&conn, "global", &e)?;

        let removidos = delete_entities_by_ids(&conn, &[entity_id])?;
        assert_eq!(removidos, 1);

        let id = find_entity_id(&conn, "global", "para-deletar")?;
        assert_eq!(id, None, "entidade deve ter sido removida");
        Ok(())
    }

    #[test]
    fn test_delete_entities_by_ids_id_inexistente_retorna_zero() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let removidos = delete_entities_by_ids(&conn, &[9999])?;
        assert_eq!(removidos, 0);
        Ok(())
    }

    #[test]
    fn test_delete_entities_by_ids_remove_multiplas() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let id1 = upsert_entity(&conn, "global", &nova_entidade("del-a"))?;
        let id2 = upsert_entity(&conn, "global", &nova_entidade("del-b"))?;
        let id3 = upsert_entity(&conn, "global", &nova_entidade("del-c"))?;

        let removidos = delete_entities_by_ids(&conn, &[id1, id2])?;
        assert_eq!(removidos, 2);

        assert!(find_entity_id(&conn, "global", "del-a")?.is_none());
        assert!(find_entity_id(&conn, "global", "del-b")?.is_none());
        assert!(find_entity_id(&conn, "global", "del-c")?.is_some());
        let _ = id3;
        Ok(())
    }

    #[test]
    fn test_delete_entities_by_ids_tambem_remove_vec() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let e = nova_entidade("del-com-vec");
        let entity_id = upsert_entity(&conn, "global", &e)?;
        let emb = embedding_zero();
        upsert_entity_vec(&conn, entity_id, "global", "project", &emb, "del-com-vec")?;

        let count_antes: i64 = conn.query_row(
            "SELECT COUNT(*) FROM vec_entities WHERE entity_id = ?1",
            params![entity_id],
            |r| r.get(0),
        )?;
        assert_eq!(count_antes, 1);

        delete_entities_by_ids(&conn, &[entity_id])?;

        let count_depois: i64 = conn.query_row(
            "SELECT COUNT(*) FROM vec_entities WHERE entity_id = ?1",
            params![entity_id],
            |r| r.get(0),
        )?;
        assert_eq!(
            count_depois, 0,
            "vec_entities deve ser limpo junto com entities"
        );
        Ok(())
    }

    // ------------------------------------------------------------------ //
    // upsert_relationship / find_relationship
    // ------------------------------------------------------------------ //

    #[test]
    fn test_upsert_relationship_cria_nova() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let id_a = upsert_entity(&conn, "global", &nova_entidade("rel-a"))?;
        let id_b = upsert_entity(&conn, "global", &nova_entidade("rel-b"))?;

        let rel = NewRelationship {
            source: "rel-a".to_string(),
            target: "rel-b".to_string(),
            relation: "uses".to_string(),
            strength: 0.8,
            description: None,
        };
        let rel_id = upsert_relationship(&conn, "global", id_a, id_b, &rel)?;
        assert!(rel_id > 0);
        Ok(())
    }

    #[test]
    fn test_upsert_relationship_idempotente() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let id_a = upsert_entity(&conn, "global", &nova_entidade("idem-a"))?;
        let id_b = upsert_entity(&conn, "global", &nova_entidade("idem-b"))?;

        let rel = NewRelationship {
            source: "idem-a".to_string(),
            target: "idem-b".to_string(),
            relation: "uses".to_string(),
            strength: 0.5,
            description: None,
        };
        let id1 = upsert_relationship(&conn, "global", id_a, id_b, &rel)?;
        let id2 = upsert_relationship(&conn, "global", id_a, id_b, &rel)?;
        assert_eq!(id1, id2);
        Ok(())
    }

    #[test]
    fn test_find_relationship_existente() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let id_a = upsert_entity(&conn, "global", &nova_entidade("fr-a"))?;
        let id_b = upsert_entity(&conn, "global", &nova_entidade("fr-b"))?;

        let rel = NewRelationship {
            source: "fr-a".to_string(),
            target: "fr-b".to_string(),
            relation: "depends_on".to_string(),
            strength: 0.7,
            description: None,
        };
        upsert_relationship(&conn, "global", id_a, id_b, &rel)?;

        let encontrada = find_relationship(&conn, id_a, id_b, "depends_on")?;
        let row = encontrada.ok_or("relação deveria existir")?;
        assert_eq!(row.source_id, id_a);
        assert_eq!(row.target_id, id_b);
        assert!((row.weight - 0.7).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn test_find_relationship_inexistente_retorna_none() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let resultado = find_relationship(&conn, 9999, 8888, "uses")?;
        assert!(resultado.is_none());
        Ok(())
    }

    // ------------------------------------------------------------------ //
    // link_memory_entity / link_memory_relationship
    // ------------------------------------------------------------------ //

    #[test]
    fn test_link_memory_entity_idempotente() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let memory_id = insert_memory(&conn)?;
        let entity_id = upsert_entity(&conn, "global", &nova_entidade("me-ent"))?;

        link_memory_entity(&conn, memory_id, entity_id)?;
        let resultado = link_memory_entity(&conn, memory_id, entity_id);
        assert!(
            resultado.is_ok(),
            "INSERT OR IGNORE não deve falhar em duplicata"
        );
        Ok(())
    }

    #[test]
    fn test_link_memory_relationship_idempotente() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let memory_id = insert_memory(&conn)?;
        let id_a = upsert_entity(&conn, "global", &nova_entidade("mr-a"))?;
        let id_b = upsert_entity(&conn, "global", &nova_entidade("mr-b"))?;

        let rel = NewRelationship {
            source: "mr-a".to_string(),
            target: "mr-b".to_string(),
            relation: "uses".to_string(),
            strength: 0.5,
            description: None,
        };
        let rel_id = upsert_relationship(&conn, "global", id_a, id_b, &rel)?;

        link_memory_relationship(&conn, memory_id, rel_id)?;
        let resultado = link_memory_relationship(&conn, memory_id, rel_id);
        assert!(
            resultado.is_ok(),
            "INSERT OR IGNORE não deve falhar em duplicata"
        );
        Ok(())
    }

    // ------------------------------------------------------------------ //
    // increment_degree / recalculate_degree
    // ------------------------------------------------------------------ //

    #[test]
    fn test_increment_degree_aumenta_contador() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let entity_id = upsert_entity(&conn, "global", &nova_entidade("grau-ent"))?;

        increment_degree(&conn, entity_id)?;
        increment_degree(&conn, entity_id)?;

        let degree: i64 = conn.query_row(
            "SELECT degree FROM entities WHERE id = ?1",
            params![entity_id],
            |r| r.get(0),
        )?;
        assert_eq!(degree, 2);
        Ok(())
    }

    #[test]
    fn test_recalculate_degree_reflete_relacoes_reais() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let id_a = upsert_entity(&conn, "global", &nova_entidade("rc-a"))?;
        let id_b = upsert_entity(&conn, "global", &nova_entidade("rc-b"))?;
        let id_c = upsert_entity(&conn, "global", &nova_entidade("rc-c"))?;

        let rel1 = NewRelationship {
            source: "rc-a".to_string(),
            target: "rc-b".to_string(),
            relation: "uses".to_string(),
            strength: 0.5,
            description: None,
        };
        let rel2 = NewRelationship {
            source: "rc-c".to_string(),
            target: "rc-a".to_string(),
            relation: "depends_on".to_string(),
            strength: 0.5,
            description: None,
        };
        upsert_relationship(&conn, "global", id_a, id_b, &rel1)?;
        upsert_relationship(&conn, "global", id_c, id_a, &rel2)?;

        recalculate_degree(&conn, id_a)?;

        let degree: i64 = conn.query_row(
            "SELECT degree FROM entities WHERE id = ?1",
            params![id_a],
            |r| r.get(0),
        )?;
        assert_eq!(degree, 2, "rc-a aparece em duas relações (source+target)");
        Ok(())
    }

    // ------------------------------------------------------------------ //
    // find_orphan_entity_ids
    // ------------------------------------------------------------------ //

    #[test]
    fn test_find_orphan_entity_ids_sem_orfaos() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let memory_id = insert_memory(&conn)?;
        let entity_id = upsert_entity(&conn, "global", &nova_entidade("nao-orfa"))?;
        link_memory_entity(&conn, memory_id, entity_id)?;

        let orfas = find_orphan_entity_ids(&conn, Some("global"))?;
        assert!(!orfas.contains(&entity_id));
        Ok(())
    }

    #[test]
    fn test_find_orphan_entity_ids_detecta_orfas() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let entity_id = upsert_entity(&conn, "global", &nova_entidade("sim-orfa"))?;

        let orfas = find_orphan_entity_ids(&conn, Some("global"))?;
        assert!(orfas.contains(&entity_id));
        Ok(())
    }

    #[test]
    fn test_find_orphan_entity_ids_sem_namespace_retorna_todas() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let id1 = upsert_entity(&conn, "ns-a", &nova_entidade("orfa-a"))?;
        let id2 = upsert_entity(&conn, "ns-b", &nova_entidade("orfa-b"))?;

        let orfas = find_orphan_entity_ids(&conn, None)?;
        assert!(orfas.contains(&id1));
        assert!(orfas.contains(&id2));
        Ok(())
    }

    // ------------------------------------------------------------------ //
    // list_entities / list_relationships_by_namespace
    // ------------------------------------------------------------------ //

    #[test]
    fn test_list_entities_com_namespace() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        upsert_entity(&conn, "le-ns", &nova_entidade("le-ent-1"))?;
        upsert_entity(&conn, "le-ns", &nova_entidade("le-ent-2"))?;
        upsert_entity(&conn, "outro-ns", &nova_entidade("le-ent-3"))?;

        let lista = list_entities(&conn, Some("le-ns"))?;
        assert_eq!(lista.len(), 2);
        assert!(lista.iter().all(|e| e.namespace == "le-ns"));
        Ok(())
    }

    #[test]
    fn test_list_entities_sem_namespace_retorna_todas() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        upsert_entity(&conn, "ns1", &nova_entidade("all-ent-1"))?;
        upsert_entity(&conn, "ns2", &nova_entidade("all-ent-2"))?;

        let lista = list_entities(&conn, None)?;
        assert!(lista.len() >= 2);
        Ok(())
    }

    #[test]
    fn test_list_relationships_by_namespace_filtra_corretamente() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let id_a = upsert_entity(&conn, "rel-ns", &nova_entidade("lr-a"))?;
        let id_b = upsert_entity(&conn, "rel-ns", &nova_entidade("lr-b"))?;

        let rel = NewRelationship {
            source: "lr-a".to_string(),
            target: "lr-b".to_string(),
            relation: "uses".to_string(),
            strength: 0.5,
            description: None,
        };
        upsert_relationship(&conn, "rel-ns", id_a, id_b, &rel)?;

        let lista = list_relationships_by_namespace(&conn, Some("rel-ns"))?;
        assert!(!lista.is_empty());
        assert!(lista.iter().all(|r| r.namespace == "rel-ns"));
        Ok(())
    }

    // ------------------------------------------------------------------ //
    // delete_relationship_by_id / create_or_fetch_relationship
    // ------------------------------------------------------------------ //

    #[test]
    fn test_delete_relationship_by_id_remove_relacao() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let id_a = upsert_entity(&conn, "global", &nova_entidade("dr-a"))?;
        let id_b = upsert_entity(&conn, "global", &nova_entidade("dr-b"))?;

        let rel = NewRelationship {
            source: "dr-a".to_string(),
            target: "dr-b".to_string(),
            relation: "uses".to_string(),
            strength: 0.5,
            description: None,
        };
        let rel_id = upsert_relationship(&conn, "global", id_a, id_b, &rel)?;

        delete_relationship_by_id(&conn, rel_id)?;

        let encontrada = find_relationship(&conn, id_a, id_b, "uses")?;
        assert!(encontrada.is_none(), "relação deve ter sido removida");
        Ok(())
    }

    #[test]
    fn test_create_or_fetch_relationship_cria_nova() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let id_a = upsert_entity(&conn, "global", &nova_entidade("cf-a"))?;
        let id_b = upsert_entity(&conn, "global", &nova_entidade("cf-b"))?;

        let (rel_id, criada) =
            create_or_fetch_relationship(&conn, "global", id_a, id_b, "uses", 0.5, None)?;
        assert!(rel_id > 0);
        assert!(criada);
        Ok(())
    }

    #[test]
    fn test_create_or_fetch_relationship_retorna_existente() -> Resultado {
        let (_tmp, conn) = setup_db()?;
        let id_a = upsert_entity(&conn, "global", &nova_entidade("cf2-a"))?;
        let id_b = upsert_entity(&conn, "global", &nova_entidade("cf2-b"))?;

        create_or_fetch_relationship(&conn, "global", id_a, id_b, "uses", 0.5, None)?;
        let (_, criada) =
            create_or_fetch_relationship(&conn, "global", id_a, id_b, "uses", 0.5, None)?;
        assert!(!criada, "segunda chamada deve retornar a relação existente");
        Ok(())
    }

    // ------------------------------------------------------------------ //
    // serde alias: campo "type" aceito como sinônimo de "entity_type"
    // ------------------------------------------------------------------ //

    #[test]
    fn aceita_campo_type_como_alias() -> Resultado {
        let json = r#"{"name": "X", "type": "concept"}"#;
        let ent: NewEntity = serde_json::from_str(json)?;
        assert_eq!(ent.entity_type, "concept");
        Ok(())
    }

    #[test]
    fn aceita_campo_entity_type_canonico() -> Resultado {
        let json = r#"{"name": "X", "entity_type": "concept"}"#;
        let ent: NewEntity = serde_json::from_str(json)?;
        assert_eq!(ent.entity_type, "concept");
        Ok(())
    }

    #[test]
    fn ambos_campos_presentes_gera_erro_de_duplicata() {
        // serde trata alias como nome alternativo do mesmo campo;
        // ter entity_type e type no mesmo JSON é duplicata e deve falhar
        let json = r#"{"name": "X", "entity_type": "A", "type": "B"}"#;
        let resultado: Result<NewEntity, _> = serde_json::from_str(json);
        assert!(
            resultado.is_err(),
            "ambos os campos no mesmo JSON é duplicata"
        );
    }
}
