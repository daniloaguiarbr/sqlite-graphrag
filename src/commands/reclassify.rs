//! Handler for the `reclassify` CLI subcommand (GAP-18).
//!
//! Reclassifies one entity (single mode) or a whole group of entities (batch
//! mode) by updating the `type` column in the `entities` table.
//!
//! Single mode: `--name <entity>` changes the type of one entity.
//! Batch mode: `--from-type <old> --to-type <new> --batch` changes every
//! entity in the namespace that currently has `<old>` as its type.

use crate::entity_type::EntityType;
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
    # Reclassify a single entity from its current type to 'tool'\n  \
    sqlite-graphrag reclassify --name tokio-runtime --new-type tool\n\n  \
    # Reclassify all 'concept' entities to 'tool' in one shot (batch)\n  \
    sqlite-graphrag reclassify --from-type concept --to-type tool --batch\n\n  \
    # Reclassify in a specific namespace\n  \
    sqlite-graphrag reclassify --name alice --new-type person --namespace my-project\n\n\
NOTE:\n  \
    Single mode requires --name and --new-type.\n  \
    Batch mode requires --from-type, --to-type and --batch.\n  \
    Providing --name together with --batch is an error.")]
pub struct ReclassifyArgs {
    /// Entity name to reclassify (single mode). Mutually exclusive with --from-type + --batch.
    #[arg(long, conflicts_with_all = ["from_type", "batch"])]
    pub name: Option<String>,
    /// New entity type for single mode.
    #[arg(long, value_enum, value_name = "TYPE")]
    pub new_type: Option<EntityType>,
    /// Current entity type to match in batch mode. Requires --to-type and --batch.
    #[arg(
        long,
        value_enum,
        value_name = "TYPE",
        requires = "to_type",
        requires = "batch"
    )]
    pub from_type: Option<EntityType>,
    /// New entity type to assign in batch mode. Requires --from-type and --batch.
    #[arg(long, value_enum, value_name = "TYPE", requires = "from_type")]
    pub to_type: Option<EntityType>,
    /// Enable batch reclassification (--from-type to --to-type). Requires --from-type and --to-type.
    #[arg(long, default_value_t = false, requires = "from_type")]
    pub batch: bool,
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
struct ReclassifyResponse {
    action: String,
    count: usize,
    namespace: String,
    /// Total execution time in milliseconds from handler start to serialisation.
    elapsed_ms: u64,
}

pub fn run(args: ReclassifyArgs) -> Result<(), AppError> {
    let inicio = std::time::Instant::now();
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    let mut conn = open_rw(&paths.db)?;

    let count = if args.batch {
        // Batch mode: --from-type + --to-type + --batch
        let from_type = args.from_type.ok_or_else(|| {
            AppError::Validation("--from-type is required in batch mode".to_string())
        })?;
        let to_type = args.to_type.ok_or_else(|| {
            AppError::Validation("--to-type is required in batch mode".to_string())
        })?;

        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let affected = tx.execute(
            "UPDATE entities SET type = ?1, updated_at = unixepoch()
             WHERE type = ?2 AND namespace = ?3",
            params![to_type.as_str(), from_type.as_str(), namespace],
        )?;
        tx.commit()?;
        affected
    } else {
        // Single mode: --name + --new-type
        let entity_name = args
            .name
            .as_deref()
            .ok_or_else(|| AppError::Validation("--name is required in single mode".to_string()))?;
        let new_type = args.new_type.ok_or_else(|| {
            AppError::Validation("--new-type is required in single mode".to_string())
        })?;

        // Verify entity exists.
        entities::find_entity_id(&conn, &namespace, entity_name)?.ok_or_else(|| {
            AppError::NotFound(errors_msg::entity_not_found(entity_name, &namespace))
        })?;

        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let affected = tx.execute(
            "UPDATE entities SET type = ?1, updated_at = unixepoch()
             WHERE name = ?2 AND namespace = ?3",
            params![new_type.as_str(), entity_name, namespace],
        )?;
        tx.commit()?;
        affected
    };

    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;

    let response = ReclassifyResponse {
        action: "reclassified".to_string(),
        count,
        namespace: namespace.clone(),
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    };

    match args.format {
        OutputFormat::Json => output::emit_json(&response)?,
        OutputFormat::Text | OutputFormat::Markdown => {
            output::emit_text(&format!(
                "reclassified: {} entities [{}]",
                response.count, response.namespace
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reclassify_response_serializes_all_fields() {
        let resp = ReclassifyResponse {
            action: "reclassified".to_string(),
            count: 5,
            namespace: "global".to_string(),
            elapsed_ms: 12,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["action"], "reclassified");
        assert_eq!(json["count"], 5);
        assert_eq!(json["namespace"], "global");
        assert!(json["elapsed_ms"].is_number());
    }

    #[test]
    fn reclassify_response_count_zero_is_valid() {
        let resp = ReclassifyResponse {
            action: "reclassified".to_string(),
            count: 0,
            namespace: "my-project".to_string(),
            elapsed_ms: 3,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["count"], 0);
        assert_eq!(json["action"], "reclassified");
    }

    #[test]
    fn reclassify_response_action_is_reclassified() {
        let resp = ReclassifyResponse {
            action: "reclassified".to_string(),
            count: 1,
            namespace: "ns".to_string(),
            elapsed_ms: 1,
        };
        assert_eq!(resp.action, "reclassified");
    }
}
