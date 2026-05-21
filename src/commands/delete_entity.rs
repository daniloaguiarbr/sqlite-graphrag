//! Handler for the `delete-entity` CLI subcommand (GAP-17).
//!
//! Deletes an entity and, with `--cascade`, all of its relationships and
//! memory bindings. Without `--cascade` the command refuses to proceed, which
//! prevents accidental data loss.

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
    # Delete an entity and all its relationships (cascade required)\n  \
    sqlite-graphrag delete-entity --name auth-module --cascade\n\n  \
    # Delete an entity in a specific namespace\n  \
    sqlite-graphrag delete-entity --name legacy-service --cascade --namespace my-project\n\n  \
    # Without --cascade the command exits with an error:\n  \
    sqlite-graphrag delete-entity --name auth-module\n  \
    # => Error: use --cascade to confirm deletion of entity and all its relationships\n\n\
NOTE:\n  \
    --cascade is required and acts as an explicit confirmation gate.\n  \
    All relationships where this entity is source or target are removed.\n  \
    All memory-entity bindings (memory_entities rows) are also removed.\n  \
    Run `sqlite-graphrag cleanup-orphans` afterwards to remove any newly orphaned entities.")]
pub struct DeleteEntityArgs {
    /// Entity name to delete (graph node, not memory name).
    #[arg(long)]
    pub name: String,
    /// Required confirmation flag. Without it the command exits with an error.
    ///
    /// Deletes all relationships and memory bindings attached to this entity.
    #[arg(long, default_value_t = false)]
    pub cascade: bool,
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
struct DeleteEntityResponse {
    action: String,
    entity_name: String,
    namespace: String,
    relationships_removed: usize,
    bindings_removed: usize,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: DeleteEntityArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();

    if !args.cascade {
        return Err(AppError::Validation(
            "use --cascade to confirm deletion of entity and all its relationships".to_string(),
        ));
    }

    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    let mut conn = open_rw(&paths.db)?;

    let entity_id = entities::find_entity_id(&conn, &namespace, &args.name)?
        .ok_or_else(|| AppError::NotFound(errors_msg::entity_not_found(&args.name, &namespace)))?;

    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

    // Step 0: collect adjacent entity IDs BEFORE deleting relationships.
    let adjacent_ids: Vec<i64> = {
        let mut stmt = tx.prepare(
            "SELECT DISTINCT CASE WHEN source_id = ?1 THEN target_id ELSE source_id END
             FROM relationships WHERE source_id = ?1 OR target_id = ?1",
        )?;
        let ids: Vec<i64> = stmt
            .query_map(params![entity_id], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        ids
    };

    // Step 1: collect relationship IDs for this entity (source or target).
    let rel_ids: Vec<i64> = {
        let mut stmt =
            tx.prepare("SELECT id FROM relationships WHERE source_id = ?1 OR target_id = ?1")?;
        let ids: Vec<i64> = stmt
            .query_map(params![entity_id], |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        ids
    };

    // Step 2: delete memory_relationships for each collected relationship id.
    for &rel_id in &rel_ids {
        tx.execute(
            "DELETE FROM memory_relationships WHERE relationship_id = ?1",
            params![rel_id],
        )?;
    }

    // Step 3: delete the relationships themselves.
    let relationships_removed = tx.execute(
        "DELETE FROM relationships WHERE source_id = ?1 OR target_id = ?1",
        params![entity_id],
    )?;

    // Step 4: delete memory_entities bindings.
    let bindings_removed = tx.execute(
        "DELETE FROM memory_entities WHERE entity_id = ?1",
        params![entity_id],
    )?;

    // Step 5: delete vec_entities row (ignore error — row may not exist).
    let _ = tx.execute(
        "DELETE FROM vec_entities WHERE entity_id = ?1",
        params![entity_id],
    );

    // Step 6: delete the entity itself.
    tx.execute("DELETE FROM entities WHERE id = ?1", params![entity_id])?;

    // Step 7: recalculate degree for adjacent entities that lost relationships.
    for &adj_id in &adjacent_ids {
        if adj_id != entity_id {
            entities::recalculate_degree(&tx, adj_id)?;
        }
    }

    tx.commit()?;

    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;

    let response = DeleteEntityResponse {
        action: "deleted".to_string(),
        entity_name: args.name.clone(),
        namespace: namespace.clone(),
        relationships_removed,
        bindings_removed,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    };

    match args.format {
        OutputFormat::Json => output::emit_json(&response)?,
        OutputFormat::Text | OutputFormat::Markdown => {
            output::emit_text(&format!(
                "deleted: {} (relationships_removed={}, bindings_removed={}) [{}]",
                response.entity_name,
                response.relationships_removed,
                response.bindings_removed,
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
    fn delete_entity_response_serializes_all_fields() {
        let resp = DeleteEntityResponse {
            action: "deleted".to_string(),
            entity_name: "auth-module".to_string(),
            namespace: "global".to_string(),
            relationships_removed: 3,
            bindings_removed: 2,
            elapsed_ms: 7,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["action"], "deleted");
        assert_eq!(json["entity_name"], "auth-module");
        assert_eq!(json["namespace"], "global");
        assert_eq!(json["relationships_removed"], 3);
        assert_eq!(json["bindings_removed"], 2);
        assert!(json["elapsed_ms"].is_number());
    }

    #[test]
    fn delete_entity_response_action_is_deleted() {
        let resp = DeleteEntityResponse {
            action: "deleted".to_string(),
            entity_name: "x".to_string(),
            namespace: "ns".to_string(),
            relationships_removed: 0,
            bindings_removed: 0,
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["action"], "deleted");
    }

    #[test]
    fn delete_entity_response_zero_counts_allowed() {
        let resp = DeleteEntityResponse {
            action: "deleted".to_string(),
            entity_name: "orphan-entity".to_string(),
            namespace: "global".to_string(),
            relationships_removed: 0,
            bindings_removed: 0,
            elapsed_ms: 1,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["relationships_removed"], 0);
        assert_eq!(json["bindings_removed"], 0);
    }
}
