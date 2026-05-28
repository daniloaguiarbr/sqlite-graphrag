//! Handler for the `prune-ner` CLI subcommand.
//!
//! Removes NER bindings (rows in `memory_entities`) for a single entity or for
//! all entities in the namespace. Useful for cleaning up low-quality automatic
//! extractions without touching the entities or memories themselves.

use crate::errors::AppError;
use crate::output::{self, OutputFormat};
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use serde::Serialize;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Preview bindings that would be removed for a single entity\n  \
    sqlite-graphrag prune-ner --entity jwt-token --dry-run\n\n  \
    # Remove all NER bindings for a single entity\n  \
    sqlite-graphrag prune-ner --entity jwt-token --yes\n\n  \
    # Remove ALL NER bindings in the current namespace\n  \
    sqlite-graphrag prune-ner --all --yes\n\n  \
NOTE:\n  \
    This command deletes rows from memory_entities (the link table between\n  \
    memories and extracted entities). The entities and memories themselves\n  \
    are not deleted. Use cleanup-orphans afterwards to remove entity nodes\n  \
    that have no remaining links.")]
pub struct PruneNerArgs {
    /// Entity name whose bindings should be removed.
    /// Mutually exclusive with --all.
    #[arg(long, conflicts_with = "all", value_name = "NAME")]
    pub entity: Option<String>,

    /// Remove all NER bindings in the namespace. Mutually exclusive with --entity.
    #[arg(long, conflicts_with = "entity", default_value_t = false)]
    pub all: bool,

    #[arg(long)]
    pub namespace: Option<String>,

    /// Preview count without deleting.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

    /// Skip confirmation for destructive operation.
    #[arg(long, default_value_t = false)]
    pub yes: bool,

    #[arg(long, value_enum, default_value = "json")]
    pub format: OutputFormat,

    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,

    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct PruneNerResponse {
    action: String,
    bindings_removed: usize,
    namespace: String,
    /// Entity name targeted, when `--entity` was used.
    #[serde(skip_serializing_if = "Option::is_none")]
    entity: Option<String>,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: PruneNerArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();

    if args.entity.is_none() && !args.all {
        return Err(AppError::Validation(
            "either --entity <NAME> or --all must be specified".to_string(),
        ));
    }

    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    let mut conn = open_rw(&paths.db)?;

    // Count how many rows would be affected.
    let count: usize = if let Some(ref entity_name) = args.entity {
        conn.query_row(
            "SELECT COUNT(*) FROM memory_entities me
             JOIN entities e ON e.id = me.entity_id
             WHERE e.name = ?1 AND e.namespace = ?2",
            rusqlite::params![entity_name, namespace],
            |r| r.get::<_, i64>(0).map(|v| v as usize),
        )?
    } else {
        conn.query_row(
            "SELECT COUNT(*) FROM memory_entities me
             JOIN entities e ON e.id = me.entity_id
             WHERE e.namespace = ?1",
            rusqlite::params![namespace],
            |r| r.get::<_, i64>(0).map(|v| v as usize),
        )?
    };

    if args.dry_run {
        let response = PruneNerResponse {
            action: "dry_run".to_string(),
            bindings_removed: count,
            namespace: namespace.clone(),
            entity: args.entity.clone(),
            elapsed_ms: inicio.elapsed().as_millis() as u64,
        };

        match args.format {
            OutputFormat::Json => output::emit_json(&response)?,
            OutputFormat::Text | OutputFormat::Markdown => {
                output::emit_text(&format!(
                    "dry_run: {count} NER bindings would be removed [{namespace}]"
                ));
            }
        }

        return Ok(());
    }

    if !args.yes {
        let response = PruneNerResponse {
            action: "aborted".to_string(),
            bindings_removed: count,
            namespace: namespace.clone(),
            entity: args.entity.clone(),
            elapsed_ms: inicio.elapsed().as_millis() as u64,
        };

        match args.format {
            OutputFormat::Json => output::emit_json(&response)?,
            OutputFormat::Text | OutputFormat::Markdown => {
                output::emit_text(&format!(
                    "aborted: {count} NER bindings would be removed; pass --yes to confirm [{namespace}]"
                ));
            }
        }

        return Ok(());
    }

    // Destructive path: COUNT + DELETE in same transaction for consistency.
    let removed: usize = if let Some(ref entity_name) = args.entity {
        // Normalize to match the normalized stored entity names.
        let entity_name = crate::parsers::normalize_entity_name(entity_name);
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let n = tx.execute(
            "DELETE FROM memory_entities WHERE entity_id IN (
                 SELECT id FROM entities WHERE name = ?1 AND namespace = ?2
             )",
            rusqlite::params![entity_name, namespace],
        )?;
        tx.commit()?;
        n
    } else {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let n = tx.execute(
            "DELETE FROM memory_entities WHERE entity_id IN (
                 SELECT id FROM entities WHERE namespace = ?1
             )",
            rusqlite::params![namespace],
        )?;
        tx.commit()?;
        n
    };

    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;

    tracing::info!(
        removed = removed,
        namespace = %namespace,
        entity = ?args.entity,
        "NER bindings pruned"
    );

    let response = PruneNerResponse {
        action: "pruned".to_string(),
        bindings_removed: removed,
        namespace: namespace.clone(),
        entity: args.entity.clone(),
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    };

    match args.format {
        OutputFormat::Json => output::emit_json(&response)?,
        OutputFormat::Text | OutputFormat::Markdown => {
            output::emit_text(&format!(
                "pruned: {removed} NER bindings removed [{namespace}]"
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_ner_response_dry_run_serializes_correctly() {
        let resp = PruneNerResponse {
            action: "dry_run".to_string(),
            bindings_removed: 42,
            namespace: "global".to_string(),
            entity: Some("jwt-token".to_string()),
            elapsed_ms: 5,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["action"], "dry_run");
        assert_eq!(json["bindings_removed"], 42);
        assert_eq!(json["entity"], "jwt-token");
        assert_eq!(json["namespace"], "global");
    }

    #[test]
    fn prune_ner_response_pruned_all_omits_entity() {
        let resp = PruneNerResponse {
            action: "pruned".to_string(),
            bindings_removed: 200,
            namespace: "project-x".to_string(),
            entity: None,
            elapsed_ms: 15,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["action"], "pruned");
        assert_eq!(json["bindings_removed"], 200);
        assert!(
            json.get("entity").is_none(),
            "entity must be omitted when None"
        );
    }

    #[test]
    fn prune_ner_response_aborted_includes_count() {
        let resp = PruneNerResponse {
            action: "aborted".to_string(),
            bindings_removed: 10,
            namespace: "global".to_string(),
            entity: None,
            elapsed_ms: 1,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["action"], "aborted");
        assert_eq!(json["bindings_removed"], 10);
        assert!(json["elapsed_ms"].is_number());
    }

    #[test]
    fn prune_ner_response_zero_bindings() {
        let resp = PruneNerResponse {
            action: "pruned".to_string(),
            bindings_removed: 0,
            namespace: "global".to_string(),
            entity: Some("nonexistent".to_string()),
            elapsed_ms: 2,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["bindings_removed"], 0);
    }
}
