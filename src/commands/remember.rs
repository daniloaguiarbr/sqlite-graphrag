use crate::chunking;
use crate::cli::MemoryType;
use crate::errors::AppError;
use crate::i18n::erros;
use crate::output::{self, JsonOutputFormat, RememberResponse};
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
    #[arg(
        long,
        value_name = "EPOCH_OR_RFC3339",
        value_parser = crate::parsers::parse_expected_updated_at,
        long_help = "Optimistic lock: reject if updated_at does not match. \
Accepts Unix epoch (e.g. 1700000000) or RFC 3339 (e.g. 2026-04-19T12:00:00Z)."
    )]
    pub expected_updated_at: Option<i64>,
    #[arg(long)]
    pub skip_extraction: bool,
    #[arg(long)]
    pub session_id: Option<String>,
    #[arg(long, value_enum, default_value_t = JsonOutputFormat::Json)]
    pub format: JsonOutputFormat,
    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
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

    let inicio = std::time::Instant::now();
    let _ = args.format;
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;

    if args.name.is_empty() || args.name.len() > MAX_MEMORY_NAME_LEN {
        return Err(AppError::Validation(
            crate::i18n::validacao::nome_comprimento(MAX_MEMORY_NAME_LEN),
        ));
    }

    if args.name.starts_with("__") {
        return Err(AppError::Validation(
            crate::i18n::validacao::nome_reservado(),
        ));
    }

    {
        let slug_re = regex::Regex::new(crate::constants::NAME_SLUG_REGEX)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("regex: {e}")))?;
        if !slug_re.is_match(&args.name) {
            return Err(AppError::Validation(crate::i18n::validacao::nome_kebab(
                &args.name,
            )));
        }
    }

    if args.description.len() > MAX_MEMORY_DESCRIPTION_LEN {
        return Err(AppError::Validation(
            crate::i18n::validacao::descricao_excede(MAX_MEMORY_DESCRIPTION_LEN),
        ));
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
        return Err(AppError::LimitExceeded(erros::limite_entidades(
            MAX_ENTITIES_PER_MEMORY,
        )));
    }
    if graph.relationships.len() > MAX_RELATIONSHIPS_PER_MEMORY {
        return Err(AppError::LimitExceeded(erros::limite_relacionamentos(
            MAX_RELATIONSHIPS_PER_MEMORY,
        )));
    }

    if raw_body.len() > MAX_MEMORY_BODY_LEN {
        return Err(AppError::LimitExceeded(
            crate::i18n::validacao::body_excede(MAX_MEMORY_BODY_LEN),
        ));
    }

    let metadata: serde_json::Value = if let Some(m) = args.metadata {
        serde_json::from_str(&m)?
    } else if let Some(path) = args.metadata_file {
        let content = std::fs::read_to_string(&path).map_err(AppError::Io)?;
        serde_json::from_str(&content)?
    } else {
        serde_json::json!({})
    };

    let body_hash = blake3::hash(raw_body.as_bytes()).to_hex().to_string();
    let snippet: String = raw_body.chars().take(200).collect();

    let paths = AppPaths::resolve(args.db.as_deref())?;
    let mut conn = open_rw(&paths.db)?;

    {
        use crate::constants::MAX_NAMESPACES_ACTIVE;
        let active_count: u32 = conn.query_row(
            "SELECT COUNT(DISTINCT namespace) FROM memories WHERE deleted_at IS NULL",
            [],
            |r| r.get::<_, i64>(0).map(|v| v as u32),
        )?;
        let ns_exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM memories WHERE namespace = ?1 AND deleted_at IS NULL)",
            rusqlite::params![namespace],
            |r| r.get::<_, i64>(0).map(|v| v > 0),
        )?;
        if !ns_exists && active_count >= MAX_NAMESPACES_ACTIVE {
            return Err(AppError::NamespaceError(format!(
                "limite de {MAX_NAMESPACES_ACTIVE} namespaces ativos excedido ao tentar criar '{namespace}'"
            )));
        }
    }

    let existing_memory = memories::find_by_name(&conn, &namespace, &args.name)?;
    if existing_memory.is_some() && !args.force_merge {
        return Err(AppError::Duplicate(erros::memoria_duplicada(
            &args.name, &namespace,
        )));
    }

    let duplicate_hash_id = memories::find_by_hash(&conn, &namespace, &body_hash)?;

    output::emit_progress_i18n("Computing embedding...", "Calculando embedding...");
    let embedder = crate::embedder::get_embedder(&paths.models)?;

    let chunks_info = chunking::split_into_chunks(&raw_body);
    let chunks_created = chunks_info.len();

    let mut chunk_embeddings_cache: Option<Vec<Vec<f32>>> = None;

    let embedding = if chunks_info.len() == 1 {
        crate::embedder::embed_passage(embedder, &raw_body)?
    } else {
        output::emit_progress_i18n(
            &format!("Embedding {} chunks...", chunks_info.len()),
            &format!("Embedando {} chunks...", chunks_info.len()),
        );
        let chunk_embeddings = crate::embedder::embed_passages_serial(
            embedder,
            chunks_info.iter().map(|c| c.text.as_str()),
        )?;
        let aggregated = chunking::aggregate_embeddings(&chunk_embeddings);
        chunk_embeddings_cache = Some(chunk_embeddings);
        aggregated
    };
    let body_for_storage = raw_body;

    let memory_type = args.r#type.as_str();
    let new_memory = NewMemory {
        namespace: namespace.clone(),
        name: args.name.clone(),
        memory_type: memory_type.to_string(),
        description: args.description.clone(),
        body: body_for_storage,
        body_hash: body_hash.clone(),
        session_id: args.session_id.clone(),
        source: "agent".to_string(),
        metadata,
    };

    let mut warnings = Vec::new();

    let (memory_id, action, version) = match existing_memory {
        Some((existing_id, _updated_at, _current_version)) => {
            if let Some(hash_id) = duplicate_hash_id {
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
                &new_memory.body,
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
            if let Some(hash_id) = duplicate_hash_id {
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
                &new_memory.body,
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
        storage_chunks::insert_chunk_slices(&tx, memory_id, &chunks_info)?;

        let chunk_embeddings = chunk_embeddings_cache.take().ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!(
                "chunk embeddings cache missing for multi-chunk remember path"
            ))
        })?;

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

    let created_at_epoch = chrono::Utc::now().timestamp();
    let created_at_iso = crate::tz::formatar_iso(chrono::Utc::now());

    output::emit_json(&RememberResponse {
        memory_id,
        name: args.name,
        namespace,
        action: action.clone(),
        operation: action,
        version,
        entities_persisted,
        relationships_persisted,
        chunks_created,
        merged_into_memory_id: None,
        warnings,
        created_at: created_at_epoch,
        created_at_iso,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

#[cfg(test)]
mod testes {
    use crate::output::RememberResponse;

    #[test]
    fn remember_response_serializa_campos_obrigatorios() {
        let resp = RememberResponse {
            memory_id: 42,
            name: "minha-mem".to_string(),
            namespace: "global".to_string(),
            action: "created".to_string(),
            operation: "created".to_string(),
            version: 1,
            entities_persisted: 0,
            relationships_persisted: 0,
            chunks_created: 1,
            merged_into_memory_id: None,
            warnings: vec![],
            created_at: 1_705_320_000,
            created_at_iso: "2024-01-15T12:00:00Z".to_string(),
            elapsed_ms: 55,
        };

        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(json["memory_id"], 42);
        assert_eq!(json["action"], "created");
        assert_eq!(json["operation"], "created");
        assert_eq!(json["version"], 1);
        assert_eq!(json["elapsed_ms"], 55u64);
        assert!(json["warnings"].is_array());
        assert!(json["merged_into_memory_id"].is_null());
    }

    #[test]
    fn remember_response_action_e_operation_sao_aliases() {
        let resp = RememberResponse {
            memory_id: 1,
            name: "mem".to_string(),
            namespace: "global".to_string(),
            action: "updated".to_string(),
            operation: "updated".to_string(),
            version: 2,
            entities_persisted: 3,
            relationships_persisted: 1,
            chunks_created: 2,
            merged_into_memory_id: None,
            warnings: vec![],
            created_at: 0,
            created_at_iso: "1970-01-01T00:00:00Z".to_string(),
            elapsed_ms: 0,
        };

        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(
            json["action"], json["operation"],
            "action e operation devem ser iguais"
        );
        assert_eq!(json["entities_persisted"], 3);
        assert_eq!(json["relationships_persisted"], 1);
        assert_eq!(json["chunks_created"], 2);
    }

    #[test]
    fn remember_response_warnings_lista_mensagens() {
        let resp = RememberResponse {
            memory_id: 5,
            name: "dup-mem".to_string(),
            namespace: "global".to_string(),
            action: "created".to_string(),
            operation: "created".to_string(),
            version: 1,
            entities_persisted: 0,
            relationships_persisted: 0,
            chunks_created: 1,
            merged_into_memory_id: None,
            warnings: vec!["identical body already exists as memory id 3".to_string()],
            created_at: 0,
            created_at_iso: "1970-01-01T00:00:00Z".to_string(),
            elapsed_ms: 10,
        };

        let json = serde_json::to_value(&resp).expect("serialização falhou");
        let warnings = json["warnings"]
            .as_array()
            .expect("warnings deve ser array");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].as_str().unwrap().contains("identical body"));
    }

    #[test]
    fn nome_invalido_prefixo_reservado_retorna_validation_error() {
        use crate::errors::AppError;
        // Valida a lógica de rejeição de nomes com prefixo "__" diretamente
        let nome = "__reservado";
        let resultado: Result<(), AppError> = if nome.starts_with("__") {
            Err(AppError::Validation(
                crate::i18n::validacao::nome_reservado(),
            ))
        } else {
            Ok(())
        };
        assert!(resultado.is_err());
        if let Err(AppError::Validation(msg)) = resultado {
            assert!(!msg.is_empty());
        }
    }

    #[test]
    fn nome_muito_longo_retorna_validation_error() {
        use crate::errors::AppError;
        let nome_longo = "a".repeat(crate::constants::MAX_MEMORY_NAME_LEN + 1);
        let resultado: Result<(), AppError> =
            if nome_longo.is_empty() || nome_longo.len() > crate::constants::MAX_MEMORY_NAME_LEN {
                Err(AppError::Validation(
                    crate::i18n::validacao::nome_comprimento(crate::constants::MAX_MEMORY_NAME_LEN),
                ))
            } else {
                Ok(())
            };
        assert!(resultado.is_err());
    }

    #[test]
    fn remember_response_merged_into_memory_id_some_serializa_inteiro() {
        let resp = RememberResponse {
            memory_id: 10,
            name: "mem-mergeada".to_string(),
            namespace: "global".to_string(),
            action: "updated".to_string(),
            operation: "updated".to_string(),
            version: 3,
            entities_persisted: 0,
            relationships_persisted: 0,
            chunks_created: 1,
            merged_into_memory_id: Some(7),
            warnings: vec![],
            created_at: 0,
            created_at_iso: "1970-01-01T00:00:00Z".to_string(),
            elapsed_ms: 0,
        };

        let json = serde_json::to_value(&resp).expect("serialização falhou");
        assert_eq!(json["merged_into_memory_id"], 7);
    }
}
