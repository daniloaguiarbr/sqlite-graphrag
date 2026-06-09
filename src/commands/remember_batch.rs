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

pub fn run(args: RememberBatchArgs) -> Result<(), AppError> {
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

    if args.transaction {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        for (idx, line) in lines.iter().enumerate() {
            match process_line(&tx, &namespace, line, idx, args.force_merge, &paths) {
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
            match process_line(&tx, &namespace, line, idx, args.force_merge, &paths) {
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

    let memory_id = if let Some((existing_id, _updated_at, _version)) = existing {
        if !force_merge {
            return Err(AppError::Duplicate(format!(
                "memory '{normalized_name}' already exists; use --force-merge to update"
            )));
        }
        let snippet: String = input.body.chars().take(200).collect();
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

        let embedding = crate::embedder::embed_passage_local(&paths.models, &input.body)?;
        memories::upsert_vec(
            tx,
            existing_id,
            namespace,
            &input.r#type,
            &embedding,
            &normalized_name,
            &snippet,
        )?;
        existing_id
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
        let embedding = crate::embedder::embed_passage_local(&paths.models, &input.body)?;
        memories::upsert_vec(
            tx,
            id,
            namespace,
            &input.r#type,
            &embedding,
            &normalized_name,
            &snippet,
        )?;
        id
    };

    // Persist graph entities and relationships if provided
    for entity in &input.entities {
        let entity_id = entities::upsert_entity(tx, namespace, entity)?;
        let entity_text = match &entity.description {
            Some(desc) => format!("{} {}", entity.name, desc),
            None => entity.name.clone(),
        };
        let entity_embedding = crate::embedder::embed_passage_local(&paths.models, &entity_text)?;
        entities::upsert_entity_vec(
            tx,
            entity_id,
            namespace,
            entity.entity_type,
            &entity_embedding,
            &entity.name,
        )?;
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
        status: "indexed".to_string(),
        memory_id: Some(memory_id),
        error: None,
        index,
    })
}
