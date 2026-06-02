//! Handler for the `unlink` CLI subcommand.

use crate::errors::AppError;
use crate::i18n::errors_msg;
use crate::output::{self, OutputFormat};
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::entities;
use serde::Serialize;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Remove a specific relationship between two entities\n  \
    sqlite-graphrag unlink --from oauth-flow --to refresh-tokens --relation related\n\n  \
    # Remove ALL relationships between two entities (any relation type)\n  \
    sqlite-graphrag unlink --from oauth-flow --to refresh-tokens\n\n  \
    # Remove ALL relationships where an entity is source or target\n  \
    sqlite-graphrag unlink --entity oauth-flow --all\n\n  \
NOTE:\n  \
    --from and --to expect ENTITY names (graph nodes), not memory names.\n  \
    To inspect current entities and relationships, run: sqlite-graphrag graph --format json")]
pub struct UnlinkArgs {
    /// Source ENTITY name (graph node, not memory). Also accepts the aliases `--source` and `--name`.
    /// To list current entities run `graph --format json | jaq '.nodes[].name'`.
    #[arg(long, alias = "source", alias = "name", conflicts_with = "entity")]
    pub from: Option<String>,
    /// Target ENTITY name (graph node, not memory). Also accepts the alias `--target`.
    #[arg(long, alias = "target", conflicts_with = "entity")]
    pub to: Option<String>,
    /// Relation type to remove. When omitted with --from/--to, ALL relationships between
    /// those two entities are deleted. Accepts canonical values (e.g. uses, depends-on)
    /// or any custom snake_case/kebab-case string.
    #[arg(long, value_parser = crate::parsers::parse_relation, value_name = "RELATION")]
    pub relation: Option<String>,
    /// Entity name for bulk removal. Must be combined with --all.
    #[arg(long, requires = "all", conflicts_with_all = ["from", "to"])]
    pub entity: Option<String>,
    /// When combined with --entity, removes ALL relationships where that entity is source or target.
    #[arg(long, requires = "entity")]
    pub all: bool,
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
struct UnlinkResponse {
    action: String,
    from_name: String,
    to_name: String,
    relation: String,
    relationships_removed: u64,
    namespace: String,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: UnlinkArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    if let Some(relation_str) = &args.relation {
        crate::parsers::warn_if_non_canonical(relation_str);
    }

    let mut conn = open_rw(&paths.db)?;

    // Mode: --entity --all → delete every relationship for that entity.
    if args.all {
        let entity_name = args.entity.as_deref().unwrap_or("");
        let entity_id =
            entities::find_entity_id(&conn, &namespace, entity_name)?.ok_or_else(|| {
                AppError::NotFound(errors_msg::entity_not_found(entity_name, &namespace))
            })?;

        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let removed = delete_all_entity_relationships(&tx, entity_id)?;
        entities::recalculate_degree(&tx, entity_id)?;
        tx.commit()?;

        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;

        let response = UnlinkResponse {
            action: "deleted".to_string(),
            from_name: entity_name.to_string(),
            to_name: "*".to_string(),
            relation: "*".to_string(),
            relationships_removed: removed,
            namespace: namespace.clone(),
            elapsed_ms: inicio.elapsed().as_millis() as u64,
        };

        match args.format {
            OutputFormat::Json => output::emit_json(&response)?,
            OutputFormat::Text | OutputFormat::Markdown => {
                output::emit_text(&format!(
                    "deleted: {} --[*]--> * removed {} relationship(s) [{}]",
                    response.from_name, response.relationships_removed, response.namespace
                ));
            }
        }
        return Ok(());
    }

    // Mode: --from/--to (with optional --relation).
    let from_name = args.from.as_deref().ok_or_else(|| {
        AppError::Validation("--from is required when --entity/--all is not used".to_string())
    })?;
    let to_name = args.to.as_deref().ok_or_else(|| {
        AppError::Validation("--to is required when --entity/--all is not used".to_string())
    })?;

    let source_id = entities::find_entity_id(&conn, &namespace, from_name)?
        .ok_or_else(|| AppError::NotFound(errors_msg::entity_not_found(from_name, &namespace)))?;
    let target_id = entities::find_entity_id(&conn, &namespace, to_name)?
        .ok_or_else(|| AppError::NotFound(errors_msg::entity_not_found(to_name, &namespace)))?;

    let (removed, relation_display) = if let Some(rel) = args.relation.as_deref() {
        // Single-relation mode: exact match required.
        let row =
            entities::find_relationship(&conn, source_id, target_id, rel)?.ok_or_else(|| {
                AppError::NotFound(errors_msg::relationship_not_found(
                    from_name, rel, to_name, &namespace,
                ))
            })?;

        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        entities::delete_relationship_by_id(&tx, row.id)?;
        entities::recalculate_degree(&tx, source_id)?;
        entities::recalculate_degree(&tx, target_id)?;
        tx.commit()?;

        (1u64, rel.to_string())
    } else {
        // Bulk mode: delete all relationships between from and to.
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let count = delete_relationships_between(&tx, source_id, target_id)?;
        entities::recalculate_degree(&tx, source_id)?;
        entities::recalculate_degree(&tx, target_id)?;
        tx.commit()?;

        (count, "*".to_string())
    };

    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;

    let response = UnlinkResponse {
        action: "deleted".to_string(),
        from_name: from_name.to_string(),
        to_name: to_name.to_string(),
        relation: relation_display.clone(),
        relationships_removed: removed,
        namespace: namespace.clone(),
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    };

    match args.format {
        OutputFormat::Json => output::emit_json(&response)?,
        OutputFormat::Text | OutputFormat::Markdown => {
            output::emit_text(&format!(
                "deleted: {} --[{}]--> {} removed {} relationship(s) [{}]",
                response.from_name,
                response.relation,
                response.to_name,
                response.relationships_removed,
                response.namespace
            ));
        }
    }

    Ok(())
}

/// Deletes all relationships where `entity_id` is source or target.
/// Returns the number of rows removed.
fn delete_all_entity_relationships(
    conn: &rusqlite::Connection,
    entity_id: i64,
) -> Result<u64, AppError> {
    // Collect IDs first to clean up memory_relationships junction.
    let mut stmt =
        conn.prepare_cached("SELECT id FROM relationships WHERE source_id = ?1 OR target_id = ?1")?;
    let ids: Vec<i64> = stmt
        .query_map(rusqlite::params![entity_id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let count = ids.len() as u64;
    for rel_id in ids {
        conn.execute(
            "DELETE FROM memory_relationships WHERE relationship_id = ?1",
            rusqlite::params![rel_id],
        )?;
        conn.execute(
            "DELETE FROM relationships WHERE id = ?1",
            rusqlite::params![rel_id],
        )?;
    }
    Ok(count)
}

/// Deletes all relationships between `source_id` and `target_id` (any relation type).
/// Returns the number of rows removed.
fn delete_relationships_between(
    conn: &rusqlite::Connection,
    source_id: i64,
    target_id: i64,
) -> Result<u64, AppError> {
    let mut stmt = conn
        .prepare_cached("SELECT id FROM relationships WHERE source_id = ?1 AND target_id = ?2")?;
    let ids: Vec<i64> = stmt
        .query_map(rusqlite::params![source_id, target_id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let count = ids.len() as u64;
    for rel_id in ids {
        conn.execute(
            "DELETE FROM memory_relationships WHERE relationship_id = ?1",
            rusqlite::params![rel_id],
        )?;
        conn.execute(
            "DELETE FROM relationships WHERE id = ?1",
            rusqlite::params![rel_id],
        )?;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlink_response_serializes_all_fields() {
        let resp = UnlinkResponse {
            action: "deleted".to_string(),
            from_name: "entity-a".to_string(),
            to_name: "entity-b".to_string(),
            relation: "uses".to_string(),
            relationships_removed: 1,
            namespace: "global".to_string(),
            elapsed_ms: 5,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["action"], "deleted");
        assert_eq!(json["from_name"], "entity-a");
        assert_eq!(json["to_name"], "entity-b");
        assert_eq!(json["relation"], "uses");
        assert_eq!(json["relationships_removed"], 1u64);
        assert_eq!(json["namespace"], "global");
        assert_eq!(json["elapsed_ms"], 5u64);
    }

    #[test]
    fn unlink_response_action_must_be_deleted() {
        let resp = UnlinkResponse {
            action: "deleted".to_string(),
            from_name: "a".to_string(),
            to_name: "b".to_string(),
            relation: "related".to_string(),
            relationships_removed: 1,
            namespace: "global".to_string(),
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(
            json["action"], "deleted",
            "unlink action must always be 'deleted'"
        );
    }

    #[test]
    fn unlink_response_bulk_uses_wildcard_relation() {
        let resp = UnlinkResponse {
            action: "deleted".to_string(),
            from_name: "origin".to_string(),
            to_name: "destination".to_string(),
            relation: "*".to_string(),
            relationships_removed: 3,
            namespace: "project".to_string(),
            elapsed_ms: 3,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["relation"], "*");
        assert_eq!(json["relationships_removed"], 3u64);
    }

    #[test]
    fn unlink_response_entity_all_uses_wildcard_to() {
        let resp = UnlinkResponse {
            action: "deleted".to_string(),
            from_name: "oauth-flow".to_string(),
            to_name: "*".to_string(),
            relation: "*".to_string(),
            relationships_removed: 5,
            namespace: "global".to_string(),
            elapsed_ms: 2,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["to_name"], "*");
        assert_eq!(json["relation"], "*");
        assert_eq!(json["relationships_removed"], 5u64);
    }

    #[test]
    fn unlink_response_relationships_removed_field_present() {
        let resp = UnlinkResponse {
            action: "deleted".to_string(),
            from_name: "a".to_string(),
            to_name: "b".to_string(),
            relation: "uses".to_string(),
            relationships_removed: 0,
            namespace: "global".to_string(),
            elapsed_ms: 0,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert!(
            json.get("relationships_removed").is_some(),
            "relationships_removed field must be present"
        );
    }
}
