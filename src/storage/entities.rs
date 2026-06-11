//! Persistence layer for entities, relationships and their junction tables.
//!
//! The entity graph mirrors the conceptual content of memories: `entities`
//! holds nodes, `relationships` holds typed edges and `memory_entities` and
//! `memory_relationships` connect each memory to the graph slice it emitted.

use crate::embedder::f32_to_bytes;
use crate::entity_type::EntityType;
use crate::errors::AppError;
use crate::parsers::normalize_entity_name;
use crate::storage::utils::with_busy_retry;
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
    pub entity_type: EntityType,
    pub description: Option<String>,
}

/// Input payload used to upsert a typed relationship between entities.
///
/// `strength` must lie within `[0.0, 1.0]` and is mapped to the `weight`
/// column of the `relationships` table.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct NewRelationship {
    #[serde(alias = "from")]
    pub source: String,
    #[serde(alias = "to")]
    pub target: String,
    #[serde(alias = "type")]
    pub relation: String,
    #[serde(alias = "weight")]
    pub strength: f64,
    pub description: Option<String>,
}

/// Validates entity name against quality rules.
///
/// Rejects names with newlines, names shorter than 2 characters, and
/// ALL_CAPS abbreviations of 4 characters or fewer (common NER noise).
///
/// # Errors
///
/// Returns `Err(AppError::Validation)` when the name violates any rule.
pub fn validate_entity_name(name: &str) -> Result<(), AppError> {
    if name.len() < 2 {
        return Err(AppError::Validation(format!(
            "entity name '{name}' must be at least 2 characters"
        )));
    }
    if name.contains('\n') || name.contains('\r') {
        return Err(AppError::Validation(
            "entity name must not contain newline characters".to_string(),
        ));
    }
    if name.len() <= 4
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c == '_' || c == '-')
    {
        return Err(AppError::Validation(format!(
            "entity name '{name}' rejected: short ALL_CAPS names are typically NER noise"
        )));
    }
    Ok(())
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
    // Step 1: validate the original name — catches ALL_CAPS short noise (NER artefacts),
    // newlines, and names shorter than 2 characters before any transformation.
    validate_entity_name(&e.name)?;
    // Step 2: normalize to kebab-case ASCII (NFKD, lowercase, spaces/underscores → hyphens).
    let normalized_name = normalize_entity_name(&e.name);
    // Step 3: guard post-normalization length — a valid original could collapse to < 2 chars
    // (e.g. a single accented character that strips entirely).
    if normalized_name.chars().count() < 2 {
        return Err(AppError::Validation(format!(
            "entity name '{}' normalizes to '{}' which is too short (minimum 2 characters)",
            e.name, normalized_name
        )));
    }
    conn.execute(
        "INSERT INTO entities (namespace, name, type, description)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(namespace, name) DO UPDATE SET
           type        = excluded.type,
           description = COALESCE(excluded.description, entities.description),
           updated_at  = unixepoch()",
        params![namespace, normalized_name, e.entity_type, e.description],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM entities WHERE namespace = ?1 AND name = ?2",
        params![namespace, normalized_name],
        |r| r.get(0),
    )?;
    Ok(id)
}

/// Replaces the vector row for an entity in `entity_embeddings`.
///
/// v1.0.76: sqlite-vec was removed. Embeddings live in a regular BLOB-backed
/// table; cosine similarity is computed in pure Rust on demand. The
/// `entity_type` and `name` arguments are accepted for API compatibility
/// but are not stored — the entities table is the source of truth.
///
/// # Errors
///
/// Returns `Err(AppError::Database)` on any `rusqlite` failure.
pub fn upsert_entity_vec(
    conn: &Connection,
    entity_id: i64,
    namespace: &str,
    _entity_type: EntityType,
    embedding: &[f32],
    _name: &str,
) -> Result<(), AppError> {
    let embedding_bytes = f32_to_bytes(embedding);
    with_busy_retry(|| {
        conn.execute(
            "DELETE FROM entity_embeddings WHERE entity_id = ?1",
            params![entity_id],
        )?;
        conn.execute(
            "INSERT INTO entity_embeddings(entity_id, namespace, embedding, source, model, dim)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                entity_id,
                namespace,
                &embedding_bytes,
                "llm-headless",
                crate::constants::SQLITE_GRAPHRAG_VERSION,
                crate::constants::embedding_dim() as i64,
            ],
        )?;
        Ok(())
    })
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

/// Links a memory to an entity in the `memory_entities` join table.
///
/// # Errors
///
/// Returns [`AppError::Database`] when the underlying SQLite operation fails.
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

/// Links a memory to a relationship in the `memory_relationships` join table.
///
/// # Errors
///
/// Returns [`AppError::Database`] when the underlying SQLite operation fails.
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

/// Increments the `degree` counter of an entity by one.
///
/// # Errors
///
/// Returns [`AppError::Database`] when the underlying SQLite operation fails.
pub fn increment_degree(conn: &Connection, entity_id: i64) -> Result<(), AppError> {
    conn.execute(
        "UPDATE entities SET degree = degree + 1 WHERE id = ?1",
        params![entity_id],
    )?;
    Ok(())
}

/// Looks up the entity by name and namespace. Returns the id when it exists.
///
/// # Errors
///
/// Returns [`AppError::Database`] when the underlying SQLite operation fails.
pub fn find_entity_id(
    conn: &Connection,
    namespace: &str,
    name: &str,
) -> Result<Option<i64>, AppError> {
    // Normalize the lookup name so it matches the normalized names written by
    // `upsert_entity`. Without this, an entity written through normalization
    // (e.g. "Foo Bar" -> "foo-bar") would be unreachable by its original
    // spelling, breaking delete-entity, reclassify, merge-entities, rename and
    // memory-entities lookups.
    let name = normalize_entity_name(name);
    let mut stmt =
        conn.prepare_cached("SELECT id FROM entities WHERE namespace = ?1 AND name = ?2")?;
    match stmt.query_row(params![namespace, &name], |r| r.get::<_, i64>(0)) {
        Ok(id) => Ok(Some(id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e)),
    }
}

/// Structure representing an existing relation.
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

/// Looks up a specific relation by (source_id, target_id, relation).
///
/// # Errors
///
/// Returns [`AppError::Database`] when the underlying SQLite operation fails.
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

/// Creates a relation if it does not exist (returns action="created")
/// or returns the existing relation (action="already_exists") with updated weight.
///
/// # Errors
///
/// - [`AppError::Database`] — SQLite query or constraint failure.
/// - [`AppError::Validation`] — self-link attempt (source equals target).
pub fn create_or_fetch_relationship(
    conn: &Connection,
    namespace: &str,
    source_id: i64,
    target_id: i64,
    relation: &str,
    weight: f64,
    description: Option<&str>,
) -> Result<(i64, bool), AppError> {
    // Check if it exists first; update weight if different.
    let existing = find_relationship(conn, source_id, target_id, relation)?;
    if let Some(row) = existing {
        if (row.weight - weight).abs() > f64::EPSILON {
            conn.execute(
                "UPDATE relationships SET weight = ?1 WHERE id = ?2",
                params![weight, row.id],
            )?;
        }
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

/// Removes a relation by id and cleans up memory_relationships.
///
/// # Errors
///
/// Returns [`AppError::Database`] when the underlying SQLite operation fails.
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

/// Recalculates the `degree` field of an entity.
///
/// # Errors
///
/// Returns [`AppError::Database`] when the underlying SQLite operation fails.
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

/// Entity row with enough data for graph export/query.
#[derive(Debug, Serialize, Clone)]
pub struct EntityNode {
    pub id: i64,
    pub name: String,
    pub namespace: String,
    pub kind: String,
}

/// Lists entities, filtering by namespace if provided.
///
/// # Errors
///
/// Returns [`AppError::Database`] when the underlying SQLite operation fails.
pub fn list_entities(
    conn: &Connection,
    namespace: Option<&str>,
) -> Result<Vec<EntityNode>, AppError> {
    if let Some(ns) = namespace {
        let mut stmt = conn.prepare_cached(
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
        let mut stmt = conn.prepare_cached(
            "SELECT id, name, namespace, type FROM entities ORDER BY namespace, id",
        )?;
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

/// Lists relations filtered by namespace (of source/target entities).
///
/// # Errors
///
/// Returns [`AppError::Database`] when the underlying SQLite operation fails.
pub fn list_relationships_by_namespace(
    conn: &Connection,
    namespace: Option<&str>,
) -> Result<Vec<RelationshipRow>, AppError> {
    if let Some(ns) = namespace {
        let mut stmt = conn.prepare_cached(
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
        let mut stmt = conn.prepare_cached(
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

/// Locates orphan entities: no link in memory_entities and no relations.
///
/// # Errors
///
/// Returns [`AppError::Database`] when the underlying SQLite operation fails.
pub fn find_orphan_entity_ids(
    conn: &Connection,
    namespace: Option<&str>,
) -> Result<Vec<i64>, AppError> {
    if let Some(ns) = namespace {
        let mut stmt = conn.prepare_cached(
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
        let mut stmt = conn.prepare_cached(
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

/// Deletes entities and their associated vectors. Returns the number of entities removed.
///
/// # Errors
///
/// Returns [`AppError::Database`] when the underlying SQLite operation fails.
pub fn delete_entities_by_ids(conn: &Connection, entity_ids: &[i64]) -> Result<usize, AppError> {
    if entity_ids.is_empty() {
        return Ok(0);
    }
    let mut removed = 0usize;
    for id in entity_ids {
        // FK CASCADE on entity_embeddings handles cleanup automatically.
        let _ = conn.execute("DELETE FROM vec_entities WHERE entity_id = ?1", params![id]);
        let affected = conn.execute("DELETE FROM entities WHERE id = ?1", params![id])?;
        removed += affected;
    }
    Ok(removed)
}

/// Counts relationships matching the given relation type within a namespace.
///
/// Used by `prune-relations --dry-run` to preview the number of relationships
/// that would be deleted without actually modifying the database.
///
/// # Errors
///
/// Returns `Err(AppError::Database)` on any `rusqlite` failure.
pub fn count_relationships_by_relation(
    conn: &Connection,
    namespace: &str,
    relation: &str,
) -> Result<usize, AppError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM relationships WHERE namespace = ?1 AND relation = ?2",
        params![namespace, relation],
        |r| r.get(0),
    )?;
    Ok(count as usize)
}

/// Returns unique entity names involved in relationships of the given type.
///
/// Queries both source and target sides of every matching relationship row,
/// deduplicates via `DISTINCT`, and returns the names in alphabetical order.
///
/// # Errors
///
/// Returns `Err(AppError::Database)` on any `rusqlite` failure.
pub fn list_entity_names_by_relation(
    conn: &Connection,
    namespace: &str,
    relation: &str,
) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT DISTINCT e.name FROM entities e
         INNER JOIN relationships r ON (e.id = r.source_id OR e.id = r.target_id)
         WHERE r.namespace = ?1 AND r.relation = ?2
         ORDER BY e.name",
    )?;
    let names: Vec<String> = stmt
        .query_map(params![namespace, relation], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(names)
}

/// Deletes all relationships matching a relation type within a namespace.
///
/// Operates in chunks of 1000 to avoid holding long write locks and blocking
/// WAL readers. After deletion, recalculates degree for every affected entity.
///
/// Returns `(count_deleted, affected_entity_ids)`.
///
/// # Errors
///
/// Returns `Err(AppError::Database)` on any `rusqlite` failure.
pub fn delete_relationships_by_relation(
    conn: &Connection,
    namespace: &str,
    relation: &str,
) -> Result<(usize, Vec<i64>), AppError> {
    // Step 1: collect all affected entity IDs before deletion.
    let mut stmt = conn.prepare_cached(
        "SELECT DISTINCT source_id FROM relationships WHERE namespace = ?1 AND relation = ?2
         UNION
         SELECT DISTINCT target_id FROM relationships WHERE namespace = ?1 AND relation = ?2",
    )?;
    let entity_ids: Vec<i64> = stmt
        .query_map(params![namespace, relation], |r| r.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;

    // Step 2: collect relationship IDs to delete.
    let mut id_stmt =
        conn.prepare_cached("SELECT id FROM relationships WHERE namespace = ?1 AND relation = ?2")?;
    let rel_ids: Vec<i64> = id_stmt
        .query_map(params![namespace, relation], |r| r.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;

    // Step 3: delete in chunks of 1000 (memory_relationships + relationships).
    let mut total_deleted: usize = 0;
    for chunk in rel_ids.chunks(1000) {
        for &rel_id in chunk {
            conn.execute(
                "DELETE FROM memory_relationships WHERE relationship_id = ?1",
                params![rel_id],
            )?;
            let affected =
                conn.execute("DELETE FROM relationships WHERE id = ?1", params![rel_id])?;
            total_deleted += affected;
        }
    }

    // Step 4: recalculate degree for all affected entities.
    for &eid in &entity_ids {
        recalculate_degree(conn, eid)?;
    }

    Ok((total_deleted, entity_ids))
}

/// Searches the `entity_embeddings` table for the k nearest neighbours
/// using pure-Rust cosine similarity.
///
/// v1.0.76: sqlite-vec was removed. The full table scan + in-process
/// cosine is O(N × D) per call. For namespaces with more than ~10k
/// entities, the operator should rely on FTS5 (`hybrid-search`) for
/// coarse filtering before reaching this function.
///
/// # Errors
///
/// - [`AppError::Database`] — SQLite query failure.
/// - [`AppError::Embedding`] — invalid or mismatched embedding dimension.
pub fn knn_search(
    conn: &Connection,
    embedding: &[f32],
    namespace: &str,
    k: usize,
) -> Result<Vec<(i64, f32)>, AppError> {
    if embedding.len() != crate::constants::embedding_dim() {
        return Err(AppError::Embedding(format!(
            "knn_search embedding has {} dims, expected {}",
            embedding.len(),
            crate::constants::embedding_dim()
        )));
    }
    let mut stmt = conn.prepare_cached(
        "SELECT entity_id, embedding FROM entity_embeddings WHERE namespace = ?1",
    )?;
    let mut scored: Vec<(i64, f32)> = stmt
        .query_map(params![namespace], |r| {
            let id: i64 = r.get(0)?;
            let bytes: Vec<u8> = r.get(1)?;
            Ok((id, bytes))
        })?
        .filter_map(|row| {
            row.ok().and_then(|(id, bytes)| {
                let stored = crate::embedder::bytes_to_f32(&bytes);
                if stored.len() != embedding.len() {
                    return None;
                }
                let score = crate::similarity::cosine_similarity(embedding, &stored);
                Some((id, score))
            })
        })
        .collect();
    // `cosine_similarity` returns a value in [-1.0, 1.0]; 1.0 is the
    // best match. Sort descending and truncate to `k`.
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(k);
    Ok(scored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::embedding_dim;
    use crate::entity_type::EntityType;
    use crate::storage::connection::register_vec_extension;
    use rusqlite::Connection;
    use tempfile::TempDir;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

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

    fn new_entity_helper(name: &str) -> NewEntity {
        NewEntity {
            name: name.to_string(),
            entity_type: EntityType::Project,
            description: None,
        }
    }

    fn embedding_zero() -> Vec<f32> {
        vec![0.0f32; embedding_dim()]
    }

    // ------------------------------------------------------------------ //
    // upsert_entity
    // ------------------------------------------------------------------ //

    #[test]
    fn test_upsert_entity_creates_new() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let e = new_entity_helper("projeto-alpha");
        let id = upsert_entity(&conn, "global", &e)?;
        assert!(id > 0);
        Ok(())
    }

    #[test]
    fn test_upsert_entity_idempotent_returns_same_id() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let e = new_entity_helper("projeto-beta");
        let id1 = upsert_entity(&conn, "global", &e)?;
        let id2 = upsert_entity(&conn, "global", &e)?;
        assert_eq!(id1, id2);
        Ok(())
    }

    #[test]
    fn test_upsert_entity_updates_description() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let e1 = new_entity_helper("projeto-gamma");
        let id1 = upsert_entity(&conn, "global", &e1)?;

        let e2 = NewEntity {
            name: "projeto-gamma".to_string(),
            entity_type: EntityType::Tool,
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
    fn test_upsert_entity_different_namespaces_create_distinct_records() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let e = new_entity_helper("compartilhada");
        let id1 = upsert_entity(&conn, "ns1", &e)?;
        let id2 = upsert_entity(&conn, "ns2", &e)?;
        assert_ne!(id1, id2);
        Ok(())
    }

    // ------------------------------------------------------------------ //
    // upsert_entity_vec — covers DELETE+INSERT (new branch after the OOM fix)
    // ------------------------------------------------------------------ //

    #[test]
    fn test_upsert_entity_vec_first_time_without_conflict() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let e = new_entity_helper("vec-nova");
        let entity_id = upsert_entity(&conn, "global", &e)?;
        let emb = embedding_zero();

        let result = upsert_entity_vec(
            &conn,
            entity_id,
            "global",
            EntityType::Project,
            &emb,
            "vec-nova",
        );
        assert!(result.is_ok(), "first insertion must succeed");

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM entity_embeddings WHERE entity_id = ?1",
            params![entity_id],
            |r| r.get(0),
        )?;
        assert_eq!(count, 1, "must have exactly one row after insertion");
        Ok(())
    }

    #[test]
    fn test_upsert_entity_vec_second_time_replaces_without_error() -> TestResult {
        // Covers the branch where DELETE removes the existing row before INSERT.
        let (_tmp, conn) = setup_db()?;
        let e = new_entity_helper("vec-existente");
        let entity_id = upsert_entity(&conn, "global", &e)?;
        let emb = embedding_zero();

        upsert_entity_vec(
            &conn,
            entity_id,
            "global",
            EntityType::Project,
            &emb,
            "vec-existente",
        )?;

        // Second call: DELETE returns 1 removed row, INSERT must succeed.
        let result = upsert_entity_vec(
            &conn,
            entity_id,
            "global",
            EntityType::Tool,
            &emb,
            "vec-existente",
        );
        assert!(
            result.is_ok(),
            "second insertion (replace) must succeed: {result:?}"
        );

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM entity_embeddings WHERE entity_id = ?1",
            params![entity_id],
            |r| r.get(0),
        )?;
        assert_eq!(count, 1, "must have exactly one row after replacement");
        Ok(())
    }

    #[test]
    fn test_upsert_entity_vec_multiple_independent_entities() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let emb = embedding_zero();

        for i in 0..3i64 {
            let nome = format!("ent-{i}");
            let e = new_entity_helper(&nome);
            let entity_id = upsert_entity(&conn, "global", &e)?;
            upsert_entity_vec(&conn, entity_id, "global", EntityType::Project, &emb, &nome)?;
        }

        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM entity_embeddings", [], |r| r.get(0))?;
        assert_eq!(
            count, 3,
            "must have three distinct rows in entity_embeddings"
        );
        Ok(())
    }

    // ------------------------------------------------------------------ //
    // find_entity_id
    // ------------------------------------------------------------------ //

    #[test]
    fn test_find_entity_id_existing_returns_some() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let e = new_entity_helper("entidade-busca");
        let id_inserido = upsert_entity(&conn, "global", &e)?;
        let id_encontrado = find_entity_id(&conn, "global", "entidade-busca")?;
        assert_eq!(id_encontrado, Some(id_inserido));
        Ok(())
    }

    #[test]
    fn test_find_entity_id_missing_returns_none() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let id = find_entity_id(&conn, "global", "nao-existe")?;
        assert_eq!(id, None);
        Ok(())
    }

    // ------------------------------------------------------------------ //
    // delete_entities_by_ids
    // ------------------------------------------------------------------ //

    #[test]
    fn test_delete_entities_by_ids_empty_list_returns_zero() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let removed = delete_entities_by_ids(&conn, &[])?;
        assert_eq!(removed, 0);
        Ok(())
    }

    #[test]
    fn test_delete_entities_by_ids_removes_valid_entity() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let e = new_entity_helper("to-delete");
        let entity_id = upsert_entity(&conn, "global", &e)?;

        let removed = delete_entities_by_ids(&conn, &[entity_id])?;
        assert_eq!(removed, 1);

        let id = find_entity_id(&conn, "global", "to-delete")?;
        assert_eq!(id, None, "entity must have been removed");
        Ok(())
    }

    #[test]
    fn test_delete_entities_by_ids_missing_id_returns_zero() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let removed = delete_entities_by_ids(&conn, &[9999])?;
        assert_eq!(removed, 0);
        Ok(())
    }

    #[test]
    fn test_delete_entities_by_ids_removes_multiple() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let id1 = upsert_entity(&conn, "global", &new_entity_helper("del-a"))?;
        let id2 = upsert_entity(&conn, "global", &new_entity_helper("del-b"))?;
        let id3 = upsert_entity(&conn, "global", &new_entity_helper("del-c"))?;

        let removed = delete_entities_by_ids(&conn, &[id1, id2])?;
        assert_eq!(removed, 2);

        assert!(find_entity_id(&conn, "global", "del-a")?.is_none());
        assert!(find_entity_id(&conn, "global", "del-b")?.is_none());
        assert!(find_entity_id(&conn, "global", "del-c")?.is_some());
        let _ = id3;
        Ok(())
    }

    #[test]
    fn test_delete_entities_by_ids_also_removes_vec() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let e = new_entity_helper("del-com-vec");
        let entity_id = upsert_entity(&conn, "global", &e)?;
        let emb = embedding_zero();
        upsert_entity_vec(
            &conn,
            entity_id,
            "global",
            EntityType::Project,
            &emb,
            "del-com-vec",
        )?;

        let count_antes: i64 = conn.query_row(
            "SELECT COUNT(*) FROM entity_embeddings WHERE entity_id = ?1",
            params![entity_id],
            |r| r.get(0),
        )?;
        assert_eq!(count_antes, 1);

        delete_entities_by_ids(&conn, &[entity_id])?;

        let count_depois: i64 = conn.query_row(
            "SELECT COUNT(*) FROM entity_embeddings WHERE entity_id = ?1",
            params![entity_id],
            |r| r.get(0),
        )?;
        assert_eq!(
            count_depois, 0,
            "entity_embeddings deve ser limpo junto com entities"
        );
        Ok(())
    }

    // ------------------------------------------------------------------ //
    // upsert_relationship / find_relationship
    // ------------------------------------------------------------------ //

    #[test]
    fn test_upsert_relationship_creates_new() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let id_a = upsert_entity(&conn, "global", &new_entity_helper("rel-a"))?;
        let id_b = upsert_entity(&conn, "global", &new_entity_helper("rel-b"))?;

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
    fn test_upsert_relationship_idempotent() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let id_a = upsert_entity(&conn, "global", &new_entity_helper("idem-a"))?;
        let id_b = upsert_entity(&conn, "global", &new_entity_helper("idem-b"))?;

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
    fn test_find_relationship_existing() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let id_a = upsert_entity(&conn, "global", &new_entity_helper("fr-a"))?;
        let id_b = upsert_entity(&conn, "global", &new_entity_helper("fr-b"))?;

        let rel = NewRelationship {
            source: "fr-a".to_string(),
            target: "fr-b".to_string(),
            relation: "depends_on".to_string(),
            strength: 0.7,
            description: None,
        };
        upsert_relationship(&conn, "global", id_a, id_b, &rel)?;

        let encontrada = find_relationship(&conn, id_a, id_b, "depends_on")?;
        let row = encontrada.ok_or("relationship should exist")?;
        assert_eq!(row.source_id, id_a);
        assert_eq!(row.target_id, id_b);
        assert!((row.weight - 0.7).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn test_find_relationship_missing_returns_none() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let resultado = find_relationship(&conn, 9999, 8888, "uses")?;
        assert!(resultado.is_none());
        Ok(())
    }

    // ------------------------------------------------------------------ //
    // link_memory_entity / link_memory_relationship
    // ------------------------------------------------------------------ //

    #[test]
    fn test_link_memory_entity_idempotent() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let memory_id = insert_memory(&conn)?;
        let entity_id = upsert_entity(&conn, "global", &new_entity_helper("me-ent"))?;

        link_memory_entity(&conn, memory_id, entity_id)?;
        let resultado = link_memory_entity(&conn, memory_id, entity_id);
        assert!(
            resultado.is_ok(),
            "INSERT OR IGNORE must not fail on duplicate"
        );
        Ok(())
    }

    #[test]
    fn test_link_memory_relationship_idempotent() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let memory_id = insert_memory(&conn)?;
        let id_a = upsert_entity(&conn, "global", &new_entity_helper("mr-a"))?;
        let id_b = upsert_entity(&conn, "global", &new_entity_helper("mr-b"))?;

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
            "INSERT OR IGNORE must not fail on duplicate"
        );
        Ok(())
    }

    // ------------------------------------------------------------------ //
    // increment_degree / recalculate_degree
    // ------------------------------------------------------------------ //

    #[test]
    fn test_increment_degree_increases_counter() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let entity_id = upsert_entity(&conn, "global", &new_entity_helper("grau-ent"))?;

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
    fn test_recalculate_degree_reflects_actual_relations() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let id_a = upsert_entity(&conn, "global", &new_entity_helper("rc-a"))?;
        let id_b = upsert_entity(&conn, "global", &new_entity_helper("rc-b"))?;
        let id_c = upsert_entity(&conn, "global", &new_entity_helper("rc-c"))?;

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
        assert_eq!(
            degree, 2,
            "rc-a appears in two relationships (source+target)"
        );
        Ok(())
    }

    // ------------------------------------------------------------------ //
    // find_orphan_entity_ids
    // ------------------------------------------------------------------ //

    #[test]
    fn test_find_orphan_entity_ids_without_orphans() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let memory_id = insert_memory(&conn)?;
        let entity_id = upsert_entity(&conn, "global", &new_entity_helper("nao-orfa"))?;
        link_memory_entity(&conn, memory_id, entity_id)?;

        let orfas = find_orphan_entity_ids(&conn, Some("global"))?;
        assert!(!orfas.contains(&entity_id));
        Ok(())
    }

    #[test]
    fn test_find_orphan_entity_ids_detects_orphans() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let entity_id = upsert_entity(&conn, "global", &new_entity_helper("sim-orfa"))?;

        let orfas = find_orphan_entity_ids(&conn, Some("global"))?;
        assert!(orfas.contains(&entity_id));
        Ok(())
    }

    #[test]
    fn test_find_orphan_entity_ids_without_namespace_returns_all() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let id1 = upsert_entity(&conn, "ns-a", &new_entity_helper("orfa-a"))?;
        let id2 = upsert_entity(&conn, "ns-b", &new_entity_helper("orfa-b"))?;

        let orfas = find_orphan_entity_ids(&conn, None)?;
        assert!(orfas.contains(&id1));
        assert!(orfas.contains(&id2));
        Ok(())
    }

    // ------------------------------------------------------------------ //
    // list_entities / list_relationships_by_namespace
    // ------------------------------------------------------------------ //

    #[test]
    fn test_list_entities_with_namespace() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        upsert_entity(&conn, "le-ns", &new_entity_helper("le-ent-1"))?;
        upsert_entity(&conn, "le-ns", &new_entity_helper("le-ent-2"))?;
        upsert_entity(&conn, "outro-ns", &new_entity_helper("le-ent-3"))?;

        let lista = list_entities(&conn, Some("le-ns"))?;
        assert_eq!(lista.len(), 2);
        assert!(lista.iter().all(|e| e.namespace == "le-ns"));
        Ok(())
    }

    #[test]
    fn test_list_entities_without_namespace_returns_all() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        upsert_entity(&conn, "ns1", &new_entity_helper("all-ent-1"))?;
        upsert_entity(&conn, "ns2", &new_entity_helper("all-ent-2"))?;

        let lista = list_entities(&conn, None)?;
        assert!(lista.len() >= 2);
        Ok(())
    }

    #[test]
    fn test_list_relationships_by_namespace_filters_correctly() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let id_a = upsert_entity(&conn, "rel-ns", &new_entity_helper("lr-a"))?;
        let id_b = upsert_entity(&conn, "rel-ns", &new_entity_helper("lr-b"))?;

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
    fn test_delete_relationship_by_id_removes_relation() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let id_a = upsert_entity(&conn, "global", &new_entity_helper("dr-a"))?;
        let id_b = upsert_entity(&conn, "global", &new_entity_helper("dr-b"))?;

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
        assert!(encontrada.is_none(), "relationship must have been removed");
        Ok(())
    }

    #[test]
    fn test_create_or_fetch_relationship_creates_new() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let id_a = upsert_entity(&conn, "global", &new_entity_helper("cf-a"))?;
        let id_b = upsert_entity(&conn, "global", &new_entity_helper("cf-b"))?;

        let (rel_id, created) =
            create_or_fetch_relationship(&conn, "global", id_a, id_b, "uses", 0.5, None)?;
        assert!(rel_id > 0);
        assert!(created);
        Ok(())
    }

    #[test]
    fn test_create_or_fetch_relationship_returns_existing() -> TestResult {
        let (_tmp, conn) = setup_db()?;
        let id_a = upsert_entity(&conn, "global", &new_entity_helper("cf2-a"))?;
        let id_b = upsert_entity(&conn, "global", &new_entity_helper("cf2-b"))?;

        create_or_fetch_relationship(&conn, "global", id_a, id_b, "uses", 0.5, None)?;
        let (_, created) =
            create_or_fetch_relationship(&conn, "global", id_a, id_b, "uses", 0.5, None)?;
        assert!(
            !created,
            "second call must return the existing relationship"
        );
        Ok(())
    }

    // ------------------------------------------------------------------ //
    // serde alias: field "type" accepted as a synonym for "entity_type"
    // ------------------------------------------------------------------ //

    #[test]
    fn accepts_type_field_as_alias() -> TestResult {
        let json = r#"{"name": "X", "type": "concept"}"#;
        let ent: NewEntity = serde_json::from_str(json)?;
        assert_eq!(ent.entity_type, EntityType::Concept);
        Ok(())
    }

    #[test]
    fn accepts_canonical_entity_type_field() -> TestResult {
        let json = r#"{"name": "X", "entity_type": "concept"}"#;
        let ent: NewEntity = serde_json::from_str(json)?;
        assert_eq!(ent.entity_type, EntityType::Concept);
        Ok(())
    }

    #[test]
    fn both_fields_present_yields_duplicate_error() {
        // having both entity_type and type in the same JSON is a duplicate and must fail
        let json = r#"{"name": "X", "entity_type": "concept", "type": "person"}"#;
        let resultado: Result<NewEntity, _> = serde_json::from_str(json);
        assert!(
            resultado.is_err(),
            "both fields in the same JSON are a duplicate"
        );
    }

    #[test]
    fn validate_entity_name_accepts_valid() {
        assert!(validate_entity_name("rust-lang").is_ok());
        assert!(validate_entity_name("sqlite-graphrag").is_ok());
        assert!(validate_entity_name("ab").is_ok());
    }

    #[test]
    fn validate_entity_name_rejects_short() {
        assert!(validate_entity_name("a").is_err());
        assert!(validate_entity_name("").is_err());
    }

    #[test]
    fn validate_entity_name_rejects_newlines() {
        assert!(validate_entity_name("foo\nbar").is_err());
        assert!(validate_entity_name("foo\rbar").is_err());
    }

    #[test]
    fn validate_entity_name_rejects_short_allcaps() {
        assert!(validate_entity_name("RAM").is_err());
        assert!(validate_entity_name("NAO").is_err());
        assert!(validate_entity_name("OK").is_err());
    }

    #[test]
    fn validate_entity_name_accepts_long_allcaps() {
        assert!(validate_entity_name("SQLITE").is_ok());
        assert!(validate_entity_name("GRAPHRAG").is_ok());
    }

    #[test]
    fn validate_entity_name_accepts_mixed_case() {
        assert!(validate_entity_name("FTS5").is_ok()); // 4 chars but has digit
        assert!(validate_entity_name("WAL").is_err()); // 3 chars ALL_CAPS
    }
}
