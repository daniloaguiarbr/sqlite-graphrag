//! Handler for the `merge-entities` CLI subcommand (GAP-19).
//!
//! Merges two or more source entities into a single target entity by:
//!   1. Retargeting all relationships pointing at any source to the target.
//!   2. Deduplicating relationships that become identical after the merge
//!      (same source_id + target_id + relation).
//!   3. Retargeting memory_entities bindings.
//!   4. Deleting the now-empty source entity rows.

use crate::errors::AppError;
use crate::i18n::errors_msg;
use crate::output::{self, OutputFormat};
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::entities;
use rusqlite::params;
use serde::Serialize;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Merge two source entities into a target\n  \
    sqlite-graphrag merge-entities --names auth,authentication --into auth-service\n\n  \
    # Merge three sources into one target across a namespace\n  \
    sqlite-graphrag merge-entities --names svc-a,svc-b,old-svc --into canonical-service --namespace my-project\n\n\
NOTE:\n  \
    --names is a comma-separated list of source entity names.\n  \
    --into is the target entity name and must already exist.\n  \
    Source entities are deleted after the merge; the target is preserved.\n  \
    Duplicate relationships (same endpoints + relation) are removed automatically.\n  \
    Run `sqlite-graphrag cleanup-orphans` afterwards if sources had no other links.")]
pub struct MergeEntitiesArgs {
    /// Comma-separated list of source entity names to merge into the target.
    #[arg(long, value_delimiter = ',', value_name = "NAMES")]
    pub names: Vec<String>,
    /// Target entity name. Must already exist. All source relationships are redirected here.
    #[arg(long, value_name = "TARGET")]
    pub into: String,
    #[arg(long)]
    pub namespace: Option<String>,
    #[arg(long, value_enum, default_value = "json")]
    pub format: OutputFormat,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct MergeEntitiesResponse {
    action: String,
    sources: Vec<String>,
    target: String,
    namespace: String,
    relationships_moved: usize,
    entities_removed: usize,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: MergeEntitiesArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();

    if args.names.is_empty() {
        return Err(AppError::Validation(
            "--names must contain at least one source entity name".to_string(),
        ));
    }

    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    let mut conn = open_rw(&paths.db)?;

    // Resolve target entity ID.
    let target_id = entities::find_entity_id(&conn, &namespace, &args.into)?
        .ok_or_else(|| AppError::NotFound(errors_msg::entity_not_found(&args.into, &namespace)))?;

    // Resolve source entity IDs (skip the target if it appears in the list).
    let mut source_ids: Vec<i64> = Vec::with_capacity(args.names.len());
    for name in &args.names {
        if name == &args.into {
            // Source equals target — skip silently to avoid self-referential merge.
            continue;
        }
        let id = entities::find_entity_id(&conn, &namespace, name)?
            .ok_or_else(|| AppError::NotFound(errors_msg::entity_not_found(name, &namespace)))?;
        if !source_ids.contains(&id) {
            source_ids.push(id);
        }
    }

    if source_ids.is_empty() {
        return Err(AppError::Validation(
            "no valid source entities to merge (all names equal the target or were duplicates)"
                .to_string(),
        ));
    }

    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

    let mut relationships_moved: usize = 0;

    for &src_id in &source_ids {
        // Step 1a: redirect source_id, ignoring UNIQUE conflicts.
        let moved_src = tx.execute(
            "UPDATE OR IGNORE relationships SET source_id = ?1 WHERE source_id = ?2",
            params![target_id, src_id],
        )?;
        tx.execute(
            "DELETE FROM relationships WHERE source_id = ?1",
            params![src_id],
        )?;
        // Step 1b: redirect target_id, ignoring UNIQUE conflicts.
        let moved_tgt = tx.execute(
            "UPDATE OR IGNORE relationships SET target_id = ?1 WHERE target_id = ?2",
            params![target_id, src_id],
        )?;
        tx.execute(
            "DELETE FROM relationships WHERE target_id = ?1",
            params![src_id],
        )?;
        relationships_moved += moved_src + moved_tgt;
    }

    // Step 2: remove self-loops introduced by the redirect (target → target).
    tx.execute("DELETE FROM relationships WHERE source_id = target_id", [])?;

    // Step 3: deduplicate relationships that now share (source, target, relation).
    // Safety net — UPDATE OR IGNORE should have handled most duplicates above.
    tx.execute(
        "DELETE FROM relationships
         WHERE id NOT IN (
             SELECT MIN(id)
             FROM relationships
             GROUP BY source_id, target_id, relation
         )",
        [],
    )?;

    // Step 4: retarget memory_entities bindings.
    for &src_id in &source_ids {
        tx.execute(
            "UPDATE memory_entities SET entity_id = ?1 WHERE entity_id = ?2",
            params![target_id, src_id],
        )?;
    }

    // Step 5: deduplicate memory_entities bindings (same memory + entity).
    tx.execute(
        "DELETE FROM memory_entities
         WHERE rowid NOT IN (
             SELECT MIN(rowid)
             FROM memory_entities
             GROUP BY memory_id, entity_id
         )",
        [],
    )?;

    // Step 6: delete source entities (vec_entities first — no FK CASCADE on vec0).
    let mut entities_removed: usize = 0;
    for &src_id in &source_ids {
        let _ = tx.execute(
            "DELETE FROM vec_entities WHERE entity_id = ?1",
            params![src_id],
        );
        let removed = tx.execute("DELETE FROM entities WHERE id = ?1", params![src_id])?;
        entities_removed += removed;
    }

    // Step 7: recalculate degree for target and all adjacent entities.
    let adjacent_ids: Vec<i64> = {
        let mut stmt = tx.prepare(
            "SELECT DISTINCT CASE WHEN source_id = ?1 THEN target_id ELSE source_id END
             FROM relationships WHERE source_id = ?1 OR target_id = ?1",
        )?;
        let ids: Vec<i64> = stmt
            .query_map(params![target_id], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        ids
    };
    entities::recalculate_degree(&tx, target_id)?;
    for &adj_id in &adjacent_ids {
        entities::recalculate_degree(&tx, adj_id)?;
    }

    tx.commit()?;

    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;

    // Build the list of sources that were actually processed (excluding target duplicates).
    let processed_sources: Vec<String> = args
        .names
        .iter()
        .filter(|n| n.as_str() != args.into.as_str())
        .cloned()
        .collect();

    let response = MergeEntitiesResponse {
        action: "merged".to_string(),
        sources: processed_sources,
        target: args.into.clone(),
        namespace: namespace.clone(),
        relationships_moved,
        entities_removed,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    };

    match args.format {
        OutputFormat::Json => output::emit_json(&response)?,
        OutputFormat::Text | OutputFormat::Markdown => {
            output::emit_text(&format!(
                "merged: {} sources into '{}' (relationships_moved={}, entities_removed={}) [{}]",
                response.sources.len(),
                response.target,
                response.relationships_moved,
                response.entities_removed,
                response.namespace
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_entities_response_serializes_all_fields() {
        let resp = MergeEntitiesResponse {
            action: "merged".to_string(),
            sources: vec!["auth".to_string(), "authentication".to_string()],
            target: "auth-service".to_string(),
            namespace: "global".to_string(),
            relationships_moved: 7,
            entities_removed: 2,
            elapsed_ms: 15,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["action"], "merged");
        assert_eq!(json["target"], "auth-service");
        assert_eq!(json["namespace"], "global");
        assert_eq!(json["relationships_moved"], 7);
        assert_eq!(json["entities_removed"], 2);
        let sources = json["sources"].as_array().expect("must be array");
        assert_eq!(sources.len(), 2);
        assert!(json["elapsed_ms"].is_number());
    }

    #[test]
    fn merge_entities_response_action_is_merged() {
        let resp = MergeEntitiesResponse {
            action: "merged".to_string(),
            sources: vec!["src".to_string()],
            target: "tgt".to_string(),
            namespace: "ns".to_string(),
            relationships_moved: 0,
            entities_removed: 1,
            elapsed_ms: 0,
        };
        assert_eq!(resp.action, "merged");
    }

    #[test]
    fn merge_entities_response_empty_sources_serializes() {
        let resp = MergeEntitiesResponse {
            action: "merged".to_string(),
            sources: vec![],
            target: "target".to_string(),
            namespace: "global".to_string(),
            relationships_moved: 0,
            entities_removed: 0,
            elapsed_ms: 1,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        let sources = json["sources"].as_array().expect("must be array");
        assert_eq!(sources.len(), 0);
    }

    #[test]
    fn merge_entities_response_multiple_sources() {
        let resp = MergeEntitiesResponse {
            action: "merged".to_string(),
            sources: vec!["a".into(), "b".into(), "c".into()],
            target: "canonical".to_string(),
            namespace: "proj".to_string(),
            relationships_moved: 12,
            entities_removed: 3,
            elapsed_ms: 42,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["entities_removed"], 3);
        let sources = json["sources"].as_array().unwrap();
        assert_eq!(sources.len(), 3);
    }
}
