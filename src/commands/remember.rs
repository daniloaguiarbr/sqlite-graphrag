use crate::chunking;
use crate::cli::MemoryType;
use crate::errors::AppError;
use crate::output::{self, OutputFormat, RememberResponse};
use crate::paths::AppPaths;
use crate::storage::chunks as storage_chunks;
use crate::storage::connection::open_rw;
use crate::storage::entities::{NewEntity, NewRelationship};
use crate::storage::memories::NewMemory;
use crate::storage::{entities, memories, versions};
use serde::Deserialize;
use std::io::Read as _;

#[derive(clap::Args)]
pub struct RememberArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long, value_enum)]
    pub r#type: MemoryType,
    #[arg(long)]
    pub description: String,
    #[arg(long)]
    pub body: Option<String>,
    #[arg(long)]
    pub body_file: Option<std::path::PathBuf>,
    #[arg(long)]
    pub body_stdin: bool,
    #[arg(long)]
    pub entities_file: Option<std::path::PathBuf>,
    #[arg(long)]
    pub relationships_file: Option<std::path::PathBuf>,
    #[arg(long)]
    pub graph_stdin: bool,
    #[arg(long, default_value = "global")]
    pub namespace: Option<String>,
    #[arg(long)]
    pub metadata: Option<String>,
    #[arg(long)]
    pub metadata_file: Option<std::path::PathBuf>,
    #[arg(long)]
    pub force_merge: bool,
    #[arg(long)]
    pub expected_updated_at: Option<i64>,
    #[arg(long)]
    pub skip_extraction: bool,
    #[arg(long)]
    pub session_id: Option<String>,
    #[arg(long, value_enum, default_value = "json")]
    pub format: OutputFormat,
    #[arg(long, env = "NEUROGRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Deserialize, Default)]
struct GraphInput {
    #[serde(default)]
    entities: Vec<NewEntity>,
    #[serde(default)]
    relationships: Vec<NewRelationship>,
}

pub fn run(args: RememberArgs) -> Result<(), AppError> {
    use crate::constants::*;

    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;

    if args.name.is_empty() || args.name.len() > MAX_MEMORY_NAME_LEN {
        return Err(AppError::Validation(format!(
            "name must be 1-{MAX_MEMORY_NAME_LEN} chars"
        )));
    }

    {
        let slug_re = regex::Regex::new(crate::constants::SLUG_REGEX)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("regex: {e}")))?;
        if !slug_re.is_match(&args.name) {
            return Err(AppError::Validation(format!(
                "name must be kebab-case slug (lowercase letters, digits, hyphens): '{}'",
                args.name
            )));
        }
    }

    if args.description.len() > MAX_MEMORY_DESCRIPTION_LEN {
        return Err(AppError::Validation(format!(
            "description must be <= {MAX_MEMORY_DESCRIPTION_LEN} chars"
        )));
    }

    let mut raw_body = if let Some(b) = args.body {
        b
    } else if let Some(path) = args.body_file {
        std::fs::read_to_string(&path).map_err(AppError::Io)?
    } else if args.body_stdin || args.graph_stdin {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(AppError::Io)?;
        buf
    } else {
        String::new()
    };

    let mut graph = GraphInput::default();
    if !args.skip_extraction {
        if let Some(path) = args.entities_file {
            let content = std::fs::read_to_string(&path).map_err(AppError::Io)?;
            graph.entities = serde_json::from_str(&content)?;
        }
        if let Some(path) = args.relationships_file {
            let content = std::fs::read_to_string(&path).map_err(AppError::Io)?;
            graph.relationships = serde_json::from_str(&content)?;
        }
        if args.graph_stdin {
            if let Ok(g) = serde_json::from_str::<GraphInput>(&raw_body) {
                graph = g;
                raw_body = String::new();
            }
        }
    }

    if graph.entities.len() > MAX_ENTITIES_PER_MEMORY {
        return Err(AppError::LimitExceeded(format!(
            "entities exceed limit of {MAX_ENTITIES_PER_MEMORY}"
        )));
    }
    if graph.relationships.len() > MAX_RELATIONSHIPS_PER_MEMORY {
        return Err(AppError::LimitExceeded(format!(
            "relationships exceed limit of {MAX_RELATIONSHIPS_PER_MEMORY}"
        )));
    }

    if raw_body.len() > MAX_MEMORY_BODY_LEN {
        return Err(AppError::LimitExceeded(format!(
            "body exceeds {MAX_MEMORY_BODY_LEN} chars"
        )));
    }

    let metadata: serde_json::Value = if let Some(m) = args.metadata {
        serde_json::from_str(&m)?
    } else if let Some(path) = args.metadata_file {
        let content = std::fs::read_to_string(&path).map_err(AppError::Io)?;
        serde_json::from_str(&content)?
    } else {
        serde_json::json!({})
    };

    let paths = AppPaths::resolve(args.db.as_deref())?;
    let mut conn = open_rw(&paths.db)?;

    output::emit_progress("Computing embedding...");
    let embedder = crate::embedder::get_embedder(&paths.models)?;

    let chunks_info = chunking::split_into_chunks(&raw_body);
    let chunks_created = chunks_info.len();

    let (body_for_storage, embedding) = if chunks_info.len() == 1 {
        (
            raw_body.clone(),
            crate::embedder::embed_passage(embedder, &raw_body)?,
        )
    } else {
        output::emit_progress(&format!("Embedding {} chunks...", chunks_info.len()));
        let texts: Vec<String> = chunks_info.iter().map(|c| c.text.clone()).collect();
        let chunk_embeddings = crate::embedder::embed_passages_batch(embedder, &texts)?;
        let aggregated = chunking::aggregate_embeddings(&chunk_embeddings);
        (raw_body.clone(), aggregated)
    };

    let body_hash = blake3::hash(body_for_storage.as_bytes())
        .to_hex()
        .to_string();
    let snippet: String = body_for_storage.chars().take(200).collect();

    let memory_type = args.r#type.as_str();
    let new_memory = NewMemory {
        namespace: namespace.clone(),
        name: args.name.clone(),
        memory_type: memory_type.to_string(),
        description: args.description.clone(),
        body: body_for_storage.clone(),
        body_hash: body_hash.clone(),
        session_id: args.session_id.clone(),
        source: "agent".to_string(),
        metadata,
    };

    let mut warnings = Vec::new();

    let (memory_id, action, version) = match memories::find_by_name(&conn, &namespace, &args.name)?
    {
        Some((existing_id, _updated_at, _current_version)) => {
            if !args.force_merge {
                return Err(AppError::Duplicate(format!(
                    "memory '{}' already exists in namespace '{}'. Use --force-merge to update.",
                    args.name, namespace
                )));
            }
            if let Some(hash_id) = memories::find_by_hash(&conn, &namespace, &body_hash)? {
                if hash_id != existing_id {
                    warnings.push(format!(
                        "identical body already exists as memory id {hash_id}"
                    ));
                }
            }
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

            if chunks_info.len() > 1 {
                storage_chunks::delete_chunks(&tx, existing_id)?;
            }

            let next_v = versions::next_version(&tx, existing_id)?;
            memories::update(&tx, existing_id, &new_memory, args.expected_updated_at)?;
            versions::insert_version(
                &tx,
                existing_id,
                next_v,
                &args.name,
                memory_type,
                &args.description,
                &body_for_storage,
                &serde_json::to_string(&new_memory.metadata)?,
                None,
                "edit",
            )?;
            memories::upsert_vec(
                &tx,
                existing_id,
                &namespace,
                memory_type,
                &embedding,
                &args.name,
                &snippet,
            )?;
            tx.commit()?;
            (existing_id, "updated".to_string(), next_v)
        }
        None => {
            if let Some(hash_id) = memories::find_by_hash(&conn, &namespace, &body_hash)? {
                warnings.push(format!(
                    "identical body already exists as memory id {hash_id}"
                ));
            }
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let id = memories::insert(&tx, &new_memory)?;
            versions::insert_version(
                &tx,
                id,
                1,
                &args.name,
                memory_type,
                &args.description,
                &body_for_storage,
                &serde_json::to_string(&new_memory.metadata)?,
                None,
                "create",
            )?;
            memories::upsert_vec(
                &tx,
                id,
                &namespace,
                memory_type,
                &embedding,
                &args.name,
                &snippet,
            )?;
            tx.commit()?;
            (id, "created".to_string(), 1)
        }
    };

    if chunks_info.len() > 1 {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let chunks: Vec<storage_chunks::Chunk> = chunks_info
            .iter()
            .enumerate()
            .map(|(i, c)| storage_chunks::Chunk {
                memory_id,
                chunk_idx: i as i32,
                chunk_text: c.text.clone(),
                start_offset: c.start_offset as i32,
                end_offset: c.end_offset as i32,
                token_count: c.token_count_approx as i32,
            })
            .collect();
        storage_chunks::insert_chunks(&tx, &chunks)?;

        let texts: Vec<String> = chunks_info.iter().map(|c| c.text.clone()).collect();
        let chunk_embeddings = crate::embedder::embed_passages_batch(embedder, &texts)?;

        for (i, emb) in chunk_embeddings.iter().enumerate() {
            storage_chunks::upsert_chunk_vec(&tx, i as i64, memory_id, i as i32, emb)?;
        }
        tx.commit()?;
    }

    let mut entities_persisted = 0usize;
    let mut relationships_persisted = 0usize;

    if !graph.entities.is_empty() || !graph.relationships.is_empty() {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        for entity in &graph.entities {
            let entity_id = entities::upsert_entity(&tx, &namespace, entity)?;
            let entity_text = match &entity.description {
                Some(desc) => format!("{} {}", entity.name, desc),
                None => entity.name.clone(),
            };
            let entity_embedding = crate::embedder::embed_passage(embedder, &entity_text)?;
            entities::upsert_entity_vec(
                &tx,
                entity_id,
                &namespace,
                &entity.entity_type,
                &entity_embedding,
                &entity.name,
            )?;
            entities::link_memory_entity(&tx, memory_id, entity_id)?;
            entities::increment_degree(&tx, entity_id)?;
            entities_persisted += 1;
        }
        for rel in &graph.relationships {
            let source_entity = NewEntity {
                name: rel.source.clone(),
                entity_type: "concept".to_string(),
                description: None,
            };
            let target_entity = NewEntity {
                name: rel.target.clone(),
                entity_type: "concept".to_string(),
                description: None,
            };
            let source_id = entities::upsert_entity(&tx, &namespace, &source_entity)?;
            let target_id = entities::upsert_entity(&tx, &namespace, &target_entity)?;
            let rel_id = entities::upsert_relationship(&tx, &namespace, source_id, target_id, rel)?;
            entities::link_memory_relationship(&tx, memory_id, rel_id)?;
            relationships_persisted += 1;
        }
        tx.commit()?;
    }

    output::emit_json(&RememberResponse {
        memory_id,
        name: args.name,
        action,
        version,
        entities_persisted,
        relationships_persisted,
        chunks_created,
        warnings,
    })?;

    Ok(())
}
