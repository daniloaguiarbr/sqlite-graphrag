//! Handler for the `link` CLI subcommand.

use crate::constants::DEFAULT_RELATION_WEIGHT;
use crate::entity_type::EntityType;
use crate::errors::AppError;
use crate::i18n::{errors_msg, validation};
use crate::output::{self, OutputFormat};
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::entities;
use crate::storage::entities::NewEntity;
use serde::Serialize;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Link two existing graph entities (extracted by GLiNER NER during `remember`)\n  \
    sqlite-graphrag link --from oauth-flow --to refresh-tokens --relation related\n\n  \
    # Auto-create entities that don't exist yet\n  \
    sqlite-graphrag link --from concept-a --to concept-b --relation depends-on --create-missing\n\n  \
    # Specify entity type for auto-created entities\n  \
    sqlite-graphrag link --from alice --to acme-corp --relation related --create-missing --entity-type person\n\n  \
    # Use a custom (non-canonical) relation type\n  \
    sqlite-graphrag link --from module-a --to module-b --relation implements --create-missing\n\n  \
    # If the entity does not exist and --create-missing is not set, the command fails with exit 4.\n  \
    # To list current entity names:\n  \
    sqlite-graphrag graph entities | jaq '.entities[].name'\n\n  \
NOTE:\n  \
    --from and --to expect ENTITY names (graph nodes), not memory names.\n  \
    Memory names are managed via remember/read/edit/forget; entities are auto-extracted\n  \
    by GLiNER NER from memory bodies or auto-created via --create-missing.")]
pub struct LinkArgs {
    /// Source ENTITY name (graph node, not memory). Entities are extracted by GLiNER NER during
    /// `remember` or auto-created via `--create-missing`. Use `graph entities` to list
    /// available entity names. Also accepts the alias `--name`.
    #[arg(long, alias = "name")]
    pub from: String,
    /// Target ENTITY name (graph node, not memory). See `--from` for sourcing entity names.
    #[arg(long)]
    pub to: String,
    /// Relation type between entities. Canonical values: applies-to, uses,
    /// depends-on, causes, fixes, contradicts, supports, follows, related,
    /// mentions, replaces, tracked-in. Any kebab-case or snake_case string
    /// is also accepted as a custom relation.
    #[arg(long, value_parser = crate::parsers::parse_relation, value_name = "RELATION")]
    pub relation: String,
    #[arg(long)]
    pub weight: Option<f64>,
    #[arg(long)]
    pub namespace: Option<String>,
    #[arg(long, value_enum, default_value = "json")]
    pub format: OutputFormat,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
    /// Auto-create entities when they do not exist. Created entities default to
    /// type `concept` unless `--entity-type` specifies a different type.
    #[arg(long, default_value_t = false)]
    pub create_missing: bool,
    /// Entity type assigned to auto-created entities (only effective with `--create-missing`).
    #[arg(long, value_enum, default_value = "concept")]
    pub entity_type: EntityType,
}

#[derive(Serialize)]
struct LinkResponse {
    action: String,
    from: String,
    to: String,
    relation: String,
    weight: f64,
    namespace: String,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
    /// Entity names that were auto-created by `--create-missing`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    created_entities: Vec<String>,
}

pub fn run(args: LinkArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    if args.from == args.to {
        return Err(AppError::Validation(validation::self_referential_link()));
    }

    let weight = args.weight.unwrap_or(DEFAULT_RELATION_WEIGHT);
    if !(0.0..=1.0).contains(&weight) {
        return Err(AppError::Validation(validation::invalid_link_weight(
            weight,
        )));
    }

    crate::storage::connection::ensure_db_ready(&paths)?;

    crate::parsers::warn_if_non_canonical(&args.relation);
    let relation_str = &args.relation;

    let mut conn = open_rw(&paths.db)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

    let mut created_entities: Vec<String> = Vec::with_capacity(2);

    let source_id = match entities::find_entity_id(&tx, &namespace, &args.from)? {
        Some(id) => id,
        None if args.create_missing => {
            let new_entity = NewEntity {
                name: args.from.clone(),
                entity_type: args.entity_type,
                description: None,
            };
            created_entities.push(args.from.clone());
            entities::upsert_entity(&tx, &namespace, &new_entity)?
        }
        None => {
            return Err(AppError::NotFound(errors_msg::entity_not_found(
                &args.from, &namespace,
            )));
        }
    };

    let target_id = match entities::find_entity_id(&tx, &namespace, &args.to)? {
        Some(id) => id,
        None if args.create_missing => {
            let new_entity = NewEntity {
                name: args.to.clone(),
                entity_type: args.entity_type,
                description: None,
            };
            created_entities.push(args.to.clone());
            entities::upsert_entity(&tx, &namespace, &new_entity)?
        }
        None => {
            return Err(AppError::NotFound(errors_msg::entity_not_found(
                &args.to, &namespace,
            )));
        }
    };

    let (_rel_id, was_created) = entities::create_or_fetch_relationship(
        &tx,
        &namespace,
        source_id,
        target_id,
        relation_str,
        weight,
        None,
    )?;

    if was_created {
        entities::recalculate_degree(&tx, source_id)?;
        entities::recalculate_degree(&tx, target_id)?;
    }
    tx.commit()?;

    let action = if was_created {
        "created".to_string()
    } else {
        "already_exists".to_string()
    };

    let response = LinkResponse {
        action: action.clone(),
        from: args.from.clone(),
        to: args.to.clone(),
        relation: relation_str.to_string(),
        weight,
        namespace: namespace.clone(),
        elapsed_ms: inicio.elapsed().as_millis() as u64,
        created_entities,
    };

    match args.format {
        OutputFormat::Json => output::emit_json(&response)?,
        OutputFormat::Text | OutputFormat::Markdown => {
            output::emit_text(&format!(
                "{}: {} --[{}]--> {} [{}]",
                action, response.from, response.relation, response.to, response.namespace
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_response_without_redundant_aliases() {
        // P1-O: source/target fields were removed from the JSON response.
        let resp = LinkResponse {
            action: "created".to_string(),
            from: "entity-a".to_string(),
            to: "entity-b".to_string(),
            relation: "uses".to_string(),
            weight: 1.0,
            namespace: "default".to_string(),
            elapsed_ms: 0,
            created_entities: vec![],
        };
        let json = serde_json::to_value(&resp).expect("serialization must work");
        assert_eq!(json["from"], "entity-a");
        assert_eq!(json["to"], "entity-b");
        assert!(
            json.get("source").is_none(),
            "field 'source' was removed in P1-O"
        );
        assert!(
            json.get("target").is_none(),
            "field 'target' was removed in P1-O"
        );
    }

    #[test]
    fn link_response_serializes_all_fields() {
        let resp = LinkResponse {
            action: "already_exists".to_string(),
            from: "origin".to_string(),
            to: "destination".to_string(),
            relation: "mentions".to_string(),
            weight: 0.8,
            namespace: "test".to_string(),
            elapsed_ms: 5,
            created_entities: vec![],
        };
        let json = serde_json::to_value(&resp).expect("serialization must work");
        assert!(json.get("action").is_some());
        assert!(json.get("from").is_some());
        assert!(json.get("to").is_some());
        assert!(json.get("relation").is_some());
        assert!(json.get("weight").is_some());
        assert!(json.get("namespace").is_some());
        assert!(json.get("elapsed_ms").is_some());
    }

    #[test]
    fn link_response_omits_created_entities_when_empty() {
        let resp = LinkResponse {
            action: "created".to_string(),
            from: "a".to_string(),
            to: "b".to_string(),
            relation: "uses".to_string(),
            weight: 1.0,
            namespace: "global".to_string(),
            elapsed_ms: 0,
            created_entities: vec![],
        };
        let json = serde_json::to_value(&resp).expect("serialization");
        assert!(
            json.get("created_entities").is_none(),
            "empty vec must be omitted"
        );
    }

    #[test]
    fn link_response_includes_created_entities_when_present() {
        let resp = LinkResponse {
            action: "created".to_string(),
            from: "new-a".to_string(),
            to: "new-b".to_string(),
            relation: "depends-on".to_string(),
            weight: 0.5,
            namespace: "test".to_string(),
            elapsed_ms: 1,
            created_entities: vec!["new-a".to_string(), "new-b".to_string()],
        };
        let json = serde_json::to_value(&resp).expect("serialization");
        let created = json["created_entities"].as_array().expect("must be array");
        assert_eq!(created.len(), 2);
        assert_eq!(created[0], "new-a");
        assert_eq!(created[1], "new-b");
    }
}
