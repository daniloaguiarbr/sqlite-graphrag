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
//!
//! ## Incremental pipeline (v1.0.43)
//!
//! Phase A runs on a rayon thread pool (size = `--ingest-parallelism`):
//! read + chunk + embed + NER per file. Results are sent immediately via a
//! bounded `mpsc::sync_channel` to Phase B so persistence starts as soon
//! as the first file completes — no waiting for all files to finish Phase A.
//!
//! Phase B runs on the main thread: receives staged files from the channel,
//! writes to SQLite per-file (WAL absorbs individual commits), and emits
//! NDJSON progress events to stderr as each file is persisted. `Connection`
//! is not `Sync` so it never crosses thread boundaries.
//!
//! This fixes B1: with the old 2-phase design, a 50-file corpus with 27s/file
//! NER would spend ~22min in Phase A alone, exceeding the user's 900s timeout
//! before Phase B (and any DB writes) could begin. With this pipeline, the
//! first file is committed within seconds of starting.

use crate::chunking;
use crate::cli::MemoryType;
use crate::entity_type::EntityType;
use crate::errors::AppError;
use crate::i18n::errors_msg;
use crate::output::{self, JsonOutputFormat};
use crate::paths::AppPaths;
use crate::storage::chunks as storage_chunks;
use crate::storage::connection::{ensure_db_ready, open_rw};
use crate::storage::entities::{NewEntity, NewRelationship};
use crate::storage::memories::NewMemory;
use crate::storage::{entities, memories, urls as storage_urls, versions};
use rayon::prelude::*;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use unicode_normalization::UnicodeNormalization;

use crate::constants::DERIVED_NAME_MAX_LEN;

/// Hard cap on the numeric suffix appended for collision resolution. If 1000
/// candidates collide we surface an error rather than loop forever.
const MAX_NAME_COLLISION_SUFFIX: usize = 1000;

#[derive(clap::Args)]
#[command(after_long_help = "EXAMPLES:\n  \
    # Ingest every Markdown file under ./docs as `document` memories\n  \
    sqlite-graphrag ingest ./docs --type document\n\n  \
    # Ingest .txt files recursively under ./notes\n  \
    sqlite-graphrag ingest ./notes --type note --pattern '*.txt' --recursive\n\n  \
    # Enable GLiNER NER extraction (disabled by default, slower)\n  \
    sqlite-graphrag ingest ./big-corpus --type reference --enable-ner\n\n  \
    # Preview file-to-name mapping without ingesting\n  \
    sqlite-graphrag ingest ./docs --dry-run\n\n  \
    # LLM-curated extraction via Claude Code CLI\n  \
    sqlite-graphrag ingest ./docs --mode claude-code --recursive --json\n\n  \
    # Resume interrupted claude-code ingest\n  \
    sqlite-graphrag ingest ./docs --mode claude-code --resume --json\n\n  \
    # Claude Code with budget cap and custom timeout\n  \
    sqlite-graphrag ingest ./docs --mode claude-code --max-cost-usd 5.00 --claude-timeout 600 --json\n\n  \
AUTHENTICATION:\n  \
    --mode claude-code: Uses existing Claude Code authentication.\n  \
      OAuth (Pro/Max/Team): works automatically from ~/.claude/.credentials.json\n  \
      API key: set ANTHROPIC_API_KEY for faster startup (optional)\n\n  \
    --mode codex: Uses existing Codex CLI authentication.\n  \
      Device auth: run `codex auth login` first\n  \
      API key: set OPENAI_API_KEY (optional)\n\n  \
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

    /// Memory type stored in `memories.type` for every ingested file. Defaults to `document`.
    #[arg(long, value_enum, default_value_t = MemoryType::Document)]
    pub r#type: MemoryType,

    /// Glob pattern matched against file basenames (default: `*.md`). Supports
    /// `*.<ext>`, `<prefix>*`, and exact filename match.
    #[arg(long, default_value = "*.md")]
    pub pattern: String,

    /// Recurse into subdirectories.
    #[arg(long, default_value_t = false)]
    pub recursive: bool,

    #[arg(
        long,
        env = "SQLITE_GRAPHRAG_ENABLE_NER",
        value_parser = crate::parsers::parse_bool_flexible,
        action = clap::ArgAction::Set,
        num_args = 0..=1,
        default_missing_value = "true",
        default_value = "false",
        help = "Enable automatic GLiNER NER entity/relationship extraction (disabled by default)"
    )]
    pub enable_ner: bool,
    #[arg(
        long,
        env = "SQLITE_GRAPHRAG_GLINER_VARIANT",
        default_value = "fp32",
        help = "GLiNER model variant: fp32 (1.1GB, best quality), fp16 (580MB), int8 (349MB, fastest but may miss entities on short texts), q4, q4f16"
    )]
    pub gliner_variant: String,

    /// Deprecated: NER is now disabled by default. Kept for backwards compatibility.
    #[arg(long, default_value_t = false, hide = true)]
    pub skip_extraction: bool,

    /// Stop on first per-file error instead of continuing with the next file.
    #[arg(long, default_value_t = false)]
    pub fail_fast: bool,

    /// Preview file-to-name mapping without loading model or persisting.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

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

    /// Number of files to extract+embed in parallel; default = max(1, cpus/2).min(4).
    #[arg(
        long,
        help = "Number of files to extract+embed in parallel; default = max(1, cpus/2).min(4)"
    )]
    pub ingest_parallelism: Option<usize>,

    /// Force single-threaded ingest to reduce RSS pressure.
    ///
    /// Equivalent to `--ingest-parallelism 1`, takes precedence over any
    /// explicit value. Recommended for environments with <4 GB available
    /// RAM or container/cgroup constraints. Trade-off: 3-4x longer wall
    /// time. Also honored via `SQLITE_GRAPHRAG_LOW_MEMORY=1` env var
    /// (CLI flag has higher precedence than the env var).
    #[arg(
        long,
        default_value_t = false,
        help = "Forces single-threaded ingest (--ingest-parallelism 1) to reduce RSS pressure. \
                Recommended for environments with <4 GB available RAM or container/cgroup \
                constraints. Trade-off: 3-4x longer wall time. Also honored via \
                SQLITE_GRAPHRAG_LOW_MEMORY=1 env var."
    )]
    pub low_memory: bool,

    /// Maximum process RSS in MiB; abort if exceeded during embedding.
    #[arg(long, default_value_t = crate::constants::DEFAULT_MAX_RSS_MB,
          help = "Maximum process RSS in MiB; abort if exceeded during embedding (default: 8192)")]
    pub max_rss_mb: u64,

    /// Maximum character length for derived memory names from file basenames.
    ///
    /// Overrides the compile-time `DERIVED_NAME_MAX_LEN` constant (default 60).
    /// Shorter values leave more headroom for collision suffix resolution.
    #[arg(long, default_value_t = crate::constants::DERIVED_NAME_MAX_LEN,
          help = "Maximum length for derived memory names (default: 60)")]
    pub max_name_length: usize,

    /// Extraction mode: `none` (body-only, default), `gliner` (NER), or `claude-code` (LLM-curated via Claude Code CLI).
    #[arg(long, value_enum, default_value_t = IngestMode::None)]
    pub mode: IngestMode,

    /// Explicit path to the Claude Code binary (only with --mode claude-code).
    #[arg(long, env = "SQLITE_GRAPHRAG_CLAUDE_BINARY")]
    pub claude_binary: Option<std::path::PathBuf>,

    /// Model override for Claude Code extraction (e.g. claude-sonnet-4-6).
    #[arg(long)]
    pub claude_model: Option<String>,

    /// Resume a previously interrupted claude-code ingest from the queue DB.
    #[arg(long, default_value_t = false)]
    pub resume: bool,

    /// Retry only failed files from a previous claude-code ingest.
    #[arg(long, default_value_t = false)]
    pub retry_failed: bool,

    /// Keep the queue DB (.ingest-queue.sqlite) after completion.
    #[arg(long, default_value_t = false)]
    pub keep_queue: bool,

    /// Custom path for the claude-code ingest queue database.
    #[arg(long, default_value = ".ingest-queue.sqlite")]
    pub queue_db: String,

    /// Initial wait time in seconds when rate-limited (only with --mode claude-code).
    #[arg(long, default_value_t = 60)]
    pub rate_limit_wait: u64,

    /// Maximum cumulative cost in USD before aborting (only with --mode claude-code).
    #[arg(long)]
    pub max_cost_usd: Option<f64>,

    /// Timeout in seconds for each claude -p invocation (only with --mode claude-code).
    #[arg(
        long,
        default_value_t = 300,
        help = "Timeout in seconds for each claude -p invocation (default: 300)"
    )]
    pub claude_timeout: u64,

    /// Explicit path to the Codex CLI binary (only with --mode codex).
    #[arg(
        long,
        env = "SQLITE_GRAPHRAG_CODEX_BINARY",
        help = "Explicit path to the Codex CLI binary (only with --mode codex)"
    )]
    pub codex_binary: Option<PathBuf>,

    /// Model override for Codex extraction (e.g. o4-mini, gpt-5.1-codex).
    #[arg(
        long,
        help = "Model override for Codex extraction (e.g. o4-mini, gpt-5.1-codex)"
    )]
    pub codex_model: Option<String>,

    /// Timeout in seconds for each codex exec invocation.
    #[arg(
        long,
        default_value_t = 300,
        help = "Timeout in seconds for each codex exec invocation (default: 300)"
    )]
    pub codex_timeout: u64,

    /// G30: poll for the job singleton every second for up to N seconds
    /// when another invocation holds the lock. Default: 0 (fail fast).
    #[arg(long, value_name = "SECONDS")]
    pub wait_job_singleton: Option<u64>,

    /// G30: force acquisition of the singleton lock by removing a stale
    /// lock file from a previously crashed invocation.
    #[arg(long, default_value_t = false)]
    pub force_job_singleton: bool,
}

/// Extraction mode for the ingest pipeline.
#[derive(Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum IngestMode {
    /// Body-only ingestion without entity/relationship extraction (default).
    None,
    /// GLiNER zero-shot NER extraction (requires --enable-ner).
    Gliner,
    /// LLM-curated extraction via locally installed Claude Code CLI.
    ClaudeCode,
    /// LLM-curated extraction via locally installed OpenAI Codex CLI.
    Codex,
}

/// Returns true when the `SQLITE_GRAPHRAG_LOW_MEMORY` env var is set to a
/// truthy value (`1`, `true`, `yes`, `on`, case-insensitive). Empty or unset
/// values evaluate to false. Unrecognized non-empty values emit a
/// `tracing::warn!` and evaluate to false.
fn env_low_memory_enabled() -> bool {
    match std::env::var("SQLITE_GRAPHRAG_LOW_MEMORY") {
        Ok(v) if v.is_empty() => false,
        Ok(v) => match v.to_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            other => {
                tracing::warn!(
                    target: "ingest",
                    value = %other,
                    "SQLITE_GRAPHRAG_LOW_MEMORY value not recognized; treating as disabled"
                );
                false
            }
        },
        Err(_) => false,
    }
}

/// Resolves the effective ingest parallelism honoring `--low-memory` and the
/// `SQLITE_GRAPHRAG_LOW_MEMORY` env var.
///
/// Precedence:
/// 1. `--low-memory` CLI flag forces parallelism = 1.
/// 2. `SQLITE_GRAPHRAG_LOW_MEMORY=1` env var forces parallelism = 1.
/// 3. Explicit `--ingest-parallelism N` (when low-memory is off).
/// 4. Default heuristic `(cpus/2).clamp(1, 4)`.
///
/// When low-memory wins and the user also passed `--ingest-parallelism N>1`,
/// emits a `tracing::warn!` advertising the override.
fn resolve_parallelism(low_memory_flag: bool, ingest_parallelism: Option<usize>) -> usize {
    let env_flag = env_low_memory_enabled();
    let low_memory = low_memory_flag || env_flag;

    if low_memory {
        if let Some(n) = ingest_parallelism {
            if n > 1 {
                tracing::warn!(
                    target: "ingest",
                    requested = n,
                    "--ingest-parallelism overridden by --low-memory; using 1"
                );
            }
        }
        if low_memory_flag {
            tracing::info!(
                target: "ingest",
                source = "flag",
                "low-memory mode enabled: forcing --ingest-parallelism 1"
            );
        } else {
            tracing::info!(
                target: "ingest",
                source = "env",
                "low-memory mode enabled via SQLITE_GRAPHRAG_LOW_MEMORY: forcing --ingest-parallelism 1"
            );
        }
        return 1;
    }

    ingest_parallelism
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|v| v.get() / 2)
                .unwrap_or(1)
                .clamp(1, 4)
        })
        .max(1)
}

#[derive(Serialize)]
struct IngestFileEvent<'a> {
    file: &'a str,
    name: &'a str,
    status: &'a str,
    /// True when the derived name was truncated to fit `DERIVED_NAME_MAX_LEN`. False otherwise.
    truncated: bool,
    /// Original derived name before truncation; only present when `truncated=true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    original_name: Option<String>,
    /// Original file basename (without extension); only present when it differs from `name`.
    #[serde(skip_serializing_if = "Option::is_none")]
    original_filename: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    memory_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<String>,
    /// Byte length of the body ingested; 0 when not yet read (e.g. skip or dry-run events).
    body_length: usize,
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
    body_length: usize,
}

/// NDJSON progress event emitted to stderr after each file completes Phase A.
/// Schema version 1; consumers should check `schema_version` before parsing.
#[derive(Serialize)]
struct StageProgressEvent<'a> {
    schema_version: u8,
    event: &'a str,
    path: &'a str,
    ms: u64,
    entities: usize,
    relationships: usize,
}

/// All artefacts pre-computed by Phase A (CPU-bound, runs on rayon thread pool).
/// Phase B persists these to SQLite on the main thread in submission order.
struct StagedFile {
    body: String,
    body_hash: String,
    snippet: String,
    name: String,
    description: String,
    embedding: Vec<f32>,
    chunk_embeddings: Option<Vec<Vec<f32>>>,
    chunks_info: Vec<crate::chunking::Chunk>,
    entities: Vec<NewEntity>,
    relationships: Vec<NewRelationship>,
    entity_embeddings: Vec<Vec<f32>>,
    urls: Vec<crate::extraction::ExtractedUrl>,
}

/// Phase A worker: reads, chunks, embeds and extracts NER for one file.
/// Never touches the database — safe to run on any rayon thread.
fn stage_file(
    _idx: usize,
    path: &Path,
    name: &str,
    paths: &AppPaths,
    enable_ner: bool,
    gliner_variant: crate::extraction::GlinerVariant,
    max_rss_mb: u64,
) -> Result<StagedFile, AppError> {
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
        let slug_re = crate::constants::name_slug_regex();
        if !slug_re.is_match(name) {
            return Err(AppError::Validation(crate::i18n::validation::name_kebab(
                name,
            )));
        }
    }

    let file_size = std::fs::metadata(path).map_err(AppError::Io)?.len();
    if file_size > MAX_MEMORY_BODY_LEN as u64 {
        return Err(AppError::LimitExceeded(
            crate::i18n::validation::body_exceeds(MAX_MEMORY_BODY_LEN),
        ));
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

    let mut extracted_entities: Vec<NewEntity> = Vec::with_capacity(30);
    let mut extracted_relationships: Vec<NewRelationship> = Vec::with_capacity(50);
    let mut extracted_urls: Vec<crate::extraction::ExtractedUrl> = Vec::with_capacity(4);
    if enable_ner {
        match crate::extraction::extract_graph_auto(&raw_body, paths, gliner_variant) {
            Ok(extracted) => {
                extracted_urls = extracted.urls;
                // v1.0.76: ExtractionResult.entities is now
                // Vec<ExtractedEntity>, not Vec<NewEntity>. Convert
                // via name + type only; start/end offsets are not
                // carried forward into the storage layer.
                extracted_entities = extracted
                    .entities
                    .into_iter()
                    .map(|e| NewEntity {
                        name: e.name,
                        entity_type: crate::entity_type::EntityType::Concept,
                        description: None,
                    })
                    .collect();
                // v1.0.76: relationships are no longer in the
                // ExtractionResult struct; the LLM backend returns
                // them in its own payload. The default build is
                // URL-only extraction.
                extracted_relationships.clear();

                if extracted_entities.len() > max_entities_per_memory() {
                    extracted_entities.truncate(max_entities_per_memory());
                }
                if extracted_relationships.len() > max_relationships_per_memory() {
                    extracted_relationships.truncate(max_relationships_per_memory());
                }
            }
            Err(e) => {
                tracing::warn!(
                    target: "ingest",
                    file = %path.display(),
                    "auto-extraction failed (graceful degradation): {e:#}"
                );
            }
        }
    }

    for rel in &mut extracted_relationships {
        rel.relation = crate::parsers::normalize_relation(&rel.relation);
        if let Err(e) = crate::parsers::validate_relation_format(&rel.relation) {
            return Err(AppError::Validation(format!(
                "{e} for relationship '{}' -> '{}'",
                rel.source, rel.target
            )));
        }
        crate::parsers::warn_if_non_canonical(&rel.relation);
        if !(0.0..=1.0).contains(&rel.strength) {
            return Err(AppError::Validation(format!(
                "invalid strength {} for relationship '{}' -> '{}'; expected value in [0.0, 1.0]",
                rel.strength, rel.source, rel.target
            )));
        }
    }

    let body_hash = blake3::hash(raw_body.as_bytes()).to_hex().to_string();
    let snippet: String = raw_body.chars().take(200).collect();

    let chunks_info = chunking::split_into_chunks_hierarchical(&raw_body);
    if chunks_info.len() > REMEMBER_MAX_SAFE_MULTI_CHUNKS {
        return Err(AppError::LimitExceeded(format!(
            "document produces {} chunks; current safe operational limit is {} chunks; split the document before using remember",
            chunks_info.len(),
            REMEMBER_MAX_SAFE_MULTI_CHUNKS
        )));
    }

    let mut chunk_embeddings_opt: Option<Vec<Vec<f32>>> = None;
    let embedding = if chunks_info.len() == 1 {
        crate::daemon::embed_passage_or_local(&paths.models, &raw_body)?
    } else {
        let chunk_texts: Vec<&str> = chunks_info
            .iter()
            .map(|c| chunking::chunk_text(&raw_body, c))
            .collect();
        let embed_cap = chunk_texts.len();
        let mut chunk_embeddings = Vec::new();
        chunk_embeddings.try_reserve(embed_cap).map_err(|_| {
            AppError::LimitExceeded(format!(
                "allocation of {embed_cap} chunk embeddings would exceed available memory"
            ))
        })?;
        for chunk_text in &chunk_texts {
            if let Some(rss) = crate::memory_guard::current_process_memory_mb() {
                if rss > max_rss_mb {
                    tracing::error!(
                        target: "ingest",
                        rss_mb = rss,
                        max_rss_mb = max_rss_mb,
                        file = %path.display(),
                        "RSS exceeded --max-rss-mb threshold; aborting to prevent system instability"
                    );
                    return Err(AppError::LowMemory {
                        available_mb: crate::memory_guard::available_memory_mb(),
                        required_mb: max_rss_mb,
                    });
                }
            }
            chunk_embeddings.push(crate::daemon::embed_passage_or_local(
                &paths.models,
                chunk_text,
            )?);
        }
        let aggregated = chunking::aggregate_embeddings(&chunk_embeddings);
        chunk_embeddings_opt = Some(chunk_embeddings);
        aggregated
    };

    let entity_embeddings = extracted_entities
        .iter()
        .map(|entity| {
            let entity_text = match &entity.description {
                Some(desc) => format!("{} {}", entity.name, desc),
                None => entity.name.clone(),
            };
            crate::daemon::embed_passage_or_local(&paths.models, &entity_text)
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(StagedFile {
        body: raw_body,
        body_hash,
        snippet,
        name: name.to_string(),
        description,
        embedding,
        chunk_embeddings: chunk_embeddings_opt,
        chunks_info,
        entities: extracted_entities,
        relationships: extracted_relationships,
        entity_embeddings,
        urls: extracted_urls,
    })
}

/// Phase B: persists one `StagedFile` to the database on the main thread.
fn persist_staged(
    conn: &mut Connection,
    namespace: &str,
    memory_type: &str,
    staged: StagedFile,
) -> Result<FileSuccess, AppError> {
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
        if !ns_exists && active_count >= crate::constants::MAX_NAMESPACES_ACTIVE {
            return Err(AppError::NamespaceError(format!(
                "active namespace limit of {} exceeded while creating '{namespace}'",
                crate::constants::MAX_NAMESPACES_ACTIVE
            )));
        }
    }

    let existing_memory = memories::find_by_name(conn, namespace, &staged.name)?;
    if existing_memory.is_some() {
        return Err(AppError::Duplicate(errors_msg::duplicate_memory(
            &staged.name,
            namespace,
        )));
    }
    let duplicate_hash_id = memories::find_by_hash(conn, namespace, &staged.body_hash)?;

    let new_memory = NewMemory {
        namespace: namespace.to_string(),
        name: staged.name.clone(),
        memory_type: memory_type.to_string(),
        description: staged.description.clone(),
        body: staged.body,
        body_hash: staged.body_hash,
        session_id: None,
        source: "agent".to_string(),
        metadata: serde_json::json!({}),
    };

    if let Some(hash_id) = duplicate_hash_id {
        tracing::debug!(
            target: "ingest",
            duplicate_memory_id = hash_id,
            "identical body already exists; persisting a new memory anyway"
        );
    }

    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

    let memory_id = memories::insert(&tx, &new_memory)?;
    versions::insert_version(
        &tx,
        memory_id,
        1,
        &staged.name,
        memory_type,
        &staged.description,
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
        &staged.embedding,
        &staged.name,
        &staged.snippet,
    )?;

    if staged.chunks_info.len() > 1 {
        storage_chunks::insert_chunk_slices(&tx, memory_id, &new_memory.body, &staged.chunks_info)?;
        let chunk_embeddings = staged.chunk_embeddings.ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!(
                "missing chunk embeddings cache on multi-chunk ingest path"
            ))
        })?;
        for (i, emb) in chunk_embeddings.iter().enumerate() {
            storage_chunks::upsert_chunk_vec(&tx, i as i64, memory_id, i as i32, emb)?;
        }
    }

    if !staged.entities.is_empty() || !staged.relationships.is_empty() {
        for (idx, entity) in staged.entities.iter().enumerate() {
            let entity_id = entities::upsert_entity(&tx, namespace, entity)?;
            let entity_embedding = &staged.entity_embeddings[idx];
            entities::upsert_entity_vec(
                &tx,
                entity_id,
                namespace,
                entity.entity_type,
                entity_embedding,
                &entity.name,
            )?;
            entities::link_memory_entity(&tx, memory_id, entity_id)?;
            entities::increment_degree(&tx, entity_id)?;
        }
        let entity_types: std::collections::HashMap<&str, EntityType> = staged
            .entities
            .iter()
            .map(|entity| (entity.name.as_str(), entity.entity_type))
            .collect();
        for rel in &staged.relationships {
            let source_entity = NewEntity {
                name: rel.source.clone(),
                entity_type: entity_types
                    .get(rel.source.as_str())
                    .copied()
                    .unwrap_or(EntityType::Concept),
                description: None,
            };
            let target_entity = NewEntity {
                name: rel.target.clone(),
                entity_type: entity_types
                    .get(rel.target.as_str())
                    .copied()
                    .unwrap_or(EntityType::Concept),
                description: None,
            };
            let source_id = entities::upsert_entity(&tx, namespace, &source_entity)?;
            let target_id = entities::upsert_entity(&tx, namespace, &target_entity)?;
            let rel_id = entities::upsert_relationship(&tx, namespace, source_id, target_id, rel)?;
            entities::link_memory_relationship(&tx, memory_id, rel_id)?;
        }
    }

    tx.commit()?;

    if !staged.urls.is_empty() {
        let url_entries: Vec<storage_urls::MemoryUrl> = staged
            .urls
            .into_iter()
            .map(|u| storage_urls::MemoryUrl {
                url: u.url,
                offset: Some(u.start as i64),
            })
            .collect();
        let _ = storage_urls::insert_urls(conn, memory_id, &url_entries);
    }

    Ok(FileSuccess {
        memory_id,
        action: "created".to_string(),
        body_length: new_memory.body.len(),
    })
}

#[tracing::instrument(skip_all, level = "debug", name = "ingest")]
pub fn run(args: IngestArgs) -> Result<(), AppError> {
    // TODO(G20): add mode-conditional flag validation before DB access.
    // Flags that are silently discarded when the wrong mode is active:
    //   --mode none/gliner:   claude_binary, claude_model, claude_timeout,
    //                         max_cost_usd, rate_limit_wait, resume,
    //                         retry_failed, keep_queue, queue_db
    //   --mode none/gliner:   codex_binary, codex_model, codex_timeout
    //   --mode claude-code:   codex_binary, codex_model, codex_timeout
    //   --mode codex:         claude_binary, claude_model, claude_timeout,
    //                         max_cost_usd, rate_limit_wait
    //   --mode none:          gliner_variant (only meaningful with --enable-ner
    //                         or --mode gliner)
    // Approach: after the mode dispatch block below, check each non-default
    // flag value and return Err(AppError::Validation(...)) for mismatches.
    tracing::debug!(target: "ingest", dir = %args.dir.display(), mode = ?args.mode, "starting ingest");
    if args.mode == IngestMode::ClaudeCode {
        return super::ingest_claude::run_claude_ingest(&args);
    }
    if args.mode == IngestMode::Codex {
        return super::ingest_codex::run_codex_ingest(&args);
    }

    let started = std::time::Instant::now();

    if !args.dir.exists() {
        return Err(AppError::Validation(format!(
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

    let mut files: Vec<PathBuf> = Vec::with_capacity(128);
    collect_files(&args.dir, &args.pattern, args.recursive, &mut files)?;
    files.sort_unstable();

    if files.len() > args.max_files {
        return Err(AppError::Validation(format!(
            "found {} files matching pattern, exceeds --max-files cap of {} (raise the cap or narrow the pattern)",
            files.len(),
            args.max_files
        )));
    }

    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
    let memory_type_str = args.r#type.as_str().to_string();

    let paths = AppPaths::resolve(args.db.as_deref())?;
    let mut conn_or_err = match init_storage(&paths) {
        Ok(c) => Ok(c),
        Err(e) => Err(format!("{e}")),
    };

    let mut succeeded: usize = 0;
    let mut failed: usize = 0;
    let mut skipped: usize = 0;
    let total = files.len();

    // Pre-resolve all names before parallelisation so Phase A workers see a
    // consistent, immutable name assignment (v1.0.31 A10 contract preserved).
    let mut taken_names: BTreeSet<String> = BTreeSet::new();

    // SlotMeta: per-slot output metadata retained on the main thread for NDJSON.
    // ProcessItem: the data moved into the producer thread for Phase A computation.
    // We split these so `slots_meta` (non-Send BTreeSet-dependent) stays on main
    // thread while `process_items` (Send: only PathBuf + String) crosses the thread
    // boundary into the rayon producer.
    enum SlotMeta {
        Skip {
            file_str: String,
            derived_base: String,
            name_truncated: bool,
            original_name: Option<String>,
            original_filename: Option<String>,
            reason: String,
        },
        Process {
            file_str: String,
            derived_name: String,
            name_truncated: bool,
            original_name: Option<String>,
            original_filename: Option<String>,
        },
    }

    struct ProcessItem {
        idx: usize,
        path: PathBuf,
        file_str: String,
        derived_name: String,
    }

    let files_cap = files.len();
    let mut slots_meta: Vec<SlotMeta> = Vec::new();
    slots_meta.try_reserve(files_cap).map_err(|_| {
        AppError::LimitExceeded(format!(
            "allocation of {files_cap} slot metadata entries would exceed available memory"
        ))
    })?;
    let mut process_items: Vec<ProcessItem> = Vec::new();
    process_items.try_reserve(files_cap).map_err(|_| {
        AppError::LimitExceeded(format!(
            "allocation of {files_cap} process items would exceed available memory"
        ))
    })?;
    let mut truncations: Vec<(String, String)> = Vec::new();
    truncations.try_reserve(files_cap).map_err(|_| {
        AppError::LimitExceeded(format!(
            "allocation of {files_cap} truncation entries would exceed available memory"
        ))
    })?;

    let max_name_length = args.max_name_length;
    for path in &files {
        let file_str = path.to_string_lossy().into_owned();
        let (derived_base, name_truncated, original_name) =
            derive_kebab_name(path, max_name_length);
        let original_basename = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

        if name_truncated {
            if let Some(ref orig) = original_name {
                truncations.push((orig.clone(), derived_base.clone()));
            }
        }

        if derived_base.is_empty() {
            // original_filename: always include when it differs from the empty derived name
            let orig_filename = if !original_basename.is_empty() {
                Some(original_basename.to_string())
            } else {
                None
            };
            slots_meta.push(SlotMeta::Skip {
                file_str,
                derived_base: String::new(),
                name_truncated: false,
                original_name: None,
                original_filename: orig_filename,
                reason: "could not derive a non-empty kebab-case name from filename".to_string(),
            });
            continue;
        }

        match unique_name(&derived_base, &taken_names) {
            Ok(derived_name) => {
                taken_names.insert(derived_name.clone());
                let idx = slots_meta.len();
                // original_filename: present only when the raw basename differs from the derived name
                let orig_filename = if original_basename != derived_name {
                    Some(original_basename.to_string())
                } else {
                    None
                };
                process_items.push(ProcessItem {
                    idx,
                    path: path.clone(),
                    file_str: file_str.clone(),
                    derived_name: derived_name.clone(),
                });
                slots_meta.push(SlotMeta::Process {
                    file_str,
                    derived_name,
                    name_truncated,
                    original_name,
                    original_filename: orig_filename,
                });
            }
            Err(e) => {
                let orig_filename = if original_basename != derived_base {
                    Some(original_basename.to_string())
                } else {
                    None
                };
                slots_meta.push(SlotMeta::Skip {
                    file_str,
                    derived_base,
                    name_truncated,
                    original_name,
                    original_filename: orig_filename,
                    reason: e.to_string(),
                });
            }
        }
    }

    if !truncations.is_empty() {
        tracing::info!(
            target: "ingest",
            count = truncations.len(),
            max_name_length = max_name_length,
            max_len = DERIVED_NAME_MAX_LEN,
            "derived names truncated; pass -vv (debug) for per-file detail"
        );
    }

    // --dry-run: emit preview events and exit before loading ONNX or touching DB.
    if args.dry_run {
        for meta in &slots_meta {
            match meta {
                SlotMeta::Skip {
                    file_str,
                    derived_base,
                    name_truncated,
                    original_name,
                    original_filename,
                    reason,
                } => {
                    output::emit_json_compact(&IngestFileEvent {
                        file: file_str,
                        name: derived_base,
                        status: "skip",
                        truncated: *name_truncated,
                        original_name: original_name.clone(),
                        original_filename: original_filename.as_deref(),
                        error: Some(reason.clone()),
                        memory_id: None,
                        action: None,
                        body_length: 0,
                    })?;
                }
                SlotMeta::Process {
                    file_str,
                    derived_name,
                    name_truncated,
                    original_name,
                    original_filename,
                } => {
                    output::emit_json_compact(&IngestFileEvent {
                        file: file_str,
                        name: derived_name,
                        status: "preview",
                        truncated: *name_truncated,
                        original_name: original_name.clone(),
                        original_filename: original_filename.as_deref(),
                        error: None,
                        memory_id: None,
                        action: None,
                        body_length: 0,
                    })?;
                }
            }
        }
        output::emit_json_compact(&IngestSummary {
            summary: true,
            dir: args.dir.to_string_lossy().into_owned(),
            pattern: args.pattern.clone(),
            recursive: args.recursive,
            files_total: total,
            files_succeeded: 0,
            files_failed: 0,
            files_skipped: 0,
            elapsed_ms: started.elapsed().as_millis() as u64,
        })?;
        return Ok(());
    }

    // Reject contradictory flag combination: explicit parallelism > 1 with --low-memory.
    if args.low_memory {
        if let Some(n) = args.ingest_parallelism {
            if n > 1 {
                return Err(AppError::Validation(
                    "--ingest-parallelism N>1 conflicts with --low-memory; use one or the other"
                        .to_string(),
                ));
            }
        }
    }

    // Determine rayon thread pool size, honoring --low-memory and the
    // SQLITE_GRAPHRAG_LOW_MEMORY env var (both force parallelism = 1).
    let parallelism = resolve_parallelism(args.low_memory, args.ingest_parallelism);

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(parallelism)
        .build()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("rayon pool: {e}")))?;

    if args.enable_ner && args.skip_extraction {
        return Err(AppError::Validation(
            "--enable-ner and --skip-extraction are mutually exclusive; remove one".to_string(),
        ));
    }
    if args.skip_extraction && !args.enable_ner {
        // v1.0.74: revert to v1.0.45 hidden no-op behavior. The v1.0.67
        // commit (9ddb17b) promoted this to a hard validation error, which
        // broke the "kept as a hidden no-op for backwards compatibility"
        // promise documented in CHANGELOG v1.0.45 and started failing
        // 5+ CI jobs whose E2E tests use this flag to skip the
        // GLiNER-ONNX model download in CI environments.
        tracing::warn!(
            "--skip-extraction is deprecated since v1.0.45 and has no effect (NER is disabled by default); remove this flag to silence the warning"
        );
    }
    let enable_ner = args.enable_ner;
    let max_rss_mb = args.max_rss_mb;
    let gliner_variant: crate::extraction::GlinerVariant = match args.gliner_variant.as_str() {
        "int8" => crate::extraction::GlinerVariant::Int8,
        _ => crate::extraction::GlinerVariant::Fp32,
    };

    let total_to_process = process_items.len();
    tracing::info!(
        target: "ingest",
        phase = "pipeline_start",
        files = total_to_process,
        ingest_parallelism = parallelism,
        "incremental pipeline starting: Phase A (rayon) → channel → Phase B (main thread)",
    );

    // Bounded channel: producer never gets more than parallelism*2 items ahead of
    // the consumer, preventing memory blowup when Phase A is faster than Phase B.
    // Each message carries the slot index so Phase B can look up SlotMeta in order.
    let channel_bound = (parallelism * 2).max(1);
    let (tx, rx) = mpsc::sync_channel::<(usize, Result<StagedFile, AppError>)>(channel_bound);

    // Phase A: launched in a dedicated OS thread so the main thread can consume
    // the channel concurrently. pool.install() blocks the calling thread until
    // all rayon workers finish — if called on the main thread it would
    // reintroduce the 2-phase blocking behaviour we are eliminating.
    let paths_owned = paths.clone();
    let producer_handle = std::thread::spawn(move || {
        pool.install(|| {
            process_items.into_par_iter().for_each(|item| {
                if crate::shutdown_requested() {
                    return;
                }
                let t0 = std::time::Instant::now();
                let result = stage_file(
                    item.idx,
                    &item.path,
                    &item.derived_name,
                    &paths_owned,
                    enable_ner,
                    gliner_variant,
                    max_rss_mb,
                );
                let elapsed_ms = t0.elapsed().as_millis() as u64;

                // Emit NDJSON progress event to stderr so the user sees work
                // happening during long NER runs (e.g. 50 files × 27s each).
                let (n_entities, n_relationships) = match &result {
                    Ok(sf) => (sf.entities.len(), sf.relationships.len()),
                    Err(_) => (0, 0),
                };
                let progress = StageProgressEvent {
                    schema_version: 1,
                    event: "file_extracted",
                    path: &item.file_str,
                    ms: elapsed_ms,
                    entities: n_entities,
                    relationships: n_relationships,
                };
                if let Ok(line) = serde_json::to_string(&progress) {
                    tracing::info!(target: "ingest_progress", "{}", line);
                }

                // Blocking send applies backpressure: if Phase B is slower,
                // Phase A workers wait here instead of accumulating staged files
                // in memory. If the receiver is dropped (fail_fast abort), ignore.
                let _ = tx.send((item.idx, result));
            });
            // Explicit drop of tx signals Phase B (rx iteration) to stop.
            drop(tx);
        });
    });

    // Phase B: main thread persists files as results arrive from the channel.
    // Results arrive in completion order (par_iter is unordered). We persist
    // each file immediately on arrival — this is the key fix for B1: with the
    // old 2-phase design the first DB write happened only after ALL files had
    // finished Phase A. Now the first commit happens as soon as the first file
    // completes Phase A, regardless of how many files remain.
    //
    // NDJSON output order follows completion order (not file-system sort order).
    // Skip slots are emitted at the end, after all Process results are consumed.
    // This trade-off is intentional: deterministic NDJSON ordering is a lesser
    // requirement than ensuring data is persisted before the user's timeout fires.
    let fail_fast = args.fail_fast;

    // Emit pending Skip events first so agents see them early.
    for meta in &slots_meta {
        if let SlotMeta::Skip {
            file_str,
            derived_base,
            name_truncated,
            original_name,
            original_filename,
            reason,
        } = meta
        {
            output::emit_json_compact(&IngestFileEvent {
                file: file_str,
                name: derived_base,
                status: "skipped",
                truncated: *name_truncated,
                original_name: original_name.clone(),
                original_filename: original_filename.as_deref(),
                error: Some(reason.clone()),
                memory_id: None,
                action: None,
                body_length: 0,
            })?;
            skipped += 1;
        }
    }

    // Build a quick index from slot index → SlotMeta reference for O(1) lookups
    // as channel messages arrive in completion order.
    let meta_index: std::collections::HashMap<usize, &SlotMeta> = slots_meta
        .iter()
        .enumerate()
        .filter(|(_, m)| matches!(m, SlotMeta::Process { .. }))
        .collect();

    tracing::info!(
        target: "ingest",
        phase = "persist_start",
        files = total_to_process,
        "phase B starting: persisting files incrementally as Phase A completes each one",
    );

    // Drain channel and persist each file immediately — no accumulation into a
    // HashMap. The bounded channel ensures Phase A cannot run too far ahead of
    // Phase B without applying backpressure.
    for (idx, stage_result) in rx {
        if crate::shutdown_requested() {
            tracing::info!(target: "ingest", "shutdown requested, stopping persistence loop");
            break;
        }
        let meta = meta_index.get(&idx).ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!(
                "channel idx {idx} has no corresponding Process slot"
            ))
        })?;
        let (file_str, derived_name, name_truncated, original_name, original_filename) = match meta
        {
            SlotMeta::Process {
                file_str,
                derived_name,
                name_truncated,
                original_name,
                original_filename,
            } => (
                file_str,
                derived_name,
                name_truncated,
                original_name,
                original_filename,
            ),
            SlotMeta::Skip { .. } => unreachable!("channel only carries Process results"),
        };

        // If storage init failed, every file fails with the same error.
        let conn = match conn_or_err.as_mut() {
            Ok(c) => c,
            Err(err_msg) => {
                let err_clone = err_msg.clone();
                output::emit_json_compact(&IngestFileEvent {
                    file: file_str,
                    name: derived_name,
                    status: "failed",
                    truncated: *name_truncated,
                    original_name: original_name.clone(),
                    original_filename: original_filename.as_deref(),
                    error: Some(err_clone.clone()),
                    memory_id: None,
                    action: None,
                    body_length: 0,
                })?;
                failed += 1;
                if fail_fast {
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

        let outcome =
            stage_result.and_then(|sf| persist_staged(conn, &namespace, &memory_type_str, sf));

        match outcome {
            Ok(FileSuccess {
                memory_id,
                action,
                body_length,
            }) => {
                output::emit_json_compact(&IngestFileEvent {
                    file: file_str,
                    name: derived_name,
                    status: "indexed",
                    truncated: *name_truncated,
                    original_name: original_name.clone(),
                    original_filename: original_filename.as_deref(),
                    error: None,
                    memory_id: Some(memory_id),
                    action: Some(action),
                    body_length,
                })?;
                succeeded += 1;
            }
            Err(ref e) if matches!(e, AppError::Duplicate(_)) => {
                output::emit_json_compact(&IngestFileEvent {
                    file: file_str,
                    name: derived_name,
                    status: "skipped",
                    truncated: *name_truncated,
                    original_name: original_name.clone(),
                    original_filename: original_filename.as_deref(),
                    error: Some(format!("{e}")),
                    memory_id: None,
                    action: Some("duplicate".to_string()),
                    body_length: 0,
                })?;
                skipped += 1;
            }
            Err(e) => {
                let err_msg = format!("{e}");
                output::emit_json_compact(&IngestFileEvent {
                    file: file_str,
                    name: derived_name,
                    status: "failed",
                    truncated: *name_truncated,
                    original_name: original_name.clone(),
                    original_filename: original_filename.as_deref(),
                    error: Some(err_msg.clone()),
                    memory_id: None,
                    action: None,
                    body_length: 0,
                })?;
                failed += 1;
                if fail_fast {
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

    // Wait for the producer thread to finish cleanly.
    producer_handle
        .join()
        .map_err(|_| AppError::Internal(anyhow::anyhow!("ingest producer thread panicked")))?;

    if let Ok(ref conn) = conn_or_err {
        if succeeded > 0 {
            let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
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

pub(crate) fn collect_files(
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

/// Returns `(final_name, truncated, original_name)`.
/// `truncated` is true when the derived name exceeded `max_len`.
/// `original_name` holds the pre-truncation name only when `truncated=true`.
///
/// Non-ASCII characters are first decomposed via NFD and then stripped of
/// combining marks so accented letters fold to their base ASCII letter
/// (e.g. `acai` from accented input, `naive` from diaeresis). Characters with no ASCII
/// fallback (emoji, CJK ideographs, symbols) are dropped silently. This
/// preserves meaningful word content rather than collapsing the basename
/// to a few stray ASCII letters as the previous filter did.
pub(crate) fn derive_kebab_name(path: &Path, max_len: usize) -> (String, bool, Option<String>) {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let lowered: String = stem
        .nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
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
    let trimmed_raw = collapsed.trim_matches('-').to_string();
    // Prefix names that start with a digit to keep them valid kebab-case identifiers.
    let trimmed = if trimmed_raw.starts_with(|c: char| c.is_ascii_digit()) {
        format!("doc-{trimmed_raw}")
    } else {
        trimmed_raw
    };
    if trimmed.len() > max_len {
        let truncated = trimmed[..max_len].trim_matches('-').to_string();
        tracing::debug!(
            target: "ingest",
            original = %trimmed,
            truncated_to = %truncated,
            max_len = max_len,
            "derived memory name truncated to fit length cap; collisions will be resolved with numeric suffixes"
        );
        (truncated, true, Some(trimmed))
    } else {
        (trimmed, false, None)
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
        let (name, truncated, original) = derive_kebab_name(&p, DERIVED_NAME_MAX_LEN);
        assert_eq!(name, "claude-code-headless");
        assert!(!truncated);
        assert!(original.is_none());
    }

    #[test]
    fn derive_kebab_uppercase_lowered() {
        let p = PathBuf::from("/tmp/README.md");
        let (name, truncated, original) = derive_kebab_name(&p, DERIVED_NAME_MAX_LEN);
        assert_eq!(name, "readme");
        assert!(!truncated);
        assert!(original.is_none());
    }

    #[test]
    fn derive_kebab_strips_non_kebab_chars() {
        let p = PathBuf::from("/tmp/some@weird#name!.md");
        let (name, truncated, original) = derive_kebab_name(&p, DERIVED_NAME_MAX_LEN);
        assert_eq!(name, "someweirdname");
        assert!(!truncated);
        assert!(original.is_none());
    }

    // Bug M-A3: NFD-based unicode normalization preserves base letters of
    // accented characters instead of dropping them entirely.
    #[test]
    fn derive_kebab_folds_accented_letters_to_ascii() {
        let p = PathBuf::from("/tmp/açaí.md");
        let (name, _, _) = derive_kebab_name(&p, DERIVED_NAME_MAX_LEN);
        assert_eq!(name, "acai", "got '{name}'");
    }

    #[test]
    fn derive_kebab_handles_naive_with_diaeresis() {
        let p = PathBuf::from("/tmp/naïve-test.md");
        let (name, _, _) = derive_kebab_name(&p, DERIVED_NAME_MAX_LEN);
        assert_eq!(name, "naive-test", "got '{name}'");
    }

    #[test]
    fn derive_kebab_drops_emoji_keeps_word() {
        let p = PathBuf::from("/tmp/🚀-rocket.md");
        let (name, _, _) = derive_kebab_name(&p, DERIVED_NAME_MAX_LEN);
        assert_eq!(name, "rocket", "got '{name}'");
    }

    #[test]
    fn derive_kebab_mixed_unicode_emoji_keeps_letters() {
        let p = PathBuf::from("/tmp/açaí🦜.md");
        let (name, _, _) = derive_kebab_name(&p, DERIVED_NAME_MAX_LEN);
        assert_eq!(name, "acai", "got '{name}'");
    }

    #[test]
    fn derive_kebab_pure_emoji_yields_empty() {
        let p = PathBuf::from("/tmp/🦜🚀🌟.md");
        let (name, _, _) = derive_kebab_name(&p, DERIVED_NAME_MAX_LEN);
        assert!(name.is_empty(), "got '{name}'");
    }

    #[test]
    fn derive_kebab_collapses_consecutive_dashes() {
        let p = PathBuf::from("/tmp/a__b___c.md");
        let (name, truncated, original) = derive_kebab_name(&p, DERIVED_NAME_MAX_LEN);
        assert_eq!(name, "a-b-c");
        assert!(!truncated);
        assert!(original.is_none());
    }

    #[test]
    fn derive_kebab_truncates_to_60_chars() {
        let p = PathBuf::from(format!("/tmp/{}.md", "a".repeat(80)));
        let (name, truncated, original) = derive_kebab_name(&p, DERIVED_NAME_MAX_LEN);
        assert!(name.len() <= 60, "got len {}", name.len());
        assert!(truncated);
        assert!(original.is_some());
        assert!(original.unwrap().len() > 60);
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
        let (name, truncated, original) = derive_kebab_name(&p, DERIVED_NAME_MAX_LEN);
        assert!(
            name.len() <= DERIVED_NAME_MAX_LEN,
            "truncated name must respect cap; got {} chars",
            name.len()
        );
        assert!(!name.is_empty());
        assert!(truncated);
        assert!(original.is_some());
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
    fn validate_relation_format_accepts_valid_relations() {
        use crate::parsers::{is_canonical_relation, validate_relation_format};
        assert!(validate_relation_format("applies_to").is_ok());
        assert!(validate_relation_format("depends_on").is_ok());
        assert!(validate_relation_format("implements").is_ok());
        assert!(validate_relation_format("").is_err());
        assert!(is_canonical_relation("applies_to"));
        assert!(!is_canonical_relation("implements"));
    }

    // ── v1.0.40 H-A1: --low-memory flag and SQLITE_GRAPHRAG_LOW_MEMORY env var ──

    use serial_test::serial;

    /// Helper: scrubs the env var around a closure to keep tests deterministic.
    fn with_env_var<F: FnOnce()>(value: Option<&str>, f: F) {
        let key = "SQLITE_GRAPHRAG_LOW_MEMORY";
        let prev = std::env::var(key).ok();
        match value {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
        f();
        match prev {
            Some(p) => std::env::set_var(key, p),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    #[serial]
    fn env_low_memory_enabled_unset_returns_false() {
        with_env_var(None, || assert!(!env_low_memory_enabled()));
    }

    #[test]
    #[serial]
    fn env_low_memory_enabled_empty_returns_false() {
        with_env_var(Some(""), || assert!(!env_low_memory_enabled()));
    }

    #[test]
    #[serial]
    fn env_low_memory_enabled_truthy_values_return_true() {
        for v in ["1", "true", "TRUE", "yes", "YES", "on", "On"] {
            with_env_var(Some(v), || {
                assert!(env_low_memory_enabled(), "value {v:?} should be truthy")
            });
        }
    }

    #[test]
    #[serial]
    fn env_low_memory_enabled_falsy_values_return_false() {
        for v in ["0", "false", "FALSE", "no", "off"] {
            with_env_var(Some(v), || {
                assert!(!env_low_memory_enabled(), "value {v:?} should be falsy")
            });
        }
    }

    #[test]
    #[serial]
    fn env_low_memory_enabled_unrecognized_value_returns_false() {
        with_env_var(Some("maybe"), || assert!(!env_low_memory_enabled()));
    }

    #[test]
    #[serial]
    fn resolve_parallelism_flag_forces_one_overriding_explicit_value() {
        with_env_var(None, || {
            assert_eq!(resolve_parallelism(true, Some(4)), 1);
            assert_eq!(resolve_parallelism(true, Some(8)), 1);
            assert_eq!(resolve_parallelism(true, None), 1);
        });
    }

    #[test]
    #[serial]
    fn resolve_parallelism_env_forces_one_when_flag_off() {
        with_env_var(Some("1"), || {
            assert_eq!(resolve_parallelism(false, Some(4)), 1);
            assert_eq!(resolve_parallelism(false, None), 1);
        });
    }

    #[test]
    #[serial]
    fn resolve_parallelism_falsy_env_does_not_override() {
        with_env_var(Some("0"), || {
            assert_eq!(resolve_parallelism(false, Some(4)), 4);
        });
    }

    #[test]
    #[serial]
    fn resolve_parallelism_explicit_value_when_low_memory_off() {
        with_env_var(None, || {
            assert_eq!(resolve_parallelism(false, Some(3)), 3);
            assert_eq!(resolve_parallelism(false, Some(1)), 1);
        });
    }

    #[test]
    #[serial]
    fn resolve_parallelism_default_when_unset() {
        with_env_var(None, || {
            let p = resolve_parallelism(false, None);
            assert!((1..=4).contains(&p), "default must be in [1, 4]; got {p}");
        });
    }

    #[test]
    fn ingest_args_parses_low_memory_flag_via_clap() {
        use clap::Parser;
        // Parse a synthetic Cli that contains the `ingest` subcommand. We rely
        // on the public `Cli` definition so the flag is wired end-to-end.
        let cli = crate::cli::Cli::try_parse_from([
            "sqlite-graphrag",
            "ingest",
            "/tmp/dummy",
            "--type",
            "document",
            "--low-memory",
        ])
        .expect("parse must succeed");
        match cli.command {
            crate::cli::Commands::Ingest(args) => {
                assert!(args.low_memory, "--low-memory must set field to true");
            }
            _ => panic!("expected Ingest subcommand"),
        }
    }

    #[test]
    fn ingest_args_low_memory_defaults_false() {
        use clap::Parser;
        let cli = crate::cli::Cli::try_parse_from([
            "sqlite-graphrag",
            "ingest",
            "/tmp/dummy",
            "--type",
            "document",
        ])
        .expect("parse must succeed");
        match cli.command {
            crate::cli::Commands::Ingest(args) => {
                assert!(!args.low_memory, "default must be false");
            }
            _ => panic!("expected Ingest subcommand"),
        }
    }
}
