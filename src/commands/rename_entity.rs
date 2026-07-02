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
    sqlite-graphrag rename-entity --name auth --new-name authentication --namespace my-project\n\n  \
    # Rename by ID (unambiguous when homonyms exist across namespaces)\n  \
    sqlite-graphrag rename-entity --id 42 --new-name authentication")]
pub struct RenameEntityArgs {
    /// Current entity name to rename.
    #[arg(
        long,
        value_name = "NAME",
        required_unless_present = "id",
        conflicts_with = "id"
    )]
    pub name: Option<String>,
    /// v1.1.1 (P5): entity ID to rename. IDs are globally unique, so --id
    /// disambiguates homonyms across namespaces. Conflicts with --name; the
    /// entity must belong to the resolved namespace.
    #[arg(long, value_name = "ID")]
    pub id: Option<i64>,
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

/// v1.1.1 (P5): resolves an entity ID to `(id, type, stored name)`, enforcing
/// that the entity exists AND belongs to the namespace — IDs are global, so a
/// bare existence check could silently cross namespaces.
fn lookup_entity_by_id(
    conn: &rusqlite::Connection,
    namespace: &str,
    id: i64,
) -> Result<(i64, EntityType, String), AppError> {
    let mut stmt = conn
        .prepare_cached("SELECT id, type, name FROM entities WHERE id = ?1 AND namespace = ?2")?;
    match stmt.query_row(params![id, namespace], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, EntityType>(1)?,
            r.get::<_, String>(2)?,
        ))
    }) {
        Ok(row) => Ok(row),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(AppError::NotFound(format!(
            "entity id={id} not found in namespace '{namespace}'"
        ))),
        Err(e) => Err(AppError::Database(e)),
    }
}

pub fn run(
    args: RenameEntityArgs,
    llm_backend: crate::cli::LlmBackendChoice,
    embedding_backend: crate::cli::EmbeddingBackendChoice,
) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;

    crate::storage::connection::ensure_db_ready(&paths)?;

    let mut conn = open_rw(&paths.db)?;

    // Verify the source entity exists and fetch id, type and stored name —
    // by ID (v1.1.1 P5, unambiguous across homonyms) or by normalized name.
    // Existence is validated here, BEFORE any mutation.
    let (entity_id, entity_type, old_name) = match args.id {
        Some(id) => lookup_entity_by_id(&conn, &namespace, id)?,
        None => {
            let Some(ref raw_name) = args.name else {
                return Err(AppError::Validation(
                    "--name or --id is required".to_string(),
                ));
            };
            // Normalize the lookup name to match the normalized stored names.
            let lookup_name = crate::parsers::normalize_entity_name(raw_name);
            let mut stmt = conn.prepare_cached(
                "SELECT id, type FROM entities WHERE namespace = ?1 AND name = ?2",
            )?;
            match stmt.query_row(params![namespace, lookup_name], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, EntityType>(1)?))
            }) {
                Ok((id, ty)) => (id, ty, lookup_name),
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    return Err(AppError::NotFound(errors_msg::entity_not_found(
                        raw_name, &namespace,
                    )))
                }
                Err(e) => return Err(AppError::Database(e)),
            }
        }
    };

    // Validate the raw new name first (catches short ALL_CAPS NER noise),
    // then normalize it for storage to preserve the normalized-name invariant.
    entities::validate_entity_name(&args.new_name)?;
    let new_name = crate::parsers::normalize_entity_name(&args.new_name);

    if old_name == new_name {
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
    let embedding: Option<Vec<f32>> = match crate::embedder::embed_passage_with_embedding_choice(
        &paths.models,
        &new_name,
        embedding_backend,
        llm_backend,
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
        old_name,
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

    // v1.1.1 (P5): ID lookup is namespace-scoped and returns the stored name,
    // so homonyms across namespaces resolve deterministically.
    #[test]
    fn lookup_entity_by_id_disambiguates_homonyms_across_namespaces() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE entities (
                id INTEGER PRIMARY KEY,
                namespace TEXT NOT NULL,
                name TEXT NOT NULL,
                type TEXT NOT NULL,
                UNIQUE(namespace, name)
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO entities (id, namespace, name, type)
             VALUES (1, 'ns-a', 'auth', 'concept'), (2, 'ns-b', 'auth', 'tool')",
            [],
        )
        .unwrap();

        let (id, ty, name) = lookup_entity_by_id(&conn, "ns-b", 2).unwrap();
        assert_eq!(id, 2);
        assert_eq!(name, "auth");
        assert_eq!(ty, EntityType::Tool);

        let err = lookup_entity_by_id(&conn, "ns-b", 1).unwrap_err();
        assert_eq!(err.exit_code(), 4, "cross-namespace ID must be NotFound");
        assert!(err.to_string().contains("id=1"), "obtido: {err}");
    }

    // v1.1.1 (P5): --name and --id are mutually exclusive at the clap level,
    // and at least one selector is required.
    #[derive(clap::Parser)]
    struct TestCli {
        #[command(flatten)]
        args: RenameEntityArgs,
    }

    #[test]
    fn clap_rejects_name_combined_with_id() {
        use clap::Parser;
        let err =
            match TestCli::try_parse_from(["t", "--name", "auth", "--id", "42", "--new-name", "x"])
            {
                Ok(_) => panic!("expected argument conflict"),
                Err(e) => e,
            };
        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn clap_requires_name_or_id() {
        use clap::Parser;
        assert!(TestCli::try_parse_from(["t", "--new-name", "x"]).is_err());
        let ok = match TestCli::try_parse_from(["t", "--id", "7", "--new-name", "x"]) {
            Ok(cli) => cli,
            Err(e) => panic!("expected successful parse: {e}"),
        };
        assert_eq!(ok.args.id, Some(7));
        assert!(ok.args.name.is_none());
    }

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
