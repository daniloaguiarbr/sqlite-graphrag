//! Handler for the `prune-relations` CLI subcommand.

use crate::errors::AppError;
use crate::i18n;
use crate::output::{self, OutputFormat};
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::entities;
use serde::Serialize;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Preview how many 'mentions' relations would be removed\n  \
    sqlite-graphrag prune-relations --relation mentions --dry-run\n\n  \
    # Remove all 'mentions' relations without confirmation prompt\n  \
    sqlite-graphrag prune-relations --relation mentions --yes\n\n\
NOTE:\n  \
    This command permanently deletes relationships. Use --dry-run first.\n  \
    Entity degree counts are automatically recalculated after pruning.")]
pub struct PruneRelationsArgs {
    /// Relation type to delete (e.g. mentions, related, uses).
    /// Accepts canonical and custom kebab-case/snake_case values.
    #[arg(long, value_parser = crate::parsers::parse_relation, value_name = "RELATION")]
    pub relation: String,
    #[arg(long)]
    pub namespace: Option<String>,
    /// Preview count without deleting.
    #[arg(long)]
    pub dry_run: bool,
    /// Skip confirmation for destructive operation.
    #[arg(long)]
    pub yes: bool,
    #[arg(long, value_enum, default_value = "json")]
    pub format: OutputFormat,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct PruneRelationsResponse {
    action: String,
    relation: String,
    count: usize,
    entities_affected: usize,
    namespace: String,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: PruneRelationsArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    crate::parsers::warn_if_non_canonical(&args.relation);

    let mut conn = open_rw(&paths.db)?;

    if args.dry_run {
        let count = entities::count_relationships_by_relation(&conn, &namespace, &args.relation)?;

        output::emit_progress(&i18n::prune_dry_run(count, &args.relation));

        let response = PruneRelationsResponse {
            action: "dry_run".to_string(),
            relation: args.relation.clone(),
            count,
            entities_affected: 0,
            namespace: namespace.clone(),
            elapsed_ms: inicio.elapsed().as_millis() as u64,
        };

        match args.format {
            OutputFormat::Json => output::emit_json(&response)?,
            OutputFormat::Text | OutputFormat::Markdown => {
                output::emit_text(&format!(
                    "dry_run: {} '{}' relations would be removed [{}]",
                    response.count, response.relation, response.namespace
                ));
            }
        }

        return Ok(());
    }

    if !args.yes {
        output::emit_progress(&i18n::prune_requires_yes());

        let count = entities::count_relationships_by_relation(&conn, &namespace, &args.relation)?;

        let response = PruneRelationsResponse {
            action: "aborted".to_string(),
            relation: args.relation.clone(),
            count,
            entities_affected: 0,
            namespace: namespace.clone(),
            elapsed_ms: inicio.elapsed().as_millis() as u64,
        };

        match args.format {
            OutputFormat::Json => output::emit_json(&response)?,
            OutputFormat::Text | OutputFormat::Markdown => {
                output::emit_text(&format!(
                    "aborted: {} '{}' relations would be removed; pass --yes to confirm [{}]",
                    response.count, response.relation, response.namespace
                ));
            }
        }

        return Ok(());
    }

    // Destructive path: delete relationships.
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let (count, entity_ids) =
        entities::delete_relationships_by_relation(&tx, &namespace, &args.relation)?;
    tx.commit()?;

    // Run ANALYZE to refresh query planner statistics after bulk deletion.
    conn.execute_batch("ANALYZE relationships; ANALYZE memory_relationships;")?;

    output::emit_progress(&i18n::relations_pruned(count, &args.relation, &namespace));

    let response = PruneRelationsResponse {
        action: "pruned".to_string(),
        relation: args.relation.clone(),
        count,
        entities_affected: entity_ids.len(),
        namespace: namespace.clone(),
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    };

    match args.format {
        OutputFormat::Json => output::emit_json(&response)?,
        OutputFormat::Text | OutputFormat::Markdown => {
            output::emit_text(&format!(
                "pruned: {} '{}' relations removed, {} entities affected [{}]",
                response.count, response.relation, response.entities_affected, response.namespace
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_response_serializes_all_fields() {
        let resp = PruneRelationsResponse {
            action: "pruned".to_string(),
            relation: "mentions".to_string(),
            count: 3451,
            entities_affected: 200,
            namespace: "global".to_string(),
            elapsed_ms: 42,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["action"], "pruned");
        assert_eq!(json["relation"], "mentions");
        assert_eq!(json["count"], 3451);
        assert_eq!(json["entities_affected"], 200);
        assert_eq!(json["namespace"], "global");
        assert!(json["elapsed_ms"].is_number());
    }

    #[test]
    fn prune_response_action_dry_run() {
        let resp = PruneRelationsResponse {
            action: "dry_run".to_string(),
            relation: "mentions".to_string(),
            count: 100,
            entities_affected: 0,
            namespace: "test".to_string(),
            elapsed_ms: 5,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["action"], "dry_run");
        assert_eq!(
            json["entities_affected"], 0,
            "dry_run must report zero entities_affected"
        );
    }

    #[test]
    fn prune_response_action_pruned() {
        let resp = PruneRelationsResponse {
            action: "pruned".to_string(),
            relation: "uses".to_string(),
            count: 50,
            entities_affected: 10,
            namespace: "my-project".to_string(),
            elapsed_ms: 120,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["action"], "pruned");
        assert!(json["count"].as_u64().unwrap() > 0);
        assert!(json["entities_affected"].as_u64().unwrap() > 0);
    }

    #[test]
    fn prune_response_zero_count_when_nothing_to_prune() {
        let resp = PruneRelationsResponse {
            action: "pruned".to_string(),
            relation: "nonexistent".to_string(),
            count: 0,
            entities_affected: 0,
            namespace: "global".to_string(),
            elapsed_ms: 1,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["count"], 0);
        assert_eq!(json["entities_affected"], 0);
    }
}
