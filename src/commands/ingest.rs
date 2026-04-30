//! Handler for the `ingest` CLI subcommand.
//!
//! Bulk-ingests every file under a directory that matches a glob pattern.
//! Each matched file is persisted as a separate memory using the same
//! validation, chunking, embedding and persistence pipeline as `remember`,
//! but executed in-process so the ONNX model is loaded only once per
//! invocation. This is the v1.0.32 Onda 4B (finding A2) refactor that
//! replaced a fork-spawn-per-file pipeline (every file paid the ~17s ONNX
//! cold-start cost) with an in-process loop reusing the warm embedder
//! (daemon when available, in-process `Embedder::new` otherwise).
//!
//! Memory names are derived from file basenames (kebab-case, lowercase,
//! ASCII alphanumerics + hyphens). Output is line-delimited JSON: one
//! object per processed file (success or error), followed by a final
//! summary object. Designed for streaming consumption by agents.

use crate::chunking;
use crate::cli::MemoryType;
use crate::errors::AppError;
use crate::i18n::errors_msg;
use crate::output::{self, JsonOutputFormat};
use crate::paths::AppPaths;
use crate::storage::chunks as storage_chunks;
use crate::storage::connection::{ensure_db_ready, open_rw};
use crate::storage::entities::{NewEntity, NewRelationship};
use crate::storage::memories::NewMemory;
use crate::storage::{entities, memories, urls as storage_urls, versions};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Maximum length of a derived kebab-case name. Longer basenames are truncated
/// (with a `tracing::warn!`) to keep the `memories.name` column bounded.
const DERIVED_NAME_MAX_LEN: usize = 60;

/// Hard cap on the numeric suffix appended for collision resolution. If 1000
/// candidates collide we surface an error rather than loop forever.
const MAX_NAME_COLLISION_SUFFIX: usize = 1000;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Ingest every Markdown file under ./docs as `document` memories\n  \
    sqlite-graphrag ingest ./docs --type document\n\n  \
    # Ingest .txt files recursively under ./notes\n  \
    sqlite-graphrag ingest ./notes --type note --pattern '*.txt' --recursive\n\n  \
    # Skip BERT NER auto-extraction for faster bulk import\n  \
    sqlite-graphrag ingest ./big-corpus --type reference --skip-extraction\n\n  \
NOTES:\n  \
    Each file becomes a separate memory. Names derive from file basenames\n  \
    (kebab-case, lowercase, ASCII). Output is NDJSON: one JSON object per file,\n  \
    followed by a final summary line with counts. Per-file errors are reported\n  \
    inline and processing continues unless --fail-fast is set.")]
pub struct IngestArgs {
    /// Directory containing files to ingest.
    #[arg(
        value_name = "DIR",
        help = "Directory to ingest recursively (each matching file becomes a memory)"
    )]
    pub dir: PathBuf,

    /// Memory type stored in `memories.type` for every ingested file.
    #[arg(long, value_enum)]
    pub r#type: MemoryType,

    /// Glob pattern matched against file basenames (default: `*.md`). Supports
    /// `*.<ext>`, `<prefix>*`, and exact filename match.
    #[arg(long, default_value = "*.md")]
    pub pattern: String,

    /// Recurse into subdirectories.
    #[arg(long, default_value_t = false)]
    pub recursive: bool,

    /// Disable automatic BERT NER entity/relationship extraction (faster bulk import).
    #[arg(long, default_value_t = false)]
    pub skip_extraction: bool,

    /// Stop on first per-file error instead of continuing with the next file.
    #[arg(long, default_value_t = false)]
    pub fail_fast: bool,

    /// Maximum number of files to ingest (safety cap to prevent runaway ingestion).
    #[arg(long, default_value_t = 10_000)]
    pub max_files: usize,

    /// Namespace for the ingested memories.
    #[arg(long)]
    pub namespace: Option<String>,

    /// Database path. Falls back to `SQLITE_GRAPHRAG_DB_PATH`, then `./graphrag.sqlite`.
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,

    #[arg(long, value_enum, default_value_t = JsonOutputFormat::Json)]
    pub format: JsonOutputFormat,

    #[arg(long, hide = true, help = "No-op; JSON is always emitted on stdout")]
    pub json: bool,
}

#[derive(Serialize)]
struct IngestFileEvent<'a> {
    file: &'a str,
    name: &'a str,
    status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    memory_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<String>,
}

#[derive(Serialize)]
struct IngestSummary {
    summary: bool,
    dir: String,
    pattern: String,
    recursive: bool,
    files_total: usize,
    files_succeeded: usize,
    files_failed: usize,
    files_skipped: usize,
    elapsed_ms: u64,
}

/// Outcome of a successful per-file ingest, used to build the NDJSON event.
struct FileSuccess {
    memory_id: i64,
    action: String,
}

pub fn run(args: IngestArgs) -> Result<(), AppError> {
    let started = std::time::Instant::now();

    if !args.dir.exists() {
        return Err(AppError::NotFound(format!(
            "directory not found: {}",
            args.dir.display()
        )));
    }
    if !args.dir.is_dir() {
        return Err(AppError::Validation(format!(
            "path is not a directory: {}",
            args.dir.display()
        )));
    }

    let mut files: Vec<PathBuf> = Vec::new();
    collect_files(&args.dir, &args.pattern, args.recursive, &mut files)?;
    files.sort();

    if files.len() > args.max_files {
        return Err(AppError::Validation(format!(
            "found {} files matching pattern, exceeds --max-files cap of {} (raise the cap or narrow the pattern)",
            files.len(),
            args.max_files
        )));
    }

    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let memory_type_str = args.r#type.as_str().to_string();

    // v1.0.32 Onda 4B: open the DB once and reuse the connection (and the
    // warm embedder via `crate::daemon::embed_passage_or_local`) across every
    // file, eliminating the ~17s ONNX cold-start that the previous
    // fork-spawn-per-file design paid on each iteration. We tolerate a startup
    // failure (e.g. an unwritable `--db` path) by capturing the error string
    // and surfacing it as a per-file failure event so callers preserve the
    // existing fail-fast / continue-on-error contract.
    let paths = AppPaths::resolve(args.db.as_deref())?;
    let mut conn_or_err = match init_storage(&paths) {
        Ok(c) => Ok(c),
        Err(e) => Err(format!("{e}")),
    };

    let mut succeeded: usize = 0;
    let mut failed: usize = 0;
    let mut skipped: usize = 0;
    let total = files.len();

    // v1.0.31 A10: track names produced during this run so two files with the
    // same kebab basename (after truncation, transliteration, etc.) get
    // distinct `-1`, `-2` suffixes within the same ingest invocation.
    // Cross-run collisions are intentionally left to the per-file persistence
    // path so re-ingesting an identical corpus still surfaces duplicates
    // instead of silently creating shadow copies.
    let mut taken_names: BTreeSet<String> = BTreeSet::new();

    for path in &files {
        let file_str = path.to_string_lossy().into_owned();
        let derived_base = derive_kebab_name(path);

        if derived_base.is_empty() {
            output::emit_json_compact(&IngestFileEvent {
                file: &file_str,
                name: "",
                status: "skipped",
                error: Some(
                    "could not derive a non-empty kebab-case name from filename".to_string(),
                ),
                memory_id: None,
                action: None,
            })?;
            skipped += 1;
            continue;
        }

        let derived_name = match unique_name(&derived_base, &taken_names) {
            Ok(n) => n,
            Err(e) => {
                output::emit_json_compact(&IngestFileEvent {
                    file: &file_str,
                    name: &derived_base,
                    status: "skipped",
                    error: Some(e.to_string()),
                    memory_id: None,
                    action: None,
                })?;
                skipped += 1;
                continue;
            }
        };
        taken_names.insert(derived_name.clone());

        // If startup failed, every file inherits the same fatal error rather
        // than silently succeeding against a non-existent database.
        let conn = match conn_or_err.as_mut() {
            Ok(c) => c,
            Err(err_msg) => {
                let err_clone = err_msg.clone();
                output::emit_json_compact(&IngestFileEvent {
                    file: &file_str,
                    name: &derived_name,
                    status: "failed",
                    error: Some(err_clone.clone()),
                    memory_id: None,
                    action: None,
                })?;
                failed += 1;
                if args.fail_fast {
                    output::emit_json_compact(&IngestSummary {
                        summary: true,
                        dir: args.dir.display().to_string(),
                        pattern: args.pattern.clone(),
                        recursive: args.recursive,
                        files_total: total,
                        files_succeeded: succeeded,
                        files_failed: failed,
                        files_skipped: skipped,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    })?;
                    return Err(AppError::Validation(format!(
                        "ingest aborted on first failure: {err_clone}"
                    )));
                }
                continue;
            }
        };

        let outcome = process_file(
            conn,
            &paths,
            &namespace,
            &memory_type_str,
            args.skip_extraction,
            path,
            &derived_name,
        );

        match outcome {
            Ok(FileSuccess { memory_id, action }) => {
                output::emit_json_compact(&IngestFileEvent {
                    file: &file_str,
                    name: &derived_name,
                    status: "indexed",
                    error: None,
                    memory_id: Some(memory_id),
                    action: Some(action),
                })?;
                succeeded += 1;
            }
            Err(e) => {
                let err_msg = format!("{e}");
                output::emit_json_compact(&IngestFileEvent {
                    file: &file_str,
                    name: &derived_name,
                    status: "failed",
                    error: Some(err_msg.clone()),
                    memory_id: None,
                    action: None,
                })?;
                failed += 1;
                if args.fail_fast {
                    output::emit_json_compact(&IngestSummary {
                        summary: true,
                        dir: args.dir.display().to_string(),
                        pattern: args.pattern.clone(),
                        recursive: args.recursive,
                        files_total: total,
                        files_succeeded: succeeded,
                        files_failed: failed,
                        files_skipped: skipped,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    })?;
                    return Err(AppError::Validation(format!(
                        "ingest aborted on first failure: {err_msg}"
                    )));
                }
            }
        }
    }

    output::emit_json_compact(&IngestSummary {
        summary: true,
        dir: args.dir.display().to_string(),
        pattern: args.pattern.clone(),
        recursive: args.recursive,
        files_total: total,
        files_succeeded: succeeded,
        files_failed: failed,
        files_skipped: skipped,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })?;

    Ok(())
}

/// Auto-initialises the database (matches the contract of every other CRUD
/// handler) and returns a fresh read/write connection ready for the ingest
/// loop. Errors here are recoverable per-file: the caller surfaces them as
/// failure events so `--fail-fast` and the continue-on-error path keep
/// working when, for example, the user points `--db` at an unwritable path.
fn init_storage(paths: &AppPaths) -> Result<Connection, AppError> {
    ensure_db_ready(paths)?;
    let conn = open_rw(&paths.db)?;
    Ok(conn)
}

/// In-process equivalent of `remember::run` for a single file. Mirrors the
/// canonical pipeline: read body, validate length, chunk, embed via the
/// daemon-or-local fallback (the warm embedder is reused across every file),
/// optionally extract entities, then persist memory + chunks + entities +
/// URLs in a single immediate transaction.
#[allow(clippy::too_many_arguments)]
fn process_file(
    conn: &mut Connection,
    paths: &AppPaths,
    namespace: &str,
    memory_type: &str,
    skip_extraction: bool,
    path: &Path,
    name: &str,
) -> Result<FileSuccess, AppError> {
    use crate::constants::*;

    if name.len() > MAX_MEMORY_NAME_LEN {
        return Err(AppError::LimitExceeded(
            crate::i18n::validation::name_length(MAX_MEMORY_NAME_LEN),
        ));
    }
    if name.starts_with("__") {
        return Err(AppError::Validation(
            crate::i18n::validation::reserved_name(),
        ));
    }
    {
        let slug_re = regex::Regex::new(NAME_SLUG_REGEX)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("regex: {e}")))?;
        if !slug_re.is_match(name) {
            return Err(AppError::Validation(crate::i18n::validation::name_kebab(
                name,
            )));
        }
    }

    let raw_body = std::fs::read_to_string(path).map_err(AppError::Io)?;
    if raw_body.len() > MAX_MEMORY_BODY_LEN {
        return Err(AppError::LimitExceeded(
            crate::i18n::validation::body_exceeds(MAX_MEMORY_BODY_LEN),
        ));
    }
    if raw_body.trim().is_empty() {
        return Err(AppError::Validation(crate::i18n::validation::empty_body()));
    }

    let description = format!("ingested from {}", path.display());
    if description.len() > MAX_MEMORY_DESCRIPTION_LEN {
        return Err(AppError::Validation(
            crate::i18n::validation::description_exceeds(MAX_MEMORY_DESCRIPTION_LEN),
        ));
    }

    // Auto-extraction is best-effort — failures degrade gracefully like in
    // `remember::run`. With `--skip-extraction` we bypass the BERT NER cost
    // entirely (the chunking + embedding cost is independent).
    let mut extracted_entities: Vec<NewEntity> = Vec::new();
    let mut extracted_relationships: Vec<NewRelationship> = Vec::new();
    let mut extracted_urls: Vec<crate::extraction::ExtractedUrl> = Vec::new();
    let mut relationships_truncated = false;
    if !skip_extraction {
        match crate::extraction::extract_graph_auto(&raw_body, paths) {
            Ok(extracted) => {
                extracted_urls = extracted.urls;
                extracted_entities = extracted.entities;
                extracted_relationships = extracted.relationships;
                relationships_truncated = extracted.relationships_truncated;

                if extracted_entities.len() > MAX_ENTITIES_PER_MEMORY {
                    extracted_entities.truncate(MAX_ENTITIES_PER_MEMORY);
                }
                if extracted_relationships.len() > MAX_RELATIONSHIPS_PER_MEMORY {
                    relationships_truncated = true;
                    extracted_relationships.truncate(MAX_RELATIONSHIPS_PER_MEMORY);
                }
            }
            Err(e) => {
                tracing::warn!(
                    file = %path.display(),
                    "auto-extraction failed (graceful degradation): {e:#}"
                );
            }
        }
    }

    // Validate extracted graph types/relations to match `remember::run` rules.
    for entity in &extracted_entities {
        if !is_valid_entity_type(&entity.entity_type) {
            return Err(AppError::Validation(format!(
                "invalid entity_type '{}' for entity '{}'",
                entity.entity_type, entity.name
            )));
        }
    }
    for rel in &mut extracted_relationships {
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

    let body_hash = blake3::hash(raw_body.as_bytes()).to_hex().to_string();
    let snippet: String = raw_body.chars().take(200).collect();

    let tokenizer = crate::tokenizer::get_tokenizer(&paths.models)?;
    let chunks_info = chunking::split_into_chunks_hierarchical(&raw_body, tokenizer);
    if chunks_info.len() > REMEMBER_MAX_SAFE_MULTI_CHUNKS {
        return Err(AppError::LimitExceeded(format!(
            "document produces {} chunks; current safe operational limit is {} chunks; split the document before using remember",
            chunks_info.len(),
            REMEMBER_MAX_SAFE_MULTI_CHUNKS
        )));
    }

    // Reuse the warm embedder (daemon when available, in-process otherwise).
    // This is the load-bearing change of Onda 4B: the model is loaded ONCE
    // for the whole ingest run, not once per file.
    let mut chunk_embeddings_cache: Option<Vec<Vec<f32>>> = None;
    let embedding = if chunks_info.len() == 1 {
        crate::daemon::embed_passage_or_local(&paths.models, &raw_body)?
    } else {
        let chunk_texts: Vec<&str> = chunks_info
            .iter()
            .map(|c| chunking::chunk_text(&raw_body, c))
            .collect();
        let mut chunk_embeddings = Vec::with_capacity(chunk_texts.len());
        for chunk_text in &chunk_texts {
            chunk_embeddings.push(crate::daemon::embed_passage_or_local(
                &paths.models,
                chunk_text,
            )?);
        }
        let aggregated = chunking::aggregate_embeddings(&chunk_embeddings);
        chunk_embeddings_cache = Some(chunk_embeddings);
        aggregated
    };

    // Namespace bookkeeping (mirrors remember::run): reject when active
    // namespaces already hit the cap and this file would create a new one.
    {
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
                "active namespace limit of {MAX_NAMESPACES_ACTIVE} exceeded while creating '{namespace}'"
            )));
        }
    }

    let existing_memory = memories::find_by_name(conn, namespace, name)?;
    if existing_memory.is_some() {
        // Ingest does not implement merge semantics; surface the duplicate as
        // a per-file failure so the caller can decide whether to remove the
        // existing memory or rename the source file.
        return Err(AppError::Duplicate(errors_msg::duplicate_memory(
            name, namespace,
        )));
    }
    let duplicate_hash_id = memories::find_by_hash(conn, namespace, &body_hash)?;

    let new_memory = NewMemory {
        namespace: namespace.to_string(),
        name: name.to_string(),
        memory_type: memory_type.to_string(),
        description: description.clone(),
        body: raw_body,
        body_hash: body_hash.clone(),
        session_id: None,
        source: "agent".to_string(),
        metadata: serde_json::json!({}),
    };

    // Pre-compute entity embeddings BEFORE the transaction so the embedder
    // (and any daemon socket) is touched outside the immediate write lock.
    let graph_entity_embeddings = extracted_entities
        .iter()
        .map(|entity| {
            let entity_text = match &entity.description {
                Some(desc) => format!("{} {}", entity.name, desc),
                None => entity.name.clone(),
            };
            crate::daemon::embed_passage_or_local(&paths.models, &entity_text)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let _ = relationships_truncated; // not surfaced in the per-file event today

    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

    if let Some(hash_id) = duplicate_hash_id {
        tracing::debug!(
            target: "ingest",
            duplicate_memory_id = hash_id,
            "identical body already exists; persisting a new memory anyway"
        );
    }

    let memory_id = memories::insert(&tx, &new_memory)?;
    versions::insert_version(
        &tx,
        memory_id,
        1,
        name,
        memory_type,
        &description,
        &new_memory.body,
        &serde_json::to_string(&new_memory.metadata)?,
        None,
        "create",
    )?;
    memories::upsert_vec(
        &tx,
        memory_id,
        namespace,
        memory_type,
        &embedding,
        name,
        &snippet,
    )?;

    if chunks_info.len() > 1 {
        storage_chunks::insert_chunk_slices(&tx, memory_id, &new_memory.body, &chunks_info)?;
        let chunk_embeddings = chunk_embeddings_cache.take().ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!(
                "missing chunk embeddings cache on multi-chunk ingest path"
            ))
        })?;
        for (i, emb) in chunk_embeddings.iter().enumerate() {
            storage_chunks::upsert_chunk_vec(&tx, i as i64, memory_id, i as i32, emb)?;
        }
    }

    if !extracted_entities.is_empty() || !extracted_relationships.is_empty() {
        for (idx, entity) in extracted_entities.iter().enumerate() {
            let entity_id = entities::upsert_entity(&tx, namespace, entity)?;
            let entity_embedding = &graph_entity_embeddings[idx];
            entities::upsert_entity_vec(
                &tx,
                entity_id,
                namespace,
                &entity.entity_type,
                entity_embedding,
                &entity.name,
            )?;
            entities::link_memory_entity(&tx, memory_id, entity_id)?;
            entities::increment_degree(&tx, entity_id)?;
        }
        let entity_types: std::collections::HashMap<&str, &str> = extracted_entities
            .iter()
            .map(|entity| (entity.name.as_str(), entity.entity_type.as_str()))
            .collect();
        for rel in &extracted_relationships {
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
            let source_id = entities::upsert_entity(&tx, namespace, &source_entity)?;
            let target_id = entities::upsert_entity(&tx, namespace, &target_entity)?;
            let rel_id = entities::upsert_relationship(&tx, namespace, source_id, target_id, rel)?;
            entities::link_memory_relationship(&tx, memory_id, rel_id)?;
        }
    }

    tx.commit()?;

    // URLs persistence is non-critical (failures don't propagate) and lives
    // outside the main transaction to mirror `remember::run` semantics.
    if !extracted_urls.is_empty() {
        let url_entries: Vec<storage_urls::MemoryUrl> = extracted_urls
            .into_iter()
            .map(|u| storage_urls::MemoryUrl {
                url: u.url,
                offset: Some(u.offset as i64),
            })
            .collect();
        let _ = storage_urls::insert_urls(conn, memory_id, &url_entries);
    }

    Ok(FileSuccess {
        memory_id,
        action: "created".to_string(),
    })
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

fn collect_files(
    dir: &Path,
    pattern: &str,
    recursive: bool,
    out: &mut Vec<PathBuf>,
) -> Result<(), AppError> {
    let entries = std::fs::read_dir(dir).map_err(AppError::Io)?;
    for entry in entries {
        let entry = entry.map_err(AppError::Io)?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(AppError::Io)?;
        if file_type.is_file() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if matches_pattern(&name_str, pattern) {
                out.push(path);
            }
        } else if file_type.is_dir() && recursive {
            collect_files(&path, pattern, recursive, out)?;
        }
    }
    Ok(())
}

fn matches_pattern(name: &str, pattern: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix('*') {
        name.ends_with(suffix)
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        name.starts_with(prefix)
    } else {
        name == pattern
    }
}

fn derive_kebab_name(path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let lowered: String = stem
        .chars()
        .map(|c| {
            if c == '_' || c.is_whitespace() {
                '-'
            } else {
                c
            }
        })
        .map(|c| c.to_ascii_lowercase())
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
        .collect();
    let collapsed = collapse_dashes(&lowered);
    let trimmed = collapsed.trim_matches('-').to_string();
    if trimmed.len() > DERIVED_NAME_MAX_LEN {
        let truncated = trimmed[..DERIVED_NAME_MAX_LEN]
            .trim_matches('-')
            .to_string();
        // v1.0.31 A10: surface the truncation so users can fix overly long file
        // basenames before they collide with siblings sharing the same prefix.
        tracing::warn!(
            target: "ingest",
            original = %trimmed,
            truncated_to = %truncated,
            max_len = DERIVED_NAME_MAX_LEN,
            "derived memory name truncated to fit length cap; collisions will be resolved with numeric suffixes"
        );
        truncated
    } else {
        trimmed
    }
}

/// v1.0.31 A10: returns the first non-colliding kebab name by appending a
/// numeric suffix (`-1`, `-2`, …) when needed.
///
/// `taken` is the set of names already consumed in the current ingest run.
/// The caller is expected to insert the returned name into `taken` so the
/// next call observes the consumption. Cross-run collisions are intentionally
/// surfaced by the per-file persistence path as duplicates so re-ingestion
/// of identical corpora stays idempotent.
///
/// Returns `Err(AppError::Validation)` after `MAX_NAME_COLLISION_SUFFIX`
/// candidates collide, signalling a pathological corpus that should be
/// renamed manually.
fn unique_name(base: &str, taken: &BTreeSet<String>) -> Result<String, AppError> {
    if !taken.contains(base) {
        return Ok(base.to_string());
    }
    for suffix in 1..=MAX_NAME_COLLISION_SUFFIX {
        let candidate = format!("{base}-{suffix}");
        if !taken.contains(&candidate) {
            tracing::warn!(
                target: "ingest",
                base = %base,
                resolved = %candidate,
                suffix,
                "memory name collision resolved with numeric suffix"
            );
            return Ok(candidate);
        }
    }
    Err(AppError::Validation(format!(
        "too many name collisions for base '{base}' (>{MAX_NAME_COLLISION_SUFFIX}); rename source files to disambiguate"
    )))
}

fn collapse_dashes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for c in s.chars() {
        if c == '-' {
            if !prev_dash {
                out.push('-');
            }
            prev_dash = true;
        } else {
            out.push(c);
            prev_dash = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn matches_pattern_suffix() {
        assert!(matches_pattern("foo.md", "*.md"));
        assert!(!matches_pattern("foo.txt", "*.md"));
        assert!(matches_pattern("foo.md", "*"));
    }

    #[test]
    fn matches_pattern_prefix() {
        assert!(matches_pattern("README.md", "README*"));
        assert!(!matches_pattern("CHANGELOG.md", "README*"));
    }

    #[test]
    fn matches_pattern_exact() {
        assert!(matches_pattern("README.md", "README.md"));
        assert!(!matches_pattern("readme.md", "README.md"));
    }

    #[test]
    fn derive_kebab_underscore_to_dash() {
        let p = PathBuf::from("/tmp/claude_code_headless.md");
        assert_eq!(derive_kebab_name(&p), "claude-code-headless");
    }

    #[test]
    fn derive_kebab_uppercase_lowered() {
        let p = PathBuf::from("/tmp/README.md");
        assert_eq!(derive_kebab_name(&p), "readme");
    }

    #[test]
    fn derive_kebab_strips_non_kebab_chars() {
        let p = PathBuf::from("/tmp/some@weird#name!.md");
        assert_eq!(derive_kebab_name(&p), "someweirdname");
    }

    #[test]
    fn derive_kebab_collapses_consecutive_dashes() {
        let p = PathBuf::from("/tmp/a__b___c.md");
        assert_eq!(derive_kebab_name(&p), "a-b-c");
    }

    #[test]
    fn derive_kebab_truncates_to_60_chars() {
        let p = PathBuf::from(format!("/tmp/{}.md", "a".repeat(80)));
        let name = derive_kebab_name(&p);
        assert!(name.len() <= 60, "got len {}", name.len());
    }

    #[test]
    fn collect_files_finds_md_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("a.md"), "x").unwrap();
        std::fs::write(tmp.path().join("b.md"), "y").unwrap();
        std::fs::write(tmp.path().join("c.txt"), "z").unwrap();
        let mut out = Vec::new();
        collect_files(tmp.path(), "*.md", false, &mut out).expect("collect");
        assert_eq!(out.len(), 2, "should find 2 .md files, got {out:?}");
    }

    #[test]
    fn collect_files_recursive_descends_subdirs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let sub = tmp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(tmp.path().join("a.md"), "x").unwrap();
        std::fs::write(sub.join("b.md"), "y").unwrap();
        let mut out = Vec::new();
        collect_files(tmp.path(), "*.md", true, &mut out).expect("collect");
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn collect_files_non_recursive_skips_subdirs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let sub = tmp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(tmp.path().join("a.md"), "x").unwrap();
        std::fs::write(sub.join("b.md"), "y").unwrap();
        let mut out = Vec::new();
        collect_files(tmp.path(), "*.md", false, &mut out).expect("collect");
        assert_eq!(out.len(), 1);
    }

    // ── v1.0.31 A10: name truncation warns and collisions are auto-resolved ──

    #[test]
    fn derive_kebab_long_basename_truncated_within_cap() {
        let p = PathBuf::from(format!("/tmp/{}.md", "a".repeat(120)));
        let name = derive_kebab_name(&p);
        assert!(
            name.len() <= DERIVED_NAME_MAX_LEN,
            "truncated name must respect cap; got {} chars",
            name.len()
        );
        assert!(!name.is_empty());
    }

    #[test]
    fn unique_name_returns_base_when_free() {
        let taken: BTreeSet<String> = BTreeSet::new();
        let resolved = unique_name("note", &taken).expect("must resolve");
        assert_eq!(resolved, "note");
    }

    #[test]
    fn unique_name_appends_first_free_suffix_on_collision() {
        let mut taken: BTreeSet<String> = BTreeSet::new();
        taken.insert("note".to_string());
        taken.insert("note-1".to_string());
        let resolved = unique_name("note", &taken).expect("must resolve");
        assert_eq!(resolved, "note-2");
    }

    #[test]
    fn unique_name_errors_after_collision_cap() {
        let mut taken: BTreeSet<String> = BTreeSet::new();
        taken.insert("note".to_string());
        for i in 1..=MAX_NAME_COLLISION_SUFFIX {
            taken.insert(format!("note-{i}"));
        }
        let err = unique_name("note", &taken).expect_err("must surface error");
        assert!(matches!(err, AppError::Validation(_)));
    }

    // ── v1.0.32 Onda 4B: in-process pipeline validation ──

    #[test]
    fn is_valid_entity_type_accepts_v008_types() {
        assert!(is_valid_entity_type("organization"));
        assert!(is_valid_entity_type("location"));
        assert!(is_valid_entity_type("date"));
        assert!(!is_valid_entity_type("unknown"));
    }

    #[test]
    fn is_valid_relation_accepts_canonical_relations() {
        assert!(is_valid_relation("applies_to"));
        assert!(is_valid_relation("depends_on"));
        assert!(!is_valid_relation("foo_bar"));
    }
}
