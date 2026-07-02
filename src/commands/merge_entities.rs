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
    sqlite-graphrag merge-entities --names svc-a,svc-b,old-svc --into canonical-service --namespace my-project\n\n  \
    # Merge by ID (unambiguous when homonyms exist across namespaces)\n  \
    sqlite-graphrag merge-entities --ids 12,17 --into-id 3\n\n\
NOTE:\n  \
    --names is a comma-separated list of source entity names.\n  \
    --into is the target entity name and must already exist.\n  \
    --ids / --into-id select entities by ID; IDs are globally unique so they\n  \
    disambiguate homonyms. They conflict with --names / --into respectively\n  \
    and must belong to the resolved namespace.\n  \
    Source entities are deleted after the merge; the target is preserved.\n  \
    Duplicate relationships (same endpoints + relation) are removed automatically.\n  \
    Run `sqlite-graphrag cleanup-orphans` afterwards if sources had no other links.")]
pub struct MergeEntitiesArgs {
    /// Comma-separated list of source entity names to merge into the target.
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "NAMES",
        required_unless_present = "ids",
        conflicts_with = "ids"
    )]
    pub names: Vec<String>,
    /// v1.1.1 (P5): comma-separated list of source entity IDs. IDs are
    /// globally unique, so they disambiguate homonyms across namespaces.
    /// Conflicts with --names; every ID must belong to the resolved namespace.
    #[arg(long, value_delimiter = ',', value_name = "IDS")]
    pub ids: Vec<i64>,
    /// Target entity name. Must already exist. All source relationships are redirected here.
    #[arg(
        long,
        value_name = "TARGET",
        required_unless_present = "into_id",
        conflicts_with = "into_id"
    )]
    pub into: Option<String>,
    /// v1.1.1 (P5): target entity ID. Unambiguous alternative to --into.
    #[arg(long, value_name = "TARGET_ID")]
    pub into_id: Option<i64>,
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
    /// v1.1.1 (P5): resolved target entity ID, echoed for unambiguous auditing.
    target_id: i64,
    relationships_moved: usize,
    entities_removed: usize,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

/// v1.1.1 (P5): resolves an entity ID to its name, enforcing that the entity
/// exists AND belongs to the namespace — IDs are global, so a bare existence
/// check could silently cross namespaces.
fn find_entity_name_by_id(
    conn: &rusqlite::Connection,
    namespace: &str,
    id: i64,
) -> Result<String, AppError> {
    let mut stmt =
        conn.prepare_cached("SELECT name FROM entities WHERE id = ?1 AND namespace = ?2")?;
    match stmt.query_row(params![id, namespace], |r| r.get::<_, String>(0)) {
        Ok(name) => Ok(name),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(AppError::NotFound(format!(
            "entity id={id} not found in namespace '{namespace}'"
        ))),
        Err(e) => Err(AppError::Database(e)),
    }
}

pub fn run(args: MergeEntitiesArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();

    if args.names.is_empty() && args.ids.is_empty() {
        return Err(AppError::Validation(
            "--names or --ids must contain at least one source entity".to_string(),
        ));
    }

    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    let mut conn = open_rw(&paths.db)?;

    // Resolve target entity — by ID (v1.1.1 P5, unambiguous) or by name.
    // Existence is validated here, BEFORE any mutation.
    let (target_id, target_name) = match args.into_id {
        Some(id) => {
            let name = find_entity_name_by_id(&conn, &namespace, id)?;
            (id, name)
        }
        None => {
            let Some(name) = args.into.clone() else {
                return Err(AppError::Validation(
                    "--into or --into-id is required".to_string(),
                ));
            };
            let id = entities::find_entity_id(&conn, &namespace, &name)?.ok_or_else(|| {
                AppError::NotFound(errors_msg::entity_not_found(&name, &namespace))
            })?;
            (id, name)
        }
    };

    // Resolve source entity IDs — reject self-referential merge (G21),
    // by ID (v1.1.1 P5) or by name. All lookups happen BEFORE the transaction.
    let mut source_ids: Vec<i64> = Vec::with_capacity(args.names.len() + args.ids.len());
    let mut source_names: Vec<String> = Vec::with_capacity(source_ids.capacity());
    if !args.ids.is_empty() {
        for &id in &args.ids {
            if id == target_id {
                return Err(AppError::Validation(format!(
                    "source entity id={id} equals target id={target_id} — \
                     self-referential merge is not allowed"
                )));
            }
            let name = find_entity_name_by_id(&conn, &namespace, id)?;
            if !source_ids.contains(&id) {
                source_ids.push(id);
                source_names.push(name);
            }
        }
    } else {
        for name in &args.names {
            if name == &target_name {
                return Err(AppError::Validation(format!(
                    "source entity '{name}' equals target '{target_name}' — \
                     self-referential merge is not allowed"
                )));
            }
            let id = entities::find_entity_id(&conn, &namespace, name)?.ok_or_else(|| {
                AppError::NotFound(errors_msg::entity_not_found(name, &namespace))
            })?;
            if id == target_id {
                return Err(AppError::Validation(format!(
                    "source entity '{name}' resolves to the target (id={target_id}) — \
                     self-referential merge is not allowed"
                )));
            }
            if !source_ids.contains(&id) {
                source_ids.push(id);
                source_names.push(name.clone());
            }
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
    // Use UPDATE OR IGNORE to skip conflicts when memory is already bound to
    // target entity. Then DELETE remaining source rows (the conflicting ones
    // that UPDATE OR IGNORE skipped). Same pattern as relationships (Step 1).
    for &src_id in &source_ids {
        tx.execute(
            "UPDATE OR IGNORE memory_entities SET entity_id = ?1 WHERE entity_id = ?2",
            params![target_id, src_id],
        )?;
        tx.execute(
            "DELETE FROM memory_entities WHERE entity_id = ?1",
            params![src_id],
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

    // Step 6: delete source entities. v1.0.76: FK ON DELETE CASCADE on
    // entity_embeddings handles the vector row automatically.
    let mut entities_removed: usize = 0;
    for &src_id in &source_ids {
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

    let response = MergeEntitiesResponse {
        action: "merged".to_string(),
        sources: source_names,
        target: target_name,
        namespace: namespace.clone(),
        target_id,
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

    // v1.1.1 (P5): ID resolution is namespace-scoped — a homonym in another
    // namespace must NOT be reachable through its ID from the wrong namespace.
    #[test]
    fn find_entity_name_by_id_disambiguates_homonyms_across_namespaces() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE entities (
                id INTEGER PRIMARY KEY,
                namespace TEXT NOT NULL,
                name TEXT NOT NULL,
                UNIQUE(namespace, name)
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO entities (id, namespace, name)
             VALUES (1, 'ns-a', 'auth'), (2, 'ns-b', 'auth')",
            [],
        )
        .unwrap();

        // Same name in two namespaces: each ID resolves only in its own.
        assert_eq!(find_entity_name_by_id(&conn, "ns-a", 1).unwrap(), "auth");
        assert_eq!(find_entity_name_by_id(&conn, "ns-b", 2).unwrap(), "auth");
        let err = find_entity_name_by_id(&conn, "ns-a", 2).unwrap_err();
        assert_eq!(err.exit_code(), 4, "cross-namespace ID must be NotFound");
        assert!(err.to_string().contains("id=2"), "obtido: {err}");
    }

    #[test]
    fn find_entity_name_by_id_missing_id_is_not_found() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE entities (
                id INTEGER PRIMARY KEY,
                namespace TEXT NOT NULL,
                name TEXT NOT NULL
            );",
        )
        .unwrap();
        let err = find_entity_name_by_id(&conn, "global", 99).unwrap_err();
        assert_eq!(err.exit_code(), 4);
    }

    // v1.1.1 (P5): clap-level exclusivity between name-based and ID-based
    // selectors, and requiredness of at least one selector per side.
    #[derive(clap::Parser)]
    struct TestCli {
        #[command(flatten)]
        args: MergeEntitiesArgs,
    }

    #[test]
    fn clap_rejects_names_combined_with_ids() {
        use clap::Parser;
        let err =
            match TestCli::try_parse_from(["t", "--names", "a,b", "--ids", "1,2", "--into", "tgt"])
            {
                Ok(_) => panic!("expected argument conflict"),
                Err(e) => e,
            };
        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn clap_rejects_into_combined_with_into_id() {
        use clap::Parser;
        let err =
            match TestCli::try_parse_from(["t", "--names", "a", "--into", "tgt", "--into-id", "3"])
            {
                Ok(_) => panic!("expected argument conflict"),
                Err(e) => e,
            };
        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn clap_requires_a_source_and_a_target_selector() {
        use clap::Parser;
        assert!(TestCli::try_parse_from(["t", "--into", "tgt"]).is_err());
        assert!(TestCli::try_parse_from(["t", "--names", "a"]).is_err());
        let ok = match TestCli::try_parse_from(["t", "--ids", "1,2", "--into-id", "3"]) {
            Ok(cli) => cli,
            Err(e) => panic!("expected successful parse: {e}"),
        };
        assert_eq!(ok.args.ids, vec![1, 2]);
        assert_eq!(ok.args.into_id, Some(3));
        assert!(ok.args.names.is_empty());
        assert!(ok.args.into.is_none());
    }

    #[test]
    fn merge_entities_response_serializes_all_fields() {
        let resp = MergeEntitiesResponse {
            action: "merged".to_string(),
            sources: vec!["auth".to_string(), "authentication".to_string()],
            target: "auth-service".to_string(),
            namespace: "global".to_string(),
            target_id: 1,
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
            target_id: 1,
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
            target_id: 1,
            relationships_moved: 0,
            entities_removed: 0,
            elapsed_ms: 1,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        let sources = json["sources"].as_array().expect("must be array");
        assert_eq!(sources.len(), 0);
    }

    #[test]
    fn merge_entities_response_with_zero_relationships_moved() {
        let resp = MergeEntitiesResponse {
            action: "merged".to_string(),
            sources: vec!["src-a".to_string()],
            target: "tgt".to_string(),
            namespace: "global".to_string(),
            target_id: 1,
            relationships_moved: 0,
            entities_removed: 1,
            elapsed_ms: 5,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["relationships_moved"], 0);
        assert_eq!(json["entities_removed"], 1);
    }

    #[test]
    fn merge_entities_response_multiple_sources() {
        let resp = MergeEntitiesResponse {
            action: "merged".to_string(),
            sources: vec!["a".into(), "b".into(), "c".into()],
            target: "canonical".to_string(),
            namespace: "proj".to_string(),
            target_id: 1,
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
