//! Persist helpers — write enrichment results to the main DB.

use super::*;

/// Persists entity bindings extracted by the LLM for a memory.
///
/// Creates entities via `upsert_entity`, links them to the memory via
/// `link_memory_entity`, and upserts relationships found between entities.
pub(super) fn persist_memory_bindings(
    conn: &Connection,
    namespace: &str,
    memory_id: i64,
    entities_json: &serde_json::Value,
    rels_json: &serde_json::Value,
) -> Result<(usize, usize), AppError> {
    #[derive(Deserialize)]
    struct EntityItem {
        name: String,
        entity_type: String,
    }
    #[derive(Deserialize)]
    struct RelItem {
        source: String,
        target: String,
        relation: String,
        strength: f64,
    }

    let extracted_entities: Vec<EntityItem> = serde_json::from_value(entities_json.clone())
        .map_err(|e| AppError::Validation(format!("failed to parse entities array: {e}")))?;

    let extracted_rels: Vec<RelItem> = serde_json::from_value(rels_json.clone())
        .map_err(|e| AppError::Validation(format!("failed to parse relationships array: {e}")))?;

    let mut ent_count = 0usize;
    let mut rel_count = 0usize;

    for item in &extracted_entities {
        // GAP-SG-47: fold non-canonical labels onto the nearest canonical kind
        // instead of discarding the entity (no silent data loss).
        let entity_type = EntityType::map_to_canonical(&item.entity_type);
        match entities::upsert_entity(
            conn,
            namespace,
            &NewEntity {
                name: item.name.clone(),
                entity_type,
                description: None,
            },
        ) {
            Ok(eid) => {
                let _ = entities::link_memory_entity(conn, memory_id, eid);
                ent_count += 1;
            }
            Err(e) => {
                tracing::warn!(
                    target: "enrich",
                    entity = %item.name,
                    error = %e,
                    "entity upsert skipped"
                );
            }
        }
    }

    for rel in &extracted_rels {
        // GAP-SG-48: rewrite non-canonical relations to canonical instead of
        // accepting them raw with only a warning.
        let normalized = crate::parsers::map_to_canonical_relation(&rel.relation);

        // Normalize entity names before lookup: upsert_entity normalizes on write,
        // so the lookup must use the same normalized form to find the row.
        let src_name = crate::parsers::normalize_entity_name(&rel.source);
        let tgt_name = crate::parsers::normalize_entity_name(&rel.target);
        let src_id = entities::find_entity_id(conn, namespace, &src_name);
        let tgt_id = entities::find_entity_id(conn, namespace, &tgt_name);
        if let (Ok(Some(sid)), Ok(Some(tid))) = (src_id, tgt_id) {
            let new_rel = NewRelationship {
                source: rel.source.clone(),
                target: rel.target.clone(),
                relation: normalized,
                strength: rel.strength,
                description: None,
            };
            if entities::upsert_relationship(conn, namespace, sid, tid, &new_rel).is_ok() {
                rel_count += 1;
            }
        }
    }

    Ok((ent_count, rel_count))
}

/// Updates an entity's description directly in the `entities` table.
pub(super) fn persist_entity_description(
    conn: &Connection,
    entity_id: i64,
    description: &str,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE entities SET description = ?1, updated_at = unixepoch() WHERE id = ?2",
        rusqlite::params![description, entity_id],
    )?;
    Ok(())
}

/// v1.0.84 (ADR-0042): on successful re-embed, records the active backend
/// into the shared accumulator (`ENRICH_LAST_BACKEND`) so the final
/// `EnrichSummary` can expose `backend_invoked` without changing every
/// caller's signature. Best-effort observability — concurrent enrich runs
/// may race, but `Mutex` keeps the mutation safe.
#[allow(clippy::too_many_arguments)]
pub(super) fn reembed_memory_vector(
    conn: &Connection,
    namespace: &str,
    memory_id: i64,
    memory_name: &str,
    memory_type: &str,
    body: &str,
    paths: &crate::paths::AppPaths,
    llm_backend: crate::cli::LlmBackendChoice,
    embedding_backend: crate::cli::EmbeddingBackendChoice,
) -> Result<(), AppError> {
    let snippet: String = body.chars().take(200).collect();
    // v1.0.82 (GAP-003): forward --llm-backend to embed_with_fallback.
    // v1.0.84 (ADR-0042): tuple (Vec<f32>, LlmBackendKind) — extrai o
    // backend que efetivamente rodou e popula o accumulator para o
    // EnrichSummary agregado.
    // v1.0.93 (GAP-OR-PROPAGATION): honour --embedding-backend openrouter.
    let (embedding, backend_kind) = crate::embedder::embed_passage_with_embedding_choice(
        &paths.models,
        body,
        embedding_backend,
        llm_backend,
    )?;
    record_enrich_backend(backend_kind.as_str());
    memories::upsert_vec(
        conn,
        memory_id,
        namespace,
        memory_type,
        &embedding,
        memory_name,
        &snippet,
    )?;
    Ok(())
}

/// v1.0.84 (ADR-0042): process-local accumulator of the last LLM backend
/// that successfully ran a re-embed during the current enrich invocation.
/// Read by `run` once at summary emission. Scoped to a single process —
/// cross-process enrichment is gated by the per-namespace singleton, so
/// there is no concurrency hazard across DBs.
pub(super) fn record_enrich_backend(backend: &'static str) {
    if let Ok(mut guard) = ENRICH_LAST_BACKEND.lock() {
        *guard = Some(backend);
    }
}

pub(super) fn take_enrich_backend() -> Option<&'static str> {
    ENRICH_LAST_BACKEND.lock().ok().and_then(|mut g| g.take())
}

pub(super) static ENRICH_LAST_BACKEND: std::sync::Mutex<Option<&'static str>> =
    std::sync::Mutex::new(None);

/// Persists an enriched memory body (body-enrich, GAP-18).
///
/// Uses `memories::update` to set the new body and `sync_fts_after_update`
/// to keep FTS5 in sync. Also re-embeds the memory for recall accuracy.
#[allow(clippy::too_many_arguments)]
pub(super) fn persist_enriched_body(
    conn: &Connection,
    namespace: &str,
    memory_id: i64,
    memory_name: &str,
    new_body: &str,
    paths: &crate::paths::AppPaths,
    llm_backend: crate::cli::LlmBackendChoice,
    embedding_backend: crate::cli::EmbeddingBackendChoice,
) -> Result<(), AppError> {
    // Read current values for FTS sync
    let (old_name, old_desc, old_body): (String, String, String) = conn.query_row(
        "SELECT name, COALESCE(description,''), COALESCE(body,'') FROM memories WHERE id=?1",
        rusqlite::params![memory_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;

    let memory_type: String = conn.query_row(
        "SELECT type FROM memories WHERE id=?1",
        rusqlite::params![memory_id],
        |r| r.get(0),
    )?;

    let description: String = conn.query_row(
        "SELECT COALESCE(description,'') FROM memories WHERE id=?1",
        rusqlite::params![memory_id],
        |r| r.get(0),
    )?;

    let body_hash = blake3::hash(new_body.as_bytes()).to_hex().to_string();

    let new_memory = memories::NewMemory {
        namespace: namespace.to_string(),
        name: memory_name.to_string(),
        memory_type: memory_type.clone(),
        description: description.clone(),
        body: new_body.to_string(),
        body_hash,
        session_id: None,
        source: "agent".to_string(),
        metadata: serde_json::json!({
            "operation": "body-enrich",
            "orig_chars": old_body.chars().count(),
            "new_chars": new_body.chars().count(),
        }),
    };

    // G29 audit: insert a new immutable version BEFORE the update so the
    // enriched body is reachable through `history --name <X>` and
    // `restore --version N` can roll back to the pre-enrich state.
    let next_version = crate::storage::versions::next_version(conn, memory_id)?;
    let version_metadata = serde_json::json!({
        "operation": "body-enrich",
        "orig_chars": old_body.chars().count(),
        "new_chars": new_body.chars().count(),
    })
    .to_string();
    crate::storage::versions::insert_version(
        conn,
        memory_id,
        next_version,
        memory_name,
        &memory_type,
        &description,
        new_body,
        &version_metadata,
        Some("enrich"),
        "edit",
    )?;

    memories::update(conn, memory_id, &new_memory, None)?;
    memories::sync_fts_after_update(
        conn,
        memory_id,
        &old_name,
        &old_desc,
        &old_body,
        &new_memory.name,
        &new_memory.description,
        &new_memory.body,
    )?;

    // Re-embed for recall accuracy
    if let Err(e) = reembed_memory_vector(
        conn,
        namespace,
        memory_id,
        memory_name,
        &memory_type,
        new_body,
        paths,
        llm_backend,
        embedding_backend,
    ) {
        tracing::warn!(target: "enrich", memory = %memory_name, error = %e, "vec upsert failed after body-enrich");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn open_test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(
            "CREATE TABLE memories (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                namespace   TEXT NOT NULL DEFAULT 'global',
                name        TEXT NOT NULL,
                type        TEXT NOT NULL DEFAULT 'note',
                description TEXT NOT NULL DEFAULT '',
                body        TEXT NOT NULL DEFAULT '',
                body_hash   TEXT NOT NULL DEFAULT '',
                session_id  TEXT,
                source      TEXT NOT NULL DEFAULT 'agent',
                metadata    TEXT NOT NULL DEFAULT '{}',
                created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
                updated_at  INTEGER NOT NULL DEFAULT (unixepoch()),
                deleted_at  INTEGER,
                UNIQUE(namespace, name)
            );
            CREATE TABLE entities (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                namespace   TEXT NOT NULL DEFAULT 'global',
                name        TEXT NOT NULL,
                type        TEXT NOT NULL DEFAULT 'concept',
                description TEXT,
                degree      INTEGER NOT NULL DEFAULT 0,
                created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
                updated_at  INTEGER NOT NULL DEFAULT (unixepoch()),
                UNIQUE(namespace, name)
            );
            CREATE TABLE memory_entities (
                memory_id  INTEGER NOT NULL,
                entity_id  INTEGER NOT NULL,
                PRIMARY KEY (memory_id, entity_id)
            );
            CREATE TABLE relationships (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                namespace  TEXT NOT NULL DEFAULT 'global',
                source_id  INTEGER NOT NULL,
                target_id  INTEGER NOT NULL,
                relation   TEXT NOT NULL,
                weight     REAL NOT NULL DEFAULT 0.5,
                description TEXT,
                UNIQUE(source_id, target_id, relation)
            );
            CREATE TABLE memory_embeddings (
                memory_id   INTEGER PRIMARY KEY,
                namespace   TEXT NOT NULL,
                embedding   BLOB NOT NULL,
                source      TEXT NOT NULL,
                model       TEXT NOT NULL DEFAULT '',
                dim         INTEGER NOT NULL DEFAULT 384,
                created_at  INTEGER NOT NULL DEFAULT (unixepoch())
            );",
        )
        .expect("schema creation must succeed");
        conn
    }

    #[test]
    fn persist_entity_description_updates_db() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO entities (namespace, name, type) VALUES ('global', 'tokio-runtime', 'tool')",
            [],
        )
        .unwrap();
        let eid: i64 = conn
            .query_row(
                "SELECT id FROM entities WHERE name='tokio-runtime'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        persist_entity_description(&conn, eid, "Async runtime for Rust applications").unwrap();

        let desc: String = conn
            .query_row(
                "SELECT description FROM entities WHERE id=?1",
                rusqlite::params![eid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(desc, "Async runtime for Rust applications");
    }
}
