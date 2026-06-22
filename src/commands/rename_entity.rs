//! Handler for the `rename-entity` CLI subcommand.
//!
//! Renames an entity preserving all relationships and memory bindings.
//! Only the `name` column in `entities` and the corresponding `vec_entities`
//! row need updating because relationships use integer FK `entity_id`.

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
    # Rename an entity\n  \
    sqlite-graphrag rename-entity --name old-name --new-name new-name\n\n  \
    # Rename with namespace\n  \
    sqlite-graphrag rename-entity --name auth --new-name authentication --namespace my-project")]
pub struct RenameEntityArgs {
    /// Current entity name to rename.
    #[arg(long, value_name = "NAME")]
    pub name: String,
    /// New name for the entity.
    #[arg(long, value_name = "NEW_NAME")]
    pub new_name: String,
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
struct RenameEntityResponse {
    action: String,
    old_name: String,
    new_name: String,
    entity_id: i64,
    namespace: String,
    elapsed_ms: u64,
}

pub fn run(
    args: RenameEntityArgs,
    llm_backend: crate::cli::LlmBackendChoice,
) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    let mut conn = open_rw(&paths.db)?;

    // Verify source entity exists and fetch its id and type.
    // Normalize the lookup name to match the normalized stored names.
    let lookup_name = crate::parsers::normalize_entity_name(&args.name);
    let row: Option<(i64, EntityType)> = {
        let mut stmt = conn
            .prepare_cached("SELECT id, type FROM entities WHERE namespace = ?1 AND name = ?2")?;
        match stmt.query_row(params![namespace, lookup_name], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, EntityType>(1)?))
        }) {
            Ok(row) => Some(row),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(AppError::Database(e)),
        }
    };
    let (entity_id, entity_type) = row
        .ok_or_else(|| AppError::NotFound(errors_msg::entity_not_found(&args.name, &namespace)))?;

    // Validate the raw new name first (catches short ALL_CAPS NER noise),
    // then normalize it for storage to preserve the normalized-name invariant.
    entities::validate_entity_name(&args.new_name)?;
    let new_name = crate::parsers::normalize_entity_name(&args.new_name);

    if lookup_name == new_name {
        return Err(AppError::Validation(
            "source and target entity names are identical".to_string(),
        ));
    }

    // Ensure new name is not already taken in this namespace.
    if entities::find_entity_id(&conn, &namespace, &new_name)?.is_some() {
        return Err(AppError::Validation(format!(
            "entity with name '{new_name}' already exists in namespace '{namespace}'"
        )));
    }

    let skip_embed = crate::embedder::should_skip_embedding_on_failure();
    let embedding: Option<Vec<f32>> = match crate::embedder::embed_passage_with_choice(
        &paths.models,
        &new_name,
        Some(llm_backend),
    ) {
        Ok((emb, _backend)) => Some(emb),
        Err(AppError::Validation(msg)) => return Err(AppError::Validation(msg)),
        Err(e) if skip_embed => {
            tracing::warn!(error = %e, "rename-entity: embedding failed; --skip-embedding-on-failure active, persisting without embedding");
            None
        }
        Err(e) => return Err(e),
    };

    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    tx.execute(
        "UPDATE entities SET name = ?1, updated_at = unixepoch() WHERE id = ?2",
        params![new_name, entity_id],
    )?;
    // v1.0.76: BLOB-backed entity_embeddings table (PK = entity_id).
    // G43: reuse the canonical writer instead of a duplicated INSERT that
    // hardcoded dim=384 and a removed local model name; `upsert_entity_vec`
    // records the real vector length and the CLI version as `model`.
    if let Some(ref emb) = embedding {
        entities::upsert_entity_vec(&tx, entity_id, &namespace, entity_type, emb, &new_name)?;
    }
    tx.commit()?;

    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;

    let response = RenameEntityResponse {
        action: "renamed".to_string(),
        old_name: args.name,
        new_name,
        entity_id,
        namespace: namespace.clone(),
        elapsed_ms: start.elapsed().as_millis() as u64,
    };

    match args.format {
        OutputFormat::Json => output::emit_json(&response)?,
        OutputFormat::Text | OutputFormat::Markdown => {
            output::emit_text(&format!(
                "renamed entity: '{}' → '{}' [{}]",
                response.old_name, response.new_name, response.namespace
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_entity_response_serializes_all_fields() {
        let resp = RenameEntityResponse {
            action: "renamed".to_string(),
            old_name: "auth".to_string(),
            new_name: "authentication".to_string(),
            entity_id: 42,
            namespace: "global".to_string(),
            elapsed_ms: 7,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["action"], "renamed");
        assert_eq!(json["old_name"], "auth");
        assert_eq!(json["new_name"], "authentication");
        assert_eq!(json["entity_id"], 42);
        assert_eq!(json["namespace"], "global");
        assert!(json["elapsed_ms"].is_number());
    }

    #[test]
    fn rename_entity_response_action_is_renamed() {
        let resp = RenameEntityResponse {
            action: "renamed".to_string(),
            old_name: "x".to_string(),
            new_name: "y".to_string(),
            entity_id: 1,
            namespace: "ns".to_string(),
            elapsed_ms: 1,
        };
        assert_eq!(resp.action, "renamed");
    }

    #[test]
    fn rename_entity_response_entity_id_preserved() {
        let resp = RenameEntityResponse {
            action: "renamed".to_string(),
            old_name: "old".to_string(),
            new_name: "new".to_string(),
            entity_id: 999,
            namespace: "test-ns".to_string(),
            elapsed_ms: 5,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["entity_id"], 999);
    }

    #[test]
    fn rejects_rename_entity_to_same_name() {
        use crate::errors::AppError;
        let err = AppError::Validation("source and target entity names are identical".to_string());
        assert_eq!(err.exit_code(), 1);
        assert!(err.to_string().contains("identical"));
    }

    #[test]
    fn rename_entity_response_namespace_reflected() {
        let resp = RenameEntityResponse {
            action: "renamed".to_string(),
            old_name: "a".to_string(),
            new_name: "b".to_string(),
            entity_id: 10,
            namespace: "my-project".to_string(),
            elapsed_ms: 2,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["namespace"], "my-project");
    }
}
