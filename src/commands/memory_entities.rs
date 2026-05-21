//! Handler for the `memory-entities` CLI subcommand.

use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_ro;
use rusqlite::params;
use serde::Serialize;

#[derive(clap::Args)]
#[command(
    about = "List entities linked to a specific memory",
    after_long_help = "EXAMPLES:\n  \
    # List entities connected to a memory\n  \
    sqlite-graphrag memory-entities --name my-memory\n\n  \
    # With namespace\n  \
    sqlite-graphrag memory-entities --name my-memory --namespace project"
)]
pub struct MemoryEntitiesArgs {
    #[arg(value_name = "NAME", conflicts_with = "name", help = "Memory name")]
    pub name_positional: Option<String>,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(
        long,
        help = "Namespace (env: SQLITE_GRAPHRAG_NAMESPACE, default: global)"
    )]
    pub namespace: Option<String>,
    #[arg(long, hide = true)]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Serialize)]
struct EntityBinding {
    entity_id: i64,
    name: String,
    entity_type: String,
}

#[derive(Serialize)]
struct MemoryEntitiesResponse {
    memory_name: String,
    entities: Vec<EntityBinding>,
    count: usize,
    elapsed_ms: u64,
}

pub fn run(args: MemoryEntitiesArgs) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let name = args.name_positional.or(args.name).ok_or_else(|| {
        AppError::Validation("name required: pass as positional argument or via --name".to_string())
    })?;
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;
    crate::storage::connection::ensure_db_ready(&paths)?;
    let conn = open_ro(&paths.db)?;

    let memory_id: i64 = conn
        .query_row(
            "SELECT id FROM memories WHERE namespace = ?1 AND name = ?2 AND deleted_at IS NULL",
            params![namespace, name],
            |r| r.get(0),
        )
        .map_err(|_| {
            AppError::NotFound(crate::i18n::errors_msg::memory_not_found(&name, &namespace))
        })?;

    let mut stmt = conn.prepare(
        "SELECT e.id, e.name, e.type AS entity_type
         FROM memory_entities me
         JOIN entities e ON e.id = me.entity_id
         WHERE me.memory_id = ?1
         ORDER BY e.name",
    )?;

    let entities: Vec<EntityBinding> = stmt
        .query_map(params![memory_id], |r| {
            Ok(EntityBinding {
                entity_id: r.get(0)?,
                name: r.get(1)?,
                entity_type: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let count = entities.len();

    output::emit_json(&MemoryEntitiesResponse {
        memory_name: name,
        entities,
        count,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_serializes_correctly() {
        let resp = MemoryEntitiesResponse {
            memory_name: "test-mem".to_string(),
            entities: vec![EntityBinding {
                entity_id: 1,
                name: "rust".to_string(),
                entity_type: "concept".to_string(),
            }],
            count: 1,
            elapsed_ms: 5,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["memory_name"], "test-mem");
        assert_eq!(json["count"], 1);
        assert_eq!(json["entities"][0]["name"], "rust");
    }
}
