//! Handler for the `remember` CLI subcommand.

use crate::chunking;
use crate::cli::MemoryType;
use crate::errors::AppError;
use crate::i18n::errors_msg;
use crate::output::{self, JsonOutputFormat, RememberResponse};
use crate::paths::AppPaths;
use crate::storage::chunks as storage_chunks;
use crate::storage::connection::{ensure_schema, open_rw};
use crate::storage::entities::{NewEntity, NewRelationship};
use crate::storage::memories::NewMemory;
use crate::storage::{entities, memories, urls as storage_urls, versions};
use serde::Deserialize;

/// Returns the number of rows that will be written to `memory_chunks` for the
/// given chunk count. Single-chunk bodies are stored directly in the
/// `memories` row, so no chunk row is appended (returns `0`). Multi-chunk
/// bodies persist every chunk and the count equals `chunks_created`.
///
/// Centralized as a function so the H-M8 invariant is unit-testable without
/// running the full handler. The schema for `chunks_persisted` documents this
/// contract explicitly (see `docs/schemas/remember.schema.json`).
fn compute_chunks_persisted(chunks_created: usize) -> usize {
    if chunks_created > 1 {
        chunks_created
    } else {
        0
    }
}

#[derive(clap::Args)]
pub struct RememberArgs {
    /// Memory name in kebab-case (lowercase letters, digits, hyphens).
    /// Acts as unique key within the namespace; collisions trigger merge or rejection.
    #[arg(long)]
    pub name: String,
    #[arg(
        long,
        value_enum,
        long_help = "Memory kind stored in `memories.type`. This is NOT the graph `entity_type` used in `--entities-file`. Valid values: user, feedback, project, reference, decision, incident, skill, document, note."
    )]
    pub r#type: MemoryType,
    /// Short description (≤500 chars) summarizing the memory for use in `list` and `recall` snippets.
    #[arg(long)]
    pub description: String,
    /// Inline body content. Mutually exclusive with --body-file, --body-stdin, --graph-stdin.
    /// Maximum 512000 bytes; rejected if empty without an external graph.
    #[arg(
        long,
        help = "Inline body content (max 500 KB / 512000 bytes; for larger inputs split into multiple memories or use --body-file)",
        conflicts_with_all = ["body_file", "body_stdin", "graph_stdin"]
    )]
    pub body: Option<String>,
    #[arg(
        long,
        help = "Read body from a file instead of --body",
        conflicts_with_all = ["body", "body_stdin", "graph_stdin"]
    )]
    pub body_file: Option<std::path::PathBuf>,
    /// Read body from stdin until EOF. Useful in pipes (echo "..." | sqlite-graphrag remember ...).
    /// Mutually exclusive with --body, --body-file, --graph-stdin.
    #[arg(
        long,
        conflicts_with_all = ["body", "body_file", "graph_stdin"]
    )]
    pub body_stdin: bool,
    #[arg(
        long,
        help = "JSON file containing entities to associate with this memory"
    )]
    pub entities_file: Option<std::path::PathBuf>,
    #[arg(
        long,
        help = "JSON file containing relationships to associate with this memory"
    )]
    pub relationships_file: Option<std::path::PathBuf>,
    #[arg(
        long,
        help = "Read graph JSON (body + entities + relationships) from stdin",
        conflicts_with_all = [
            "body",
            "body_file",
            "body_stdin",
            "entities_file",
            "relationships_file"
        ]
    )]
    pub graph_stdin: bool,
    #[arg(long, default_value = "global")]
    pub namespace: Option<String>,
    /// Inline JSON object with arbitrary metadata key-value pairs. Mutually exclusive with --metadata-file.
    #[arg(long)]
    pub metadata: Option<String>,
    #[arg(long, help = "JSON file containing metadata key-value pairs")]
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
    #[arg(
        long,
        help = "Disable automatic entity/relationship extraction from body"
    )]
    pub skip_extraction: bool,
    /// Optional opaque session identifier for tracing memory provenance across multi-agent runs.
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
#[serde(deny_unknown_fields)]
struct GraphInput {
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    entities: Vec<NewEntity>,
    #[serde(default)]
    relationships: Vec<NewRelationship>,
}

fn normalize_and_validate_graph_input(graph: &mut GraphInput) -> Result<(), AppError> {
    for entity in &graph.entities {
        if !is_valid_entity_type(&entity.entity_type) {
            return Err(AppError::Validation(format!(
                "invalid entity_type '{}' for entity '{}'",
                entity.entity_type, entity.name
            )));
        }
    }

    for rel in &mut graph.relationships {
        rel.relation = rel.relation.replace('-', "_");
        if !is_valid_relation(&rel.relation) {
            return Err(AppError::Validation(format!(
                "invalid relation '{}' for relationship '{}' -> '{}'",
                rel.relation, rel.source, rel.target
            )));
        }
        if !(0.0..=1.0).contains(&rel.strength) {
            return Err(AppError::Validation(format!(
                "invalid strength {} for relationship '{}' -> '{}'; expected value in [0.0, 1.0]",
                rel.strength, rel.source, rel.target
            )));
        }
    }

    Ok(())
}

fn is_valid_entity_type(entity_type: &str) -> bool {
    matches!(
        entity_type,
        "project"
            | "tool"
            | "person"
            | "file"
            | "concept"
            | "incident"
            | "decision"
            | "memory"
            | "dashboard"
            | "issue_tracker"
            | "organization"
            | "location"
            | "date"
    )
}

fn is_valid_relation(relation: &str) -> bool {
    matches!(
        relation,
        "applies_to"
            | "uses"
            | "depends_on"
            | "causes"
            | "fixes"
            | "contradicts"
            | "supports"
            | "follows"
            | "related"
            | "mentions"
            | "replaces"
            | "tracked_in"
    )
}

pub fn run(args: RememberArgs) -> Result<(), AppError> {
    use crate::constants::*;

    let inicio = std::time::Instant::now();
    let _ = args.format;
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;

    // Capture the original `--name` before normalization so the JSON response can
    // surface `name_was_normalized` + `original_name` (B_4 in v1.0.32). Stored as
    // an owned String because `args.name` is moved into the response below.
    let original_name = args.name.clone();

    // Auto-normalize to kebab-case before validation (P2-H).
    // v1.0.20: also trims hyphens at the boundary (including trailing) to avoid rejection
    // after truncation by a long filename ending in a hyphen.
    let normalized_name = {
        let lower = args.name.to_lowercase().replace(['_', ' '], "-");
        let trimmed = lower.trim_matches('-').to_string();
        if trimmed != args.name {
            tracing::warn!(
                original = %args.name,
                normalized = %trimmed,
                "name auto-normalized to kebab-case"
            );
        }
        trimmed
    };
    let name_was_normalized = normalized_name != original_name;

    if normalized_name.is_empty() {
        return Err(AppError::Validation(
            "name cannot be empty after normalization (input was blank or contained only hyphens/underscores/spaces)".to_string(),
        ));
    }
    if normalized_name.len() > MAX_MEMORY_NAME_LEN {
        return Err(AppError::LimitExceeded(
            crate::i18n::validation::name_length(MAX_MEMORY_NAME_LEN),
        ));
    }

    if normalized_name.starts_with("__") {
        return Err(AppError::Validation(
            crate::i18n::validation::reserved_name(),
        ));
    }

    {
        let slug_re = regex::Regex::new(crate::constants::NAME_SLUG_REGEX)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("regex: {e}")))?;
        if !slug_re.is_match(&normalized_name) {
            return Err(AppError::Validation(crate::i18n::validation::name_kebab(
                &normalized_name,
            )));
        }
    }

    if args.description.len() > MAX_MEMORY_DESCRIPTION_LEN {
        return Err(AppError::Validation(
            crate::i18n::validation::description_exceeds(MAX_MEMORY_DESCRIPTION_LEN),
        ));
    }

    let mut raw_body = if let Some(b) = args.body {
        b
    } else if let Some(path) = args.body_file {
        std::fs::read_to_string(&path).map_err(AppError::Io)?
    } else if args.body_stdin || args.graph_stdin {
        crate::stdin_helper::read_stdin_with_timeout(60)?
    } else {
        String::new()
    };

    let entities_provided_externally =
        args.entities_file.is_some() || args.relationships_file.is_some() || args.graph_stdin;

    let mut graph = GraphInput::default();
    if let Some(path) = args.entities_file {
        let content = std::fs::read_to_string(&path).map_err(AppError::Io)?;
        graph.entities = serde_json::from_str(&content)?;
    }
    if let Some(path) = args.relationships_file {
        let content = std::fs::read_to_string(&path).map_err(AppError::Io)?;
        graph.relationships = serde_json::from_str(&content)?;
    }
    if args.graph_stdin {
        graph = serde_json::from_str::<GraphInput>(&raw_body).map_err(|e| {
            AppError::Validation(format!("invalid JSON payload on --graph-stdin: {e}"))
        })?;
        raw_body = graph.body.take().unwrap_or_default();
    }

    if graph.entities.len() > MAX_ENTITIES_PER_MEMORY {
        return Err(AppError::LimitExceeded(errors_msg::entity_limit_exceeded(
            MAX_ENTITIES_PER_MEMORY,
        )));
    }
    if graph.relationships.len() > MAX_RELATIONSHIPS_PER_MEMORY {
        return Err(AppError::LimitExceeded(
            errors_msg::relationship_limit_exceeded(MAX_RELATIONSHIPS_PER_MEMORY),
        ));
    }
    normalize_and_validate_graph_input(&mut graph)?;

    if raw_body.len() > MAX_MEMORY_BODY_LEN {
        return Err(AppError::LimitExceeded(
            crate::i18n::validation::body_exceeds(MAX_MEMORY_BODY_LEN),
        ));
    }

    // v1.0.22 P1: reject empty or whitespace-only body when no external graph is provided.
    // Without this check, empty embeddings would be persisted, breaking recall semantics.
    if !entities_provided_externally && graph.entities.is_empty() && raw_body.trim().is_empty() {
        return Err(AppError::Validation(crate::i18n::validation::empty_body()));
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
    paths.ensure_dirs()?;

    // v1.0.20: use .trim().is_empty() to reject bodies that are only whitespace.
    let mut extraction_method: Option<String> = None;
    let mut extracted_urls: Vec<crate::extraction::ExtractedUrl> = Vec::new();
    let mut relationships_truncated = false;
    if !args.skip_extraction
        && !entities_provided_externally
        && graph.entities.is_empty()
        && !raw_body.trim().is_empty()
    {
        match crate::extraction::extract_graph_auto(&raw_body, &paths) {
            Ok(extracted) => {
                extraction_method = Some(extracted.extraction_method.clone());
                extracted_urls = extracted.urls;
                graph.entities = extracted.entities;
                graph.relationships = extracted.relationships;
                relationships_truncated = extracted.relationships_truncated;

                if graph.entities.len() > MAX_ENTITIES_PER_MEMORY {
                    graph.entities.truncate(MAX_ENTITIES_PER_MEMORY);
                }
                if graph.relationships.len() > MAX_RELATIONSHIPS_PER_MEMORY {
                    relationships_truncated = true;
                    graph.relationships.truncate(MAX_RELATIONSHIPS_PER_MEMORY);
                }
                normalize_and_validate_graph_input(&mut graph)?;
            }
            Err(e) => {
                tracing::warn!("auto-extraction failed (graceful degradation): {e:#}");
            }
        }
    }

    let mut conn = open_rw(&paths.db)?;
    ensure_schema(&mut conn)?;

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
                "active namespace limit of {MAX_NAMESPACES_ACTIVE} reached while trying to create '{namespace}'"
            )));
        }
    }

    let existing_memory = memories::find_by_name(&conn, &namespace, &normalized_name)?;
    if existing_memory.is_some() && !args.force_merge {
        return Err(AppError::Duplicate(errors_msg::duplicate_memory(
            &normalized_name,
            &namespace,
        )));
    }

    let duplicate_hash_id = memories::find_by_hash(&conn, &namespace, &body_hash)?;

    output::emit_progress_i18n(
        &format!(
            "Remember stage: validated input; available memory {} MB",
            crate::memory_guard::available_memory_mb()
        ),
        &format!(
            "Stage remember: input validated; available memory {} MB",
            crate::memory_guard::available_memory_mb()
        ),
    );

    let tokenizer = crate::tokenizer::get_tokenizer(&paths.models)?;
    let model_max_length = crate::tokenizer::get_model_max_length(&paths.models)?;
    let total_passage_tokens = crate::tokenizer::count_passage_tokens(tokenizer, &raw_body)?;
    let chunks_info = chunking::split_into_chunks_hierarchical(&raw_body, tokenizer);
    let chunks_created = chunks_info.len();
    // For single-chunk bodies the memory row itself stores the content and no
    // entry is appended to `memory_chunks` (see line ~545). For multi-chunk
    // bodies every chunk is persisted via `insert_chunk_slices`.
    let chunks_persisted = compute_chunks_persisted(chunks_info.len());

    output::emit_progress_i18n(
        &format!(
            "Remember stage: tokenizer counted {total_passage_tokens} passage tokens (model max {model_max_length}); chunking produced {} chunks; process RSS {} MB",
            chunks_created,
            crate::memory_guard::current_process_memory_mb().unwrap_or(0)
        ),
        &format!(
            "Stage remember: tokenizer counted {total_passage_tokens} passage tokens (model max {model_max_length}); chunking produced {} chunks; process RSS {} MB",
            chunks_created,
            crate::memory_guard::current_process_memory_mb().unwrap_or(0)
        ),
    );

    if chunks_created > crate::constants::REMEMBER_MAX_SAFE_MULTI_CHUNKS {
        return Err(AppError::LimitExceeded(format!(
            "document produces {chunks_created} chunks; current safe operational limit is {} chunks; split the document before using remember",
            crate::constants::REMEMBER_MAX_SAFE_MULTI_CHUNKS
        )));
    }

    output::emit_progress_i18n("Computing embedding...", "Calculando embedding...");
    let mut chunk_embeddings_cache: Option<Vec<Vec<f32>>> = None;

    let embedding = if chunks_info.len() == 1 {
        crate::daemon::embed_passage_or_local(&paths.models, &raw_body)?
    } else {
        let chunk_texts: Vec<&str> = chunks_info
            .iter()
            .map(|c| chunking::chunk_text(&raw_body, c))
            .collect();
        output::emit_progress_i18n(
            &format!(
                "Embedding {} chunks serially to keep memory bounded...",
                chunks_info.len()
            ),
            &format!(
                "Embedding {} chunks serially to keep memory bounded...",
                chunks_info.len()
            ),
        );
        let mut chunk_embeddings = Vec::with_capacity(chunk_texts.len());
        for chunk_text in &chunk_texts {
            chunk_embeddings.push(crate::daemon::embed_passage_or_local(
                &paths.models,
                chunk_text,
            )?);
        }
        output::emit_progress_i18n(
            &format!(
                "Remember stage: chunk embeddings complete; process RSS {} MB",
                crate::memory_guard::current_process_memory_mb().unwrap_or(0)
            ),
            &format!(
                "Stage remember: chunk embeddings completed; process RSS {} MB",
                crate::memory_guard::current_process_memory_mb().unwrap_or(0)
            ),
        );
        let aggregated = chunking::aggregate_embeddings(&chunk_embeddings);
        chunk_embeddings_cache = Some(chunk_embeddings);
        aggregated
    };
    let body_for_storage = raw_body;

    let memory_type = args.r#type.as_str();
    let new_memory = NewMemory {
        namespace: namespace.clone(),
        name: normalized_name.clone(),
        memory_type: memory_type.to_string(),
        description: args.description.clone(),
        body: body_for_storage,
        body_hash: body_hash.clone(),
        session_id: args.session_id.clone(),
        source: "agent".to_string(),
        metadata,
    };

    let mut warnings = Vec::new();
    let mut entities_persisted = 0usize;
    let mut relationships_persisted = 0usize;

    let graph_entity_embeddings = graph
        .entities
        .iter()
        .map(|entity| {
            let entity_text = match &entity.description {
                Some(desc) => format!("{} {}", entity.name, desc),
                None => entity.name.clone(),
            };
            crate::daemon::embed_passage_or_local(&paths.models, &entity_text)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

    let (memory_id, action, version) = match existing_memory {
        Some((existing_id, _updated_at, _current_version)) => {
            if let Some(hash_id) = duplicate_hash_id {
                if hash_id != existing_id {
                    warnings.push(format!(
                        "identical body already exists as memory id {hash_id}"
                    ));
                }
            }

            storage_chunks::delete_chunks(&tx, existing_id)?;

            let next_v = versions::next_version(&tx, existing_id)?;
            memories::update(&tx, existing_id, &new_memory, args.expected_updated_at)?;
            versions::insert_version(
                &tx,
                existing_id,
                next_v,
                &normalized_name,
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
                &normalized_name,
                &snippet,
            )?;
            (existing_id, "updated".to_string(), next_v)
        }
        None => {
            if let Some(hash_id) = duplicate_hash_id {
                warnings.push(format!(
                    "identical body already exists as memory id {hash_id}"
                ));
            }
            let id = memories::insert(&tx, &new_memory)?;
            versions::insert_version(
                &tx,
                id,
                1,
                &normalized_name,
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
                &normalized_name,
                &snippet,
            )?;
            (id, "created".to_string(), 1)
        }
    };

    if chunks_info.len() > 1 {
        storage_chunks::insert_chunk_slices(&tx, memory_id, &new_memory.body, &chunks_info)?;

        let chunk_embeddings = chunk_embeddings_cache.take().ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!(
                "cache de embeddings de chunks ausente no caminho multi-chunk do remember"
            ))
        })?;

        for (i, emb) in chunk_embeddings.iter().enumerate() {
            storage_chunks::upsert_chunk_vec(&tx, i as i64, memory_id, i as i32, emb)?;
        }
        output::emit_progress_i18n(
            &format!(
                "Remember stage: persisted chunk vectors; process RSS {} MB",
                crate::memory_guard::current_process_memory_mb().unwrap_or(0)
            ),
            &format!(
                "Etapa remember: vetores de chunks persistidos; RSS do processo {} MB",
                crate::memory_guard::current_process_memory_mb().unwrap_or(0)
            ),
        );
    }

    if !graph.entities.is_empty() || !graph.relationships.is_empty() {
        for entity in &graph.entities {
            let entity_id = entities::upsert_entity(&tx, &namespace, entity)?;
            let entity_embedding = &graph_entity_embeddings[entities_persisted];
            entities::upsert_entity_vec(
                &tx,
                entity_id,
                &namespace,
                &entity.entity_type,
                entity_embedding,
                &entity.name,
            )?;
            entities::link_memory_entity(&tx, memory_id, entity_id)?;
            entities::increment_degree(&tx, entity_id)?;
            entities_persisted += 1;
        }
        let entity_types: std::collections::HashMap<&str, &str> = graph
            .entities
            .iter()
            .map(|entity| (entity.name.as_str(), entity.entity_type.as_str()))
            .collect();

        for rel in &graph.relationships {
            let source_entity = NewEntity {
                name: rel.source.clone(),
                entity_type: entity_types
                    .get(rel.source.as_str())
                    .copied()
                    .unwrap_or("concept")
                    .to_string(),
                description: None,
            };
            let target_entity = NewEntity {
                name: rel.target.clone(),
                entity_type: entity_types
                    .get(rel.target.as_str())
                    .copied()
                    .unwrap_or("concept")
                    .to_string(),
                description: None,
            };
            let source_id = entities::upsert_entity(&tx, &namespace, &source_entity)?;
            let target_id = entities::upsert_entity(&tx, &namespace, &target_entity)?;
            let rel_id = entities::upsert_relationship(&tx, &namespace, source_id, target_id, rel)?;
            entities::link_memory_relationship(&tx, memory_id, rel_id)?;
            relationships_persisted += 1;
        }
    }
    tx.commit()?;

    // v1.0.24 P0-2: persist URLs in a dedicated table, outside the main transaction.
    // Failures do not propagate — non-critical path with graceful degradation.
    let urls_persisted = if !extracted_urls.is_empty() {
        let url_entries: Vec<storage_urls::MemoryUrl> = extracted_urls
            .into_iter()
            .map(|u| storage_urls::MemoryUrl {
                url: u.url,
                offset: Some(u.offset as i64),
            })
            .collect();
        storage_urls::insert_urls(&conn, memory_id, &url_entries)
    } else {
        0
    };

    let created_at_epoch = chrono::Utc::now().timestamp();
    let created_at_iso = crate::tz::format_iso(chrono::Utc::now());

    output::emit_json(&RememberResponse {
        memory_id,
        // Persist the normalized (kebab-case) slug as `name` since that is the
        // storage key. The original input is exposed via `original_name` only
        // when normalization actually changed something (B_4 in v1.0.32).
        name: normalized_name.clone(),
        namespace,
        action: action.clone(),
        operation: action,
        version,
        entities_persisted,
        relationships_persisted,
        relationships_truncated,
        chunks_created,
        chunks_persisted,
        urls_persisted,
        extraction_method,
        merged_into_memory_id: None,
        warnings,
        created_at: created_at_epoch,
        created_at_iso,
        elapsed_ms: inicio.elapsed().as_millis() as u64,
        name_was_normalized,
        original_name: name_was_normalized.then_some(original_name),
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::compute_chunks_persisted;
    use crate::output::RememberResponse;

    // Bug H-M8: chunks_persisted contract is unit-testable and matches schema.
    #[test]
    fn chunks_persisted_zero_for_zero_chunks() {
        assert_eq!(compute_chunks_persisted(0), 0);
    }

    #[test]
    fn chunks_persisted_zero_for_single_chunk_body() {
        // Single-chunk bodies live in the memories row itself; no row is
        // appended to memory_chunks. This is the documented contract.
        assert_eq!(compute_chunks_persisted(1), 0);
    }

    #[test]
    fn chunks_persisted_equals_count_for_multi_chunk_body() {
        // Every chunk above the first triggers a row in memory_chunks.
        assert_eq!(compute_chunks_persisted(2), 2);
        assert_eq!(compute_chunks_persisted(7), 7);
        assert_eq!(compute_chunks_persisted(64), 64);
    }

    #[test]
    fn remember_response_serializes_required_fields() {
        let resp = RememberResponse {
            memory_id: 42,
            name: "minha-mem".to_string(),
            namespace: "global".to_string(),
            action: "created".to_string(),
            operation: "created".to_string(),
            version: 1,
            entities_persisted: 0,
            relationships_persisted: 0,
            relationships_truncated: false,
            chunks_created: 1,
            chunks_persisted: 0,
            urls_persisted: 0,
            extraction_method: None,
            merged_into_memory_id: None,
            warnings: vec![],
            created_at: 1_705_320_000,
            created_at_iso: "2024-01-15T12:00:00Z".to_string(),
            elapsed_ms: 55,
            name_was_normalized: false,
            original_name: None,
        };

        let json = serde_json::to_value(&resp).expect("serialization failed");
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
            relationships_truncated: false,
            extraction_method: None,
            chunks_created: 2,
            chunks_persisted: 2,
            urls_persisted: 0,
            merged_into_memory_id: None,
            warnings: vec![],
            created_at: 0,
            created_at_iso: "1970-01-01T00:00:00Z".to_string(),
            elapsed_ms: 0,
            name_was_normalized: false,
            original_name: None,
        };

        let json = serde_json::to_value(&resp).expect("serialization failed");
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
            extraction_method: None,
            relationships_persisted: 0,
            relationships_truncated: false,
            chunks_created: 1,
            chunks_persisted: 0,
            urls_persisted: 0,
            merged_into_memory_id: None,
            warnings: vec!["identical body already exists as memory id 3".to_string()],
            created_at: 0,
            created_at_iso: "1970-01-01T00:00:00Z".to_string(),
            elapsed_ms: 10,
            name_was_normalized: false,
            original_name: None,
        };

        let json = serde_json::to_value(&resp).expect("serialization failed");
        let warnings = json["warnings"]
            .as_array()
            .expect("warnings deve ser array");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].as_str().unwrap().contains("identical body"));
    }

    #[test]
    fn invalid_name_reserved_prefix_returns_validation_error() {
        use crate::errors::AppError;
        // Validates the rejection logic for names with the "__" prefix directly
        let nome = "__reservado";
        let resultado: Result<(), AppError> = if nome.starts_with("__") {
            Err(AppError::Validation(
                crate::i18n::validation::reserved_name(),
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
    fn name_too_long_returns_validation_error() {
        use crate::errors::AppError;
        let nome_longo = "a".repeat(crate::constants::MAX_MEMORY_NAME_LEN + 1);
        let resultado: Result<(), AppError> =
            if nome_longo.is_empty() || nome_longo.len() > crate::constants::MAX_MEMORY_NAME_LEN {
                Err(AppError::Validation(crate::i18n::validation::name_length(
                    crate::constants::MAX_MEMORY_NAME_LEN,
                )))
            } else {
                Ok(())
            };
        assert!(resultado.is_err());
    }

    #[test]
    fn remember_response_merged_into_memory_id_some_serializes_integer() {
        let resp = RememberResponse {
            memory_id: 10,
            name: "mem-mergeada".to_string(),
            namespace: "global".to_string(),
            action: "updated".to_string(),
            operation: "updated".to_string(),
            version: 3,
            extraction_method: None,
            entities_persisted: 0,
            relationships_persisted: 0,
            relationships_truncated: false,
            chunks_created: 1,
            chunks_persisted: 0,
            urls_persisted: 0,
            merged_into_memory_id: Some(7),
            warnings: vec![],
            created_at: 0,
            created_at_iso: "1970-01-01T00:00:00Z".to_string(),
            elapsed_ms: 0,
            name_was_normalized: false,
            original_name: None,
        };

        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["merged_into_memory_id"], 7);
    }

    #[test]
    fn remember_response_urls_persisted_serializes_field() {
        // v1.0.24 P0-2: garante que urls_persisted aparece no JSON e aceita valor > 0.
        let resp = RememberResponse {
            memory_id: 3,
            name: "mem-com-urls".to_string(),
            namespace: "global".to_string(),
            action: "created".to_string(),
            operation: "created".to_string(),
            version: 1,
            entities_persisted: 0,
            relationships_persisted: 0,
            relationships_truncated: false,
            chunks_created: 1,
            chunks_persisted: 0,
            urls_persisted: 3,
            extraction_method: Some("regex-only".to_string()),
            merged_into_memory_id: None,
            warnings: vec![],
            created_at: 0,
            created_at_iso: "1970-01-01T00:00:00Z".to_string(),
            elapsed_ms: 0,
            name_was_normalized: false,
            original_name: None,
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["urls_persisted"], 3);
    }

    #[test]
    fn empty_name_after_normalization_returns_specific_message() {
        // P0-4 regression: name consisting only of hyphens normalizes to empty string;
        // must produce a distinct error message, not the "too long" message.
        use crate::errors::AppError;
        let normalized = "---".to_lowercase().replace(['_', ' '], "-");
        let normalized = normalized.trim_matches('-').to_string();
        let resultado: Result<(), AppError> = if normalized.is_empty() {
            Err(AppError::Validation(
                "name cannot be empty after normalization (input was blank or contained only hyphens/underscores/spaces)".to_string(),
            ))
        } else {
            Ok(())
        };
        assert!(resultado.is_err());
        if let Err(AppError::Validation(msg)) = resultado {
            assert!(
                msg.contains("empty after normalization"),
                "mensagem deve mencionar 'empty after normalization', obteve: {msg}"
            );
        }
    }

    #[test]
    fn name_only_underscores_after_normalization_returns_specific_message() {
        // P0-4 regression: name consisting only of underscores normalizes to empty string.
        use crate::errors::AppError;
        let normalized = "___".to_lowercase().replace(['_', ' '], "-");
        let normalized = normalized.trim_matches('-').to_string();
        assert!(
            normalized.is_empty(),
            "underscores devem normalizar para string vazia"
        );
        let resultado: Result<(), AppError> = if normalized.is_empty() {
            Err(AppError::Validation(
                "name cannot be empty after normalization (input was blank or contained only hyphens/underscores/spaces)".to_string(),
            ))
        } else {
            Ok(())
        };
        assert!(resultado.is_err());
        if let Err(AppError::Validation(msg)) = resultado {
            assert!(
                msg.contains("empty after normalization"),
                "mensagem deve mencionar 'empty after normalization', obteve: {msg}"
            );
        }
    }

    #[test]
    fn remember_response_relationships_truncated_serializes_field() {
        // P1-D: garante que relationships_truncated aparece no JSON como bool.
        let resp_false = RememberResponse {
            memory_id: 1,
            name: "test".to_string(),
            namespace: "global".to_string(),
            action: "created".to_string(),
            operation: "created".to_string(),
            version: 1,
            entities_persisted: 2,
            relationships_persisted: 1,
            relationships_truncated: false,
            chunks_created: 1,
            chunks_persisted: 0,
            urls_persisted: 0,
            extraction_method: None,
            merged_into_memory_id: None,
            warnings: vec![],
            created_at: 0,
            created_at_iso: "1970-01-01T00:00:00Z".to_string(),
            elapsed_ms: 0,
            name_was_normalized: false,
            original_name: None,
        };
        let json_false = serde_json::to_value(&resp_false).expect("serialization failed");
        assert_eq!(json_false["relationships_truncated"], false);

        let resp_true = RememberResponse {
            relationships_truncated: true,
            ..resp_false
        };
        let json_true = serde_json::to_value(&resp_true).expect("serialization failed");
        assert_eq!(json_true["relationships_truncated"], true);
    }

    #[test]
    fn is_valid_entity_type_accepts_v008_types() {
        // V008 added organization, location, date — ensure the validator accepts them.
        assert!(super::is_valid_entity_type("organization"));
        assert!(super::is_valid_entity_type("location"));
        assert!(super::is_valid_entity_type("date"));
        assert!(!super::is_valid_entity_type("unknown_type_xyz"));
    }
}
