//! Handler for the `remember-batch` CLI subcommand (G08).
//!
//! Accepts NDJSON via stdin where each line is a memory to persist.
//! One CLI invocation, one slot, one DB connection — eliminates N-process
//! contention from parallel `remember` calls.

use crate::errors::AppError;
use crate::output;
use crate::paths::AppPaths;
use crate::storage::connection::open_rw;
use crate::storage::{entities, memories, versions};
use serde::{Deserialize, Serialize};
use std::io::BufRead;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Pipe NDJSON memories from stdin\n  \
    echo '{\"name\":\"mem-a\",\"type\":\"note\",\"description\":\"a\",\"body\":\"content\"}' | \
    sqlite-graphrag remember-batch --json\n\n  \
    # Atomic batch with --transaction\n  \
    cat memories.ndjson | sqlite-graphrag remember-batch --transaction --json")]
pub struct RememberBatchArgs {
    /// Apply all memories in a single transaction (all-or-nothing).
    #[arg(long)]
    pub transaction: bool,
    /// Stop processing on the first failure.
    #[arg(long)]
    pub fail_fast: bool,
    /// Apply force-merge to all memories (update existing by name).
    #[arg(long)]
    pub force_merge: bool,
    /// Validate inputs and emit preview events without persisting or embedding.
    #[arg(long)]
    pub dry_run: bool,
    /// Namespace override for all memories.
    #[arg(long, env = "SQLITE_GRAPHRAG_NAMESPACE")]
    pub namespace: Option<String>,
    /// Emit NDJSON output.
    #[arg(long)]
    pub json: bool,
    /// Database path override.
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

#[derive(Deserialize)]
struct BatchInputLine {
    name: String,
    #[serde(default = "default_type")]
    r#type: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    entities: Vec<crate::storage::entities::NewEntity>,
    #[serde(default)]
    relationships: Vec<crate::storage::entities::NewRelationship>,
}

fn default_type() -> String {
    "note".to_string()
}

#[derive(Serialize)]
struct BatchItemEvent {
    name: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    memory_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    index: usize,
}

#[derive(Serialize)]
struct BatchSummary {
    summary: bool,
    total: usize,
    succeeded: usize,
    failed: usize,
    elapsed_ms: u64,
}

pub fn run(
    args: RememberBatchArgs,
    llm_backend: crate::cli::LlmBackendChoice,
) -> Result<(), AppError> {
    let start = std::time::Instant::now();
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let paths = AppPaths::resolve(args.db.as_deref())?;
    paths.ensure_dirs()?;
    crate::storage::connection::ensure_db_ready(&paths)?;
    let mut conn = open_rw(&paths.db)?;

    let stdin = std::io::stdin();
    let lines: Vec<String> = stdin
        .lock()
        .lines()
        .map_while(Result::ok)
        .filter(|l| !l.trim().is_empty())
        .collect();

    let total = lines.len();
    let mut succeeded = 0usize;
    let mut failed = 0usize;

    if args.dry_run {
        for (idx, line) in lines.iter().enumerate() {
            match serde_json::from_str::<BatchInputLine>(line) {
                Ok(input) => {
                    let normalized_name = crate::parsers::normalize_entity_name(&input.name);
                    if normalized_name.is_empty() {
                        failed += 1;
                        output::emit_json(&BatchItemEvent {
                            name: String::new(),
                            status: "failed".to_string(),
                            memory_id: None,
                            error: Some(format!("line {idx}: name normalizes to empty string")),
                            index: idx,
                        })?;
                        continue;
                    }
                    let existing = memories::find_by_name(&conn, &namespace, &normalized_name)?;
                    let action = if existing.is_some() {
                        if args.force_merge {
                            "would_update"
                        } else {
                            "would_fail_duplicate"
                        }
                    } else {
                        "would_create"
                    };
                    succeeded += 1;
                    output::emit_json(&BatchItemEvent {
                        name: normalized_name,
                        status: action.to_string(),
                        memory_id: existing.map(|(id, _, _)| id),
                        error: None,
                        index: idx,
                    })?;
                }
                Err(e) => {
                    failed += 1;
                    output::emit_json(&BatchItemEvent {
                        name: String::new(),
                        status: "failed".to_string(),
                        memory_id: None,
                        error: Some(format!("line {idx}: invalid JSON: {e}")),
                        index: idx,
                    })?;
                }
            }
        }

        output::emit_json(&BatchSummary {
            summary: true,
            total,
            succeeded,
            failed,
            elapsed_ms: start.elapsed().as_millis() as u64,
        })?;
        return Ok(());
    }

    if args.transaction {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        for (idx, line) in lines.iter().enumerate() {
            match process_line(
                &tx,
                &namespace,
                line,
                idx,
                args.force_merge,
                &paths,
                llm_backend,
            ) {
                Ok(event) => {
                    output::emit_json(&event)?;
                    succeeded += 1;
                }
                Err(e) => {
                    failed += 1;
                    output::emit_json(&BatchItemEvent {
                        name: String::new(),
                        status: "failed".to_string(),
                        memory_id: None,
                        error: Some(format!("{e}")),
                        index: idx,
                    })?;
                    if args.fail_fast {
                        break;
                    }
                }
            }
        }
        if failed == 0 || !args.fail_fast {
            tx.commit()?;
        }
    } else {
        for (idx, line) in lines.iter().enumerate() {
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            match process_line(
                &tx,
                &namespace,
                line,
                idx,
                args.force_merge,
                &paths,
                llm_backend,
            ) {
                Ok(event) => {
                    tx.commit()?;
                    output::emit_json(&event)?;
                    succeeded += 1;
                }
                Err(e) => {
                    drop(tx);
                    failed += 1;
                    output::emit_json(&BatchItemEvent {
                        name: String::new(),
                        status: "failed".to_string(),
                        memory_id: None,
                        error: Some(format!("{e}")),
                        index: idx,
                    })?;
                    if args.fail_fast {
                        break;
                    }
                }
            }
        }
    }

    output::emit_json(&BatchSummary {
        summary: true,
        total,
        succeeded,
        failed,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

fn process_line(
    tx: &rusqlite::Transaction<'_>,
    namespace: &str,
    line: &str,
    index: usize,
    force_merge: bool,
    paths: &AppPaths,
    llm_backend: crate::cli::LlmBackendChoice,
) -> Result<BatchItemEvent, AppError> {
    let input: BatchInputLine = serde_json::from_str(line)
        .map_err(|e| AppError::Validation(format!("line {index}: invalid JSON: {e}")))?;

    let normalized_name = crate::parsers::normalize_entity_name(&input.name);
    if normalized_name.is_empty() {
        return Err(AppError::Validation(format!(
            "line {index}: name normalizes to empty string"
        )));
    }

    let body_hash = blake3::hash(input.body.as_bytes()).to_hex().to_string();

    let existing = memories::find_by_name(tx, namespace, &normalized_name)?;

    let (memory_id, batch_action) = if let Some((existing_id, _updated_at, _version)) = existing {
        if !force_merge {
            return Err(AppError::Duplicate(format!(
                "memory '{normalized_name}' already exists; use --force-merge to update"
            )));
        }
        let snippet: String = input.body.chars().take(200).collect();
        // Capture old FTS values BEFORE the UPDATE for sync_fts_after_update
        // (trg_fts_au trigger is absent by design due to sqlite-vec conflict).
        let (old_fts_name, old_fts_desc, old_fts_body): (String, String, String) = tx.query_row(
            "SELECT name, description, body FROM memories WHERE id = ?1",
            rusqlite::params![existing_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        memories::update(
            tx,
            existing_id,
            &memories::NewMemory {
                namespace: namespace.to_string(),
                name: normalized_name.clone(),
                memory_type: input.r#type.clone(),
                description: input.description.clone(),
                body: input.body.clone(),
                body_hash,
                session_id: None,
                source: "agent".to_string(),
                metadata: serde_json::json!({}),
            },
            None,
        )?;
        memories::sync_fts_after_update(
            tx,
            existing_id,
            &old_fts_name,
            &old_fts_desc,
            &old_fts_body,
            &normalized_name,
            &input.description,
            &input.body,
        )?;
        let next_v = versions::next_version(tx, existing_id)?;
        versions::insert_version(
            tx,
            existing_id,
            next_v,
            &normalized_name,
            &input.r#type,
            &input.description,
            &input.body,
            "{}",
            None,
            "edit",
        )?;

        let skip_embed = crate::embedder::should_skip_embedding_on_failure();
        match crate::embedder::embed_passage_with_choice(
            &paths.models,
            &input.body,
            Some(llm_backend),
        ) {
            Ok((embedding, _backend)) => {
                memories::upsert_vec(
                    tx,
                    existing_id,
                    namespace,
                    &input.r#type,
                    &embedding,
                    &normalized_name,
                    &snippet,
                )?;
            }
            Err(AppError::Validation(msg)) => return Err(AppError::Validation(msg)),
            Err(e) if skip_embed => {
                tracing::warn!(error = %e, "remember-batch: embedding failed; --skip-embedding-on-failure active, persisting without embedding");
            }
            Err(e) => return Err(e),
        }
        (existing_id, "updated")
    } else {
        let new_mem = memories::NewMemory {
            namespace: namespace.to_string(),
            name: normalized_name.clone(),
            memory_type: input.r#type.clone(),
            description: input.description.clone(),
            body: input.body.clone(),
            body_hash,
            session_id: None,
            source: "agent".to_string(),
            metadata: serde_json::json!({}),
        };
        let id = memories::insert(tx, &new_mem)?;
        versions::insert_version(
            tx,
            id,
            1,
            &normalized_name,
            &input.r#type,
            &input.description,
            &input.body,
            "{}",
            None,
            "create",
        )?;

        let snippet: String = input.body.chars().take(200).collect();
        let skip_embed = crate::embedder::should_skip_embedding_on_failure();
        match crate::embedder::embed_passage_with_choice(
            &paths.models,
            &input.body,
            Some(llm_backend),
        ) {
            Ok((embedding, _backend)) => {
                memories::upsert_vec(
                    tx,
                    id,
                    namespace,
                    &input.r#type,
                    &embedding,
                    &normalized_name,
                    &snippet,
                )?;
            }
            Err(AppError::Validation(msg)) => return Err(AppError::Validation(msg)),
            Err(e) if skip_embed => {
                tracing::warn!(error = %e, "remember-batch: embedding failed; --skip-embedding-on-failure active, persisting without embedding");
            }
            Err(e) => return Err(e),
        }
        (id, "created")
    };

    // Persist graph entities and relationships if provided
    for entity in &input.entities {
        let entity_id = entities::upsert_entity(tx, namespace, entity)?;
        let entity_text = match &entity.description {
            Some(desc) => format!("{} {}", entity.name, desc),
            None => entity.name.clone(),
        };
        let skip_embed = crate::embedder::should_skip_embedding_on_failure();
        match crate::embedder::embed_entity_texts_cached(
            &paths.models,
            std::slice::from_ref(&entity_text),
            1,
        ) {
            Ok((entity_embedding_vec, _stats)) => {
                if let Some(entity_embedding) = entity_embedding_vec.into_iter().next() {
                    entities::upsert_entity_vec(
                        tx,
                        entity_id,
                        namespace,
                        entity.entity_type,
                        &entity_embedding,
                        &entity.name,
                    )?;
                }
            }
            Err(e) if skip_embed => {
                tracing::warn!(error = %e, "remember-batch: entity embedding failed; --skip-embedding-on-failure active");
            }
            Err(e) => return Err(e),
        }
        entities::link_memory_entity(tx, memory_id, entity_id)?;
    }

    for rel in &input.relationships {
        let src_name = crate::parsers::normalize_entity_name(&rel.source);
        let tgt_name = crate::parsers::normalize_entity_name(&rel.target);
        if let (Some(src_id), Some(tgt_id)) = (
            entities::find_entity_id(tx, namespace, &src_name)?,
            entities::find_entity_id(tx, namespace, &tgt_name)?,
        ) {
            entities::create_or_fetch_relationship(
                tx,
                namespace,
                src_id,
                tgt_id,
                &rel.relation,
                rel.strength,
                rel.description.as_deref(),
            )?;
        }
    }

    Ok(BatchItemEvent {
        name: normalized_name,
        status: batch_action.to_string(),
        memory_id: Some(memory_id),
        error: None,
        index,
    })
}
