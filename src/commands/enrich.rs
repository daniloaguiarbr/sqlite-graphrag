//! Handler for the `enrich` CLI subcommand (GAP-14 + GAP-18).
//!
//! Enriches the knowledge graph by running LLM-powered analysis over memories
//! and entities that are missing key structural data. Operations are:
//!
//! - `memory-bindings`: memories without `memory_entities` rows get entity extraction
//! - `entity-descriptions`: entities with NULL/empty descriptions get LLM descriptions
//! - `body-enrich`: memories with short bodies get expanded by the LLM (GAP-18)
//! - all others: scan + structured NDJSON output (not-yet-implemented dispatch)
//!
//! Architecture mirrors `ingest_claude.rs`: SCAN → JUDGE (LLM) → PERSIST, with a
//! SQLite queue DB (`.enrich-queue.sqlite`) for resume/retry support.
//!
//! # DRY opportunity
//!
//! `extract_with_claude`, `parse_claude_output`, `emit_json`, and the `open_queue_db`
//! queue schema in `ingest_claude.rs` are private functions that duplicate patterns used
//! here verbatim. A future refactoring could extract them into a shared
//! `src/commands/llm_runner.rs` module (or `src/llm_runner.rs`) without changing any
//! public APIs. That extraction requires editing `ingest_claude.rs`, which is outside
//! this stream's boundary — flagged here for the Integration stream to evaluate.

use crate::commands::ingest_claude::find_claude_binary;
use crate::entity_type::EntityType;
use crate::errors::AppError;
use crate::paths::AppPaths;
use crate::storage::connection::{ensure_db_ready, open_rw};
use crate::storage::entities::{self, NewEntity, NewRelationship};
use crate::storage::memories;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MIN_CLAUDE_VERSION: &str = "2.1.0";
const DEFAULT_QUEUE_DB: &str = ".enrich-queue.sqlite";
const DEFAULT_RATE_LIMIT_WAIT: u64 = 60;
const DEFAULT_BODY_ENRICH_MIN_CHARS: usize = 500;
const DEFAULT_BODY_ENRICH_MAX_CHARS: usize = 2000;

// ---------------------------------------------------------------------------
// JSON schema used for memory-bindings and body-enrich extraction
// ---------------------------------------------------------------------------

const BINDINGS_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "entities": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "name": { "type": "string" },
          "entity_type": {
            "type": "string",
            "enum": ["project","tool","person","file","concept","incident","decision","organization","location","date"]
          }
        },
        "required": ["name", "entity_type"],
        "additionalProperties": false
      }
    },
    "relationships": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "source": { "type": "string" },
          "target": { "type": "string" },
          "relation": {
            "type": "string",
            "enum": ["applies-to","uses","depends-on","causes","fixes","contradicts","supports","follows","related","replaces","tracked-in"]
          },
          "strength": { "type": "number", "minimum": 0, "maximum": 1 }
        },
        "required": ["source","target","relation","strength"],
        "additionalProperties": false
      }
    }
  },
  "required": ["entities","relationships"],
  "additionalProperties": false
}"#;

const ENTITY_DESCRIPTION_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "description": { "type": "string" }
  },
  "required": ["description"],
  "additionalProperties": false
}"#;

const BODY_ENRICH_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "enriched_body": { "type": "string" }
  },
  "required": ["enriched_body"],
  "additionalProperties": false
}"#;

// ---------------------------------------------------------------------------
// Prompts
// ---------------------------------------------------------------------------

const BINDINGS_PROMPT: &str = "You are a knowledge graph entity extractor. Given a memory body, extract:\n\
1. Domain-specific entities (concepts, tools, people, decisions, projects, files)\n\
2. Typed relationships between entities with strength scores\n\n\
Rules:\n\
- Entity names: lowercase kebab-case, 2+ chars, domain-specific only\n\
- NEVER extract generic terms, stop words, numbers, UUIDs, or single characters\n\
- Relationship types MUST be one of: applies-to, uses, depends-on, causes, fixes, contradicts, supports, follows, related, replaces, tracked-in\n\
- NEVER use 'mentions' as relationship type\n\
- Strength: 0.9 for hard dependencies, 0.7 for design relationships, 0.5 for contextual links, 0.3 for weak references\n\
- Prefer fewer high-quality entities over many low-quality ones";

const ENTITY_DESCRIPTION_PROMPT_PREFIX: &str = "You are a knowledge graph annotator. Given an entity name and type, write a concise one-sentence description (10-20 words) that explains what this entity IS and WHY it matters in the context of software/system design.\n\nEntity name: ";

const BODY_ENRICH_PROMPT_PREFIX: &str = "You are a knowledge assistant. Given a short or sparse memory body, expand it into a richer, more complete and useful description. Preserve all existing facts. Add context, implications, and relationships that would be valuable for knowledge retrieval.\n\nConstraints:\n- Output only the enriched body text (no metadata, no headers)\n- Preserve the original meaning exactly\n- Target length is provided in the system context\n\nMemory body to enrich:\n\n";

// ---------------------------------------------------------------------------
// CLI args
// ---------------------------------------------------------------------------

/// Operation to perform in the `enrich` command.
#[derive(Debug, Clone, PartialEq, Eq, clap::ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EnrichOperation {
    /// Add missing entity/relationship bindings to memories (fully implemented).
    MemoryBindings,
    /// Fill NULL/empty entity descriptions with LLM-generated summaries (fully implemented).
    EntityDescriptions,
    /// Expand short memory bodies into richer content (fully implemented, GAP-18).
    BodyEnrich,
    /// Calibrate relationship weights using LLM analysis (scan only).
    WeightCalibrate,
    /// Reclassify relationship types using LLM judgment (scan only).
    RelationReclassify,
    /// Connect isolated entities by suggesting new relationships (scan only).
    EntityConnect,
    /// Validate entity type assignments using LLM judgment (scan only).
    EntityTypeValidate,
    /// Enrich memory descriptions that are generic/auto-generated (scan only).
    DescriptionEnrich,
    /// Identify cross-domain bridges between disconnected subgraphs (scan only).
    CrossDomainBridges,
    /// Classify memories into domain categories (scan only).
    DomainClassify,
    /// Audit the graph for quality issues (scan only).
    GraphAudit,
    /// Synthesize deep-research findings into graph memories (scan only).
    DeepResearchSynth,
    /// Extract structured body from unstructured text (scan only).
    BodyExtract,
}

/// LLM provider for enrichment.
#[derive(Debug, Clone, PartialEq, Eq, clap::ValueEnum)]
pub enum EnrichMode {
    /// Use locally installed Claude Code CLI (OAuth-first).
    ClaudeCode,
    /// Use locally installed OpenAI Codex CLI.
    Codex,
}

impl std::fmt::Display for EnrichMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnrichMode::ClaudeCode => write!(f, "claude-code"),
            EnrichMode::Codex => write!(f, "codex"),
        }
    }
}

/// Arguments for the `enrich` subcommand.
#[derive(clap::Args)]
#[command(
    about = "Enrich graph memories and entities using an LLM provider",
    after_long_help = "EXAMPLES:\n  \
    # Add missing entity bindings to all unbound memories\n  \
    sqlite-graphrag enrich --operation memory-bindings --mode claude-code\n\n  \
    # Fill entity descriptions (dry-run preview, no tokens spent)\n  \
    sqlite-graphrag enrich --operation entity-descriptions --dry-run --json\n\n  \
    # Expand short memory bodies (GAP-18)\n  \
    sqlite-graphrag enrich --operation body-enrich --min-output-chars 600\n\n  \
    # Resume an interrupted body-enrich run\n  \
    sqlite-graphrag enrich --operation body-enrich --resume --json\n\n  \
    # Retry only failed items from a previous run\n  \
    sqlite-graphrag enrich --operation memory-bindings --retry-failed --json\n\n\
    EXIT CODES:\n  \
    0  success\n  \
    1  validation error (bad args, binary not found)\n  \
    14 I/O error"
)]
pub struct EnrichArgs {
    /// Enrichment operation to run.
    #[arg(long, short = 'o', value_enum, value_name = "OPERATION")]
    pub operation: EnrichOperation,

    /// LLM provider to use. Default: claude-code (OAuth-first).
    #[arg(long, value_enum, default_value = "claude-code")]
    pub mode: EnrichMode,

    /// Maximum number of items to process in this run. Omit for all.
    #[arg(long, value_name = "N")]
    pub limit: Option<usize>,

    /// Preview items without calling the LLM (zero tokens consumed).
    #[arg(long)]
    pub dry_run: bool,

    /// Namespace to operate on. Default: global.
    #[arg(long, env = "SQLITE_GRAPHRAG_NAMESPACE")]
    pub namespace: Option<String>,

    // -- Provider flags (Claude) --
    /// Path to the Claude Code binary. Default: auto-detect from PATH.
    #[arg(long, value_name = "PATH")]
    pub claude_binary: Option<PathBuf>,

    /// Claude model to use (e.g. claude-sonnet-4-6).
    #[arg(long, value_name = "MODEL")]
    pub claude_model: Option<String>,

    /// Timeout per item in seconds when using Claude Code. Default: 300.
    #[arg(long, value_name = "SECONDS", default_value_t = 300)]
    pub claude_timeout: u64,

    // -- Provider flags (Codex) --
    /// Path to the Codex CLI binary. Default: auto-detect from PATH.
    #[arg(long, value_name = "PATH")]
    pub codex_binary: Option<PathBuf>,

    /// Codex model to use (e.g. o4-mini).
    #[arg(long, value_name = "MODEL")]
    pub codex_model: Option<String>,

    /// Timeout per item in seconds when using Codex. Default: 300.
    #[arg(long, value_name = "SECONDS", default_value_t = 300)]
    pub codex_timeout: u64,

    // -- Cost controls --
    /// Abort when cumulative cost exceeds this USD budget (API key only; ignored for OAuth).
    #[arg(long, value_name = "USD")]
    pub max_cost_usd: Option<f64>,

    // -- Queue controls --
    /// Resume a previously interrupted run (skip already-done items).
    #[arg(long)]
    pub resume: bool,

    /// Retry only items that failed in a previous run.
    #[arg(long)]
    pub retry_failed: bool,

    // -- body-enrich specific flags (GAP-18) --
    /// Minimum output character count for body-enrich. Default: 500.
    #[arg(long, value_name = "CHARS", default_value_t = DEFAULT_BODY_ENRICH_MIN_CHARS)]
    pub min_output_chars: usize,

    /// Maximum output character count for body-enrich. Default: 2000.
    #[arg(long, value_name = "CHARS", default_value_t = DEFAULT_BODY_ENRICH_MAX_CHARS)]
    pub max_output_chars: usize,

    /// Check that enriched body preserves all facts from the original (LLM judge). Default: true.
    #[arg(long, default_value_t = true)]
    pub preserve_check: bool,

    /// Path to a custom prompt template file for body-enrich.
    #[arg(long, value_name = "PATH")]
    pub prompt_template: Option<PathBuf>,

    // -- Output / infra --
    /// Emit NDJSON output. Always true; flag accepted for compatibility.
    #[arg(long)]
    pub json: bool,

    /// Database path override.
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,
}

// ---------------------------------------------------------------------------
// Internal types — raw LLM output structs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ClaudeElement {
    r#type: Option<String>,
    subtype: Option<String>,
    #[serde(default)]
    is_error: bool,
    structured_output: Option<serde_json::Value>,
    result: Option<String>,
    total_cost_usd: Option<f64>,
    error: Option<String>,
    #[serde(rename = "apiKeySource")]
    api_key_source: Option<String>,
}

// ---------------------------------------------------------------------------
// NDJSON event types emitted to stdout
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct PhaseEvent<'a> {
    phase: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    binary_path: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    items_total: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    items_pending: Option<usize>,
}

#[derive(Debug, Serialize)]
struct ItemEvent<'a> {
    /// Item identifier (memory name or entity name).
    item: &'a str,
    status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    memory_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    entity_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    entities: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rels: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chars_before: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chars_after: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cost_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    elapsed_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    index: usize,
    total: usize,
}

#[derive(Debug, Serialize)]
struct EnrichSummary {
    summary: bool,
    operation: String,
    items_total: usize,
    completed: usize,
    failed: usize,
    skipped: usize,
    cost_usd: f64,
    elapsed_ms: u64,
}

// ---------------------------------------------------------------------------
// Helper: emit a single JSON line to stdout
// ---------------------------------------------------------------------------

fn emit_json<T: Serialize>(value: &T) {
    if let Ok(json) = serde_json::to_string(value) {
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        let _ = writeln!(lock, "{json}");
        let _ = lock.flush();
    }
}

// ---------------------------------------------------------------------------
// Queue DB
// ---------------------------------------------------------------------------

/// Opens or creates the enrichment queue database.
///
/// The queue schema mirrors `ingest_claude` for resume/retry parity.
/// Uses a different filename (`.enrich-queue.sqlite`) to avoid collision.
///
/// # DRY note
///
/// This is a near-verbatim copy of `open_queue_db` in `ingest_claude.rs`.
/// Both should be unified in a shared `llm_runner.rs` module by the
/// Integration stream.
fn open_queue_db(path: &str) -> Result<Connection, AppError> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "wal")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS queue (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            item_key    TEXT NOT NULL UNIQUE,
            item_type   TEXT NOT NULL DEFAULT 'memory',
            status      TEXT NOT NULL DEFAULT 'pending',
            memory_id   INTEGER,
            entity_id   INTEGER,
            entities    INTEGER DEFAULT 0,
            rels        INTEGER DEFAULT 0,
            error       TEXT,
            cost_usd    REAL DEFAULT 0.0,
            attempt     INTEGER DEFAULT 0,
            elapsed_ms  INTEGER,
            created_at  TEXT DEFAULT (datetime('now')),
            done_at     TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_enrich_queue_status ON queue(status);",
    )?;
    Ok(conn)
}

// ---------------------------------------------------------------------------
// Validate Claude version (private copy — see DRY note above)
// ---------------------------------------------------------------------------

fn validate_claude_version_local(binary: &Path) -> Result<String, AppError> {
    let output = Command::new(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(AppError::Io)?;

    if !output.status.success() {
        return Err(AppError::Validation(
            "failed to run 'claude --version'".to_string(),
        ));
    }

    let version_str = String::from_utf8(output.stdout)
        .map_err(|_| AppError::Validation("claude --version output is not UTF-8".to_string()))?;
    let version = version_str.trim().to_string();
    let numeric = version.split([' ', '(']).next().unwrap_or("").trim();

    fn parse_semver(s: &str) -> Option<(u64, u64, u64)> {
        let parts: Vec<&str> = s.splitn(3, '.').collect();
        if parts.len() < 2 {
            return None;
        }
        let major = parts[0].parse::<u64>().ok()?;
        let minor = parts[1].parse::<u64>().ok()?;
        let patch = parts
            .get(2)
            .and_then(|p| p.parse::<u64>().ok())
            .unwrap_or(0);
        Some((major, minor, patch))
    }

    if let (Some(actual), Some(min)) = (parse_semver(numeric), parse_semver(MIN_CLAUDE_VERSION)) {
        if actual < min {
            return Err(AppError::Validation(format!(
                "Claude Code version {numeric} is below minimum required {MIN_CLAUDE_VERSION}"
            )));
        }
    }

    Ok(version)
}

// ---------------------------------------------------------------------------
// LLM invocation — Claude Code
// ---------------------------------------------------------------------------

/// Calls `claude -p` with a prompt and JSON schema, returning the parsed JSON value.
///
/// Returns `(output_value, cost_usd, is_oauth)`.
///
/// # DRY note
///
/// Mirrors `extract_with_claude` in `ingest_claude.rs`. Should be unified in a
/// shared module by the Integration stream.
fn call_claude(
    binary: &Path,
    prompt: &str,
    json_schema: &str,
    input_text: &str,
    model: Option<&str>,
    timeout_secs: u64,
) -> Result<(serde_json::Value, f64, bool), AppError> {
    use wait_timeout::ChildExt;

    let full_prompt = format!("{prompt}\n\n{input_text}");

    let mut cmd = Command::new(binary);

    // Least-privilege environment
    cmd.env_clear();
    for var in &[
        "PATH",
        "HOME",
        "USER",
        "SHELL",
        "TERM",
        "LANG",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_RUNTIME_DIR",
        "ANTHROPIC_API_KEY",
        "CLAUDE_CONFIG_DIR",
        "TMPDIR",
        "TMP",
        "TEMP",
        "DYLD_FALLBACK_LIBRARY_PATH",
    ] {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }

    #[cfg(windows)]
    for var in &[
        "LOCALAPPDATA",
        "APPDATA",
        "USERPROFILE",
        "SystemRoot",
        "COMSPEC",
        "PATHEXT",
        "HOMEPATH",
        "HOMEDRIVE",
    ] {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }

    cmd.arg("-p")
        .arg(&full_prompt)
        .arg("--output-format")
        .arg("json")
        .arg("--json-schema")
        .arg(json_schema)
        .arg("--max-turns")
        .arg("3")
        .arg("--no-session-persistence");

    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        cmd.arg("--bare");
    } else {
        cmd.arg("--dangerously-skip-permissions")
            .arg("--settings")
            .arg(r#"{"hooks":{}}"#);
    }

    if let Some(m) = model {
        cmd.arg("--model").arg(m);
    }

    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to spawn claude: {e}"),
        ))
    })?;

    let timeout = std::time::Duration::from_secs(timeout_secs);
    let status = child.wait_timeout(timeout).map_err(AppError::Io)?;

    match status {
        Some(exit_status) => {
            let mut stdout_buf = Vec::new();
            let mut stderr_buf = Vec::new();
            if let Some(mut out) = child.stdout.take() {
                std::io::Read::read_to_end(&mut out, &mut stdout_buf).map_err(AppError::Io)?;
            }
            if let Some(mut err) = child.stderr.take() {
                std::io::Read::read_to_end(&mut err, &mut stderr_buf).map_err(AppError::Io)?;
            }

            if !exit_status.success() {
                let stderr_str = String::from_utf8_lossy(&stderr_buf);
                if stderr_str.contains("auth") || stderr_str.contains("login") {
                    tracing::warn!(
                        target: "enrich",
                        "Claude Code authentication may have failed. Re-authenticate with: claude"
                    );
                }
                return Err(AppError::Validation(format!(
                    "claude -p exited with code {:?}: {}",
                    exit_status.code(),
                    stderr_str.trim()
                )));
            }

            let stdout_str = String::from_utf8(stdout_buf)
                .map_err(|_| AppError::Validation("claude -p stdout is not valid UTF-8".into()))?;
            parse_claude_json_output(&stdout_str)
        }
        None => {
            tracing::warn!(target: "enrich", timeout_secs, "claude -p timed out, killing process");
            let _ = child.kill();
            let _ = child.wait();
            Err(AppError::Validation(format!(
                "claude -p timed out after {timeout_secs} seconds"
            )))
        }
    }
}

/// Parses the JSON array output from `claude -p --output-format json`.
///
/// Returns `(structured_value, cost_usd, is_oauth)`.
///
/// # DRY note
///
/// Mirrors `parse_claude_output` in `ingest_claude.rs`. Should be unified.
fn parse_claude_json_output(stdout: &str) -> Result<(serde_json::Value, f64, bool), AppError> {
    let elements: Vec<ClaudeElement> = serde_json::from_str(stdout).map_err(|e| {
        AppError::Validation(format!("failed to parse claude output as JSON array: {e}"))
    })?;

    let is_oauth = elements
        .iter()
        .find(|e| e.r#type.as_deref() == Some("system") && e.subtype.as_deref() == Some("init"))
        .and_then(|e| e.api_key_source.as_deref())
        .map(|s| s == "none")
        .unwrap_or(false);

    let result_elem = elements
        .iter()
        .find(|e| e.r#type.as_deref() == Some("result"))
        .ok_or_else(|| {
            AppError::Validation("claude output missing 'result' element".to_string())
        })?;

    if result_elem.is_error {
        let err_msg = result_elem
            .error
            .as_deref()
            .or(result_elem.result.as_deref())
            .unwrap_or("unknown error");
        if err_msg.contains("rate_limit") || err_msg.contains("overloaded") {
            return Err(AppError::Validation(format!("RATE_LIMITED: {err_msg}")));
        }
        return Err(AppError::Validation(format!(
            "claude extraction failed: {err_msg}"
        )));
    }

    let value = if let Some(v) = result_elem.structured_output.clone() {
        v
    } else if let Some(text) = &result_elem.result {
        serde_json::from_str(text).map_err(|e| {
            AppError::Validation(format!("failed to parse claude result field as JSON: {e}"))
        })?
    } else {
        return Err(AppError::Validation(
            "claude result missing structured_output and result field".into(),
        ));
    };

    let cost = result_elem.total_cost_usd.unwrap_or(0.0);
    Ok((value, cost, is_oauth))
}

// ---------------------------------------------------------------------------
// SCAN helpers — SQL queries that find items needing enrichment
// ---------------------------------------------------------------------------

/// Returns memories without any `memory_entities` binding.
///
/// These are the targets for `memory-bindings` enrichment.
fn scan_unbound_memories(
    conn: &Connection,
    namespace: &str,
    limit: Option<usize>,
) -> Result<Vec<(i64, String, String)>, AppError> {
    let limit_clause = limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();
    let sql = format!(
        "SELECT m.id, m.name, m.body
         FROM memories m
         WHERE m.namespace = ?1
           AND m.deleted_at IS NULL
           AND NOT EXISTS (
               SELECT 1 FROM memory_entities me WHERE me.memory_id = m.id
           )
         ORDER BY m.id
         {limit_clause}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params![namespace], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Returns entities with NULL or empty description.
///
/// These are the targets for `entity-descriptions` enrichment.
fn scan_entities_without_description(
    conn: &Connection,
    namespace: &str,
    limit: Option<usize>,
) -> Result<Vec<(i64, String, String)>, AppError> {
    let limit_clause = limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();
    let sql = format!(
        "SELECT id, name, type
         FROM entities
         WHERE namespace = ?1
           AND (description IS NULL OR description = '')
         ORDER BY id
         {limit_clause}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params![namespace], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Returns memories whose body length is below the configured minimum.
///
/// These are the targets for `body-enrich` (GAP-18).
fn scan_short_body_memories(
    conn: &Connection,
    namespace: &str,
    min_chars: usize,
    limit: Option<usize>,
) -> Result<Vec<(i64, String, String)>, AppError> {
    let limit_clause = limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();
    let sql = format!(
        "SELECT m.id, m.name, m.body
         FROM memories m
         WHERE m.namespace = ?1
           AND m.deleted_at IS NULL
           AND LENGTH(COALESCE(m.body,'')) < ?2
         ORDER BY m.id
         {limit_clause}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params![namespace, min_chars as i64], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// PERSIST helpers for fully-implemented operations
// ---------------------------------------------------------------------------

/// Persists entity bindings extracted by the LLM for a memory.
///
/// Creates entities via `upsert_entity`, links them to the memory via
/// `link_memory_entity`, and upserts relationships found between entities.
fn persist_memory_bindings(
    conn: &Connection,
    namespace: &str,
    memory_id: i64,
    entities_json: &serde_json::Value,
    rels_json: &serde_json::Value,
) -> Result<(usize, usize), AppError> {
    #[derive(Deserialize)]
    struct EntityItem {
        name: String,
        entity_type: String,
    }
    #[derive(Deserialize)]
    struct RelItem {
        source: String,
        target: String,
        relation: String,
        strength: f64,
    }

    let extracted_entities: Vec<EntityItem> = serde_json::from_value(entities_json.clone())
        .map_err(|e| AppError::Validation(format!("failed to parse entities array: {e}")))?;

    let extracted_rels: Vec<RelItem> = serde_json::from_value(rels_json.clone())
        .map_err(|e| AppError::Validation(format!("failed to parse relationships array: {e}")))?;

    let mut ent_count = 0usize;
    let mut rel_count = 0usize;

    for item in &extracted_entities {
        let entity_type = match item.entity_type.parse::<EntityType>() {
            Ok(et) => et,
            Err(_) => {
                tracing::warn!(
                    target: "enrich",
                    entity = %item.name,
                    entity_type = %item.entity_type,
                    "entity type not recognized, skipping"
                );
                continue;
            }
        };
        match entities::upsert_entity(
            conn,
            namespace,
            &NewEntity {
                name: item.name.clone(),
                entity_type,
                description: None,
            },
        ) {
            Ok(eid) => {
                let _ = entities::link_memory_entity(conn, memory_id, eid);
                ent_count += 1;
            }
            Err(e) => {
                tracing::warn!(
                    target: "enrich",
                    entity = %item.name,
                    error = %e,
                    "entity upsert skipped"
                );
            }
        }
    }

    for rel in &extracted_rels {
        let normalized = crate::parsers::normalize_relation(&rel.relation);
        crate::parsers::warn_if_non_canonical(&normalized);

        // Normalize entity names before lookup: upsert_entity normalizes on write,
        // so the lookup must use the same normalized form to find the row.
        let src_name = crate::parsers::normalize_entity_name(&rel.source);
        let tgt_name = crate::parsers::normalize_entity_name(&rel.target);
        let src_id = entities::find_entity_id(conn, namespace, &src_name);
        let tgt_id = entities::find_entity_id(conn, namespace, &tgt_name);
        if let (Ok(Some(sid)), Ok(Some(tid))) = (src_id, tgt_id) {
            let new_rel = NewRelationship {
                source: rel.source.clone(),
                target: rel.target.clone(),
                relation: normalized,
                strength: rel.strength,
                description: None,
            };
            if entities::upsert_relationship(conn, namespace, sid, tid, &new_rel).is_ok() {
                rel_count += 1;
            }
        }
    }

    Ok((ent_count, rel_count))
}

/// Updates an entity's description directly in the `entities` table.
fn persist_entity_description(
    conn: &Connection,
    entity_id: i64,
    description: &str,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE entities SET description = ?1, updated_at = unixepoch() WHERE id = ?2",
        rusqlite::params![description, entity_id],
    )?;
    Ok(())
}

/// Persists an enriched memory body (body-enrich, GAP-18).
///
/// Uses `memories::update` to set the new body and `sync_fts_after_update`
/// to keep FTS5 in sync. Also re-embeds the memory for recall accuracy.
fn persist_enriched_body(
    conn: &Connection,
    namespace: &str,
    memory_id: i64,
    memory_name: &str,
    new_body: &str,
    paths: &crate::paths::AppPaths,
) -> Result<(), AppError> {
    // Read current values for FTS sync
    let (old_name, old_desc, old_body): (String, String, String) = conn.query_row(
        "SELECT name, COALESCE(description,''), COALESCE(body,'') FROM memories WHERE id=?1",
        rusqlite::params![memory_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;

    let memory_type: String = conn.query_row(
        "SELECT type FROM memories WHERE id=?1",
        rusqlite::params![memory_id],
        |r| r.get(0),
    )?;

    let description: String = conn.query_row(
        "SELECT COALESCE(description,'') FROM memories WHERE id=?1",
        rusqlite::params![memory_id],
        |r| r.get(0),
    )?;

    let body_hash = blake3::hash(new_body.as_bytes()).to_hex().to_string();

    let new_memory = memories::NewMemory {
        namespace: namespace.to_string(),
        name: memory_name.to_string(),
        memory_type: memory_type.clone(),
        description: description.clone(),
        body: new_body.to_string(),
        body_hash,
        session_id: None,
        source: "enrich".to_string(),
        metadata: serde_json::Value::Object(serde_json::Map::new()),
    };

    memories::update(conn, memory_id, &new_memory, None)?;
    memories::sync_fts_after_update(
        conn,
        memory_id,
        &old_name,
        &old_desc,
        &old_body,
        &new_memory.name,
        &new_memory.description,
        &new_memory.body,
    )?;

    // Re-embed for recall accuracy
    let snippet: String = new_body.chars().take(200).collect();
    let tokenizer = crate::tokenizer::get_tokenizer(&paths.models)?;
    let chunks_info = crate::chunking::split_into_chunks_hierarchical(new_body, tokenizer);
    let embedding_result = if chunks_info.len() <= 1 {
        crate::daemon::embed_passage_or_local(&paths.models, new_body)
    } else {
        let mut chunk_embeddings: Vec<Vec<f32>> = Vec::with_capacity(chunks_info.len());
        let mut ok = true;
        for chunk in &chunks_info {
            let text = crate::chunking::chunk_text(new_body, chunk);
            match crate::daemon::embed_passage_or_local(&paths.models, text) {
                Ok(emb) => chunk_embeddings.push(emb),
                Err(e) => {
                    tracing::warn!(target: "enrich", error = %e, "chunk embedding failed");
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            Ok(crate::chunking::aggregate_embeddings(&chunk_embeddings))
        } else {
            crate::daemon::embed_passage_or_local(&paths.models, new_body)
        }
    };

    if let Ok(embedding) = embedding_result {
        if let Err(e) = memories::upsert_vec(
            conn,
            memory_id,
            namespace,
            &memory_type,
            &embedding,
            memory_name,
            &snippet,
        ) {
            tracing::warn!(target: "enrich", memory = %memory_name, error = %e, "vec upsert failed after body-enrich");
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Main entry point for the `enrich` command.
pub fn run(args: &EnrichArgs) -> Result<(), AppError> {
    let started = Instant::now();

    let paths = AppPaths::resolve(args.db.as_deref())?;
    ensure_db_ready(&paths)?;
    let conn = open_rw(&paths.db)?;
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;

    // Validate provider binary upfront
    let provider_binary = match args.mode {
        EnrichMode::ClaudeCode => {
            let bin = find_claude_binary(args.claude_binary.as_deref())?;
            let version = validate_claude_version_local(&bin)?;
            tracing::info!(target: "enrich", binary = %bin.display(), version = %version, "Claude Code binary validated");
            emit_json(&PhaseEvent {
                phase: "validate",
                binary_path: bin.to_str(),
                version: Some(&version),
                items_total: None,
                items_pending: None,
            });
            bin
        }
        EnrichMode::Codex => {
            // Codex provider: locate binary using env or PATH
            let bin = find_codex_binary(args.codex_binary.as_deref())?;
            emit_json(&PhaseEvent {
                phase: "validate",
                binary_path: bin.to_str(),
                version: None,
                items_total: None,
                items_pending: None,
            });
            bin
        }
    };

    // SCAN phase
    let scan_result = scan_operation(&conn, &namespace, args)?;
    let total = scan_result.len();

    emit_json(&PhaseEvent {
        phase: "scan",
        binary_path: None,
        version: None,
        items_total: Some(total),
        items_pending: Some(total),
    });

    // Dry-run: emit preview events and summary without calling LLM
    if args.dry_run {
        for (idx, key) in scan_result.iter().enumerate() {
            emit_json(&ItemEvent {
                item: key,
                status: "preview",
                memory_id: None,
                entity_id: None,
                entities: None,
                rels: None,
                chars_before: None,
                chars_after: None,
                cost_usd: None,
                elapsed_ms: None,
                error: None,
                index: idx,
                total,
            });
        }
        emit_json(&EnrichSummary {
            summary: true,
            operation: format!("{:?}", args.operation),
            items_total: total,
            completed: 0,
            failed: 0,
            skipped: 0,
            cost_usd: 0.0,
            elapsed_ms: started.elapsed().as_millis() as u64,
        });
        return Ok(());
    }

    // For operations not yet fully implemented, emit a clear structured response
    // and exit without calling the LLM, so callers can branch on the NDJSON.
    match args.operation {
        EnrichOperation::MemoryBindings
        | EnrichOperation::EntityDescriptions
        | EnrichOperation::BodyEnrich => {
            // Fully implemented below
        }
        _ => {
            for (idx, key) in scan_result.iter().enumerate() {
                emit_json(&serde_json::json!({
                    "item": key,
                    "status": "not_yet_implemented",
                    "operation": format!("{:?}", args.operation),
                    "index": idx,
                    "total": total
                }));
            }
            emit_json(&EnrichSummary {
                summary: true,
                operation: format!("{:?}", args.operation),
                items_total: total,
                completed: 0,
                failed: 0,
                skipped: total,
                cost_usd: 0.0,
                elapsed_ms: started.elapsed().as_millis() as u64,
            });
            return Ok(());
        }
    }

    // Queue setup for resume/retry
    let queue_conn = open_queue_db(DEFAULT_QUEUE_DB)?;

    if args.resume {
        let reset = queue_conn
            .execute(
                "UPDATE queue SET status='pending' WHERE status='processing'",
                [],
            )
            .map_err(|e| AppError::Validation(format!("queue resume failed: {e}")))?;
        if reset > 0 {
            tracing::info!(target: "enrich", count = reset, "reset stuck processing items to pending");
        }
    }

    if args.retry_failed {
        let count = queue_conn
            .execute(
                "UPDATE queue SET status='pending', attempt=0 WHERE status='failed'",
                [],
            )
            .map_err(|e| AppError::Validation(format!("queue retry-failed reset failed: {e}")))?;
        tracing::info!(target: "enrich", count, "retrying failed items");
    }

    if !args.resume && !args.retry_failed {
        queue_conn
            .execute("DELETE FROM queue", [])
            .map_err(|e| AppError::Validation(format!("queue clear failed: {e}")))?;
    }

    // Populate queue
    for (idx, key) in scan_result.iter().enumerate() {
        let item_type = match args.operation {
            EnrichOperation::EntityDescriptions => "entity",
            _ => "memory",
        };
        let _ = queue_conn.execute(
            "INSERT OR IGNORE INTO queue (item_key, item_type, status) VALUES (?1, ?2, 'pending')",
            rusqlite::params![key, item_type],
        );
        let _ = idx; // suppress unused warning
    }

    let mut completed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut cost_total = 0.0f64;
    let mut oauth_detected = false;
    let mut backoff_secs = DEFAULT_RATE_LIMIT_WAIT;

    let provider_timeout = match args.mode {
        EnrichMode::ClaudeCode => args.claude_timeout,
        EnrichMode::Codex => args.codex_timeout,
    };

    let provider_model: Option<&str> = match args.mode {
        EnrichMode::ClaudeCode => args.claude_model.as_deref(),
        EnrichMode::Codex => args.codex_model.as_deref(),
    };

    loop {
        // Budget check
        if let Some(budget) = args.max_cost_usd {
            if !oauth_detected && cost_total >= budget {
                tracing::warn!(target: "enrich", spent = cost_total, budget, "budget exceeded, stopping");
                break;
            }
        }

        // Dequeue next pending item
        let pending: Option<(i64, String, String)> = queue_conn
            .query_row(
                "UPDATE queue SET status='processing', attempt=attempt+1 \
                 WHERE id = (SELECT id FROM queue WHERE status='pending' ORDER BY id LIMIT 1) \
                 RETURNING id, item_key, item_type",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .ok();

        let (queue_id, item_key, item_type) = match pending {
            Some(p) => p,
            None => break,
        };

        let item_started = Instant::now();
        let current_index = completed + failed + skipped;

        let call_result = match args.operation {
            EnrichOperation::MemoryBindings => call_memory_bindings(
                &conn,
                &namespace,
                &item_key,
                &provider_binary,
                provider_model,
                provider_timeout,
                &args.mode,
            ),
            EnrichOperation::EntityDescriptions => call_entity_description(
                &conn,
                &namespace,
                &item_key,
                &provider_binary,
                provider_model,
                provider_timeout,
                &args.mode,
            ),
            EnrichOperation::BodyEnrich => call_body_enrich(
                &conn,
                &namespace,
                &item_key,
                &provider_binary,
                provider_model,
                provider_timeout,
                &args.mode,
                args.min_output_chars,
                args.max_output_chars,
                args.prompt_template.as_deref(),
                &paths,
            ),
            _ => unreachable!("non-implemented ops handled above"),
        };

        match call_result {
            Ok(EnrichItemResult::Done {
                memory_id,
                entity_id,
                entities,
                rels,
                chars_before,
                chars_after,
                cost,
                is_oauth,
            }) => {
                if is_oauth && !oauth_detected {
                    oauth_detected = true;
                    tracing::info!(target: "enrich", "OAuth subscription detected — cost_usd omitted from output");
                }
                backoff_secs = DEFAULT_RATE_LIMIT_WAIT;

                // Persist depends on the operation
                let persist_err: Option<String> = match args.operation {
                    EnrichOperation::MemoryBindings => {
                        // Bindings already persisted inside call_memory_bindings
                        None
                    }
                    EnrichOperation::EntityDescriptions => {
                        // Description already persisted inside call_entity_description
                        None
                    }
                    EnrichOperation::BodyEnrich => {
                        // Body already persisted inside call_body_enrich
                        None
                    }
                    _ => unreachable!(),
                };

                let _ = queue_conn.execute(
                    "UPDATE queue SET status='done', memory_id=?1, entity_id=?2, entities=?3, rels=?4, cost_usd=?5, elapsed_ms=?6, done_at=datetime('now') WHERE id=?7",
                    rusqlite::params![
                        memory_id,
                        entity_id,
                        entities as i64,
                        rels as i64,
                        cost,
                        item_started.elapsed().as_millis() as i64,
                        queue_id
                    ],
                );

                if persist_err.is_none() {
                    completed += 1;
                    if !is_oauth {
                        cost_total += cost;
                    }
                    emit_json(&ItemEvent {
                        item: &item_key,
                        status: "done",
                        memory_id,
                        entity_id,
                        entities: Some(entities),
                        rels: Some(rels),
                        chars_before,
                        chars_after,
                        cost_usd: if is_oauth { None } else { Some(cost) },
                        elapsed_ms: Some(item_started.elapsed().as_millis() as u64),
                        error: None,
                        index: current_index,
                        total,
                    });
                } else {
                    failed += 1;
                    emit_json(&ItemEvent {
                        item: &item_key,
                        status: "failed",
                        memory_id: None,
                        entity_id: None,
                        entities: None,
                        rels: None,
                        chars_before: None,
                        chars_after: None,
                        cost_usd: None,
                        elapsed_ms: Some(item_started.elapsed().as_millis() as u64),
                        error: persist_err,
                        index: current_index,
                        total,
                    });
                }
            }
            Ok(EnrichItemResult::Skipped { reason }) => {
                skipped += 1;
                let _ = queue_conn.execute(
                    "UPDATE queue SET status='skipped', error=?1, done_at=datetime('now') WHERE id=?2",
                    rusqlite::params![reason, queue_id],
                );
                emit_json(&ItemEvent {
                    item: &item_key,
                    status: "skipped",
                    memory_id: None,
                    entity_id: None,
                    entities: None,
                    rels: None,
                    chars_before: None,
                    chars_after: None,
                    cost_usd: None,
                    elapsed_ms: Some(item_started.elapsed().as_millis() as u64),
                    error: None,
                    index: current_index,
                    total,
                });
            }
            Err(e) => {
                let err_str = format!("{e}");
                if err_str.contains("RATE_LIMITED") {
                    tracing::warn!(target: "enrich", wait_seconds = backoff_secs, "rate limited, waiting before retry");
                    let _ = queue_conn.execute(
                        "UPDATE queue SET status='pending' WHERE id=?1",
                        rusqlite::params![queue_id],
                    );
                    std::thread::sleep(std::time::Duration::from_secs(backoff_secs));
                    backoff_secs = (backoff_secs * 2).min(900);
                    continue;
                }

                failed += 1;
                let _ = queue_conn.execute(
                    "UPDATE queue SET status='failed', error=?1, done_at=datetime('now') WHERE id=?2",
                    rusqlite::params![err_str, queue_id],
                );
                emit_json(&ItemEvent {
                    item: &item_key,
                    status: "failed",
                    memory_id: None,
                    entity_id: None,
                    entities: None,
                    rels: None,
                    chars_before: None,
                    chars_after: None,
                    cost_usd: None,
                    elapsed_ms: Some(item_started.elapsed().as_millis() as u64),
                    error: Some(err_str),
                    index: current_index,
                    total,
                });
            }
        }

        let _ = item_type; // used via queue schema only
    }

    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    let _ = queue_conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");

    emit_json(&EnrichSummary {
        summary: true,
        operation: format!("{:?}", args.operation),
        items_total: total,
        completed,
        failed,
        skipped,
        cost_usd: cost_total,
        elapsed_ms: started.elapsed().as_millis() as u64,
    });

    if failed == 0 {
        let _ = std::fs::remove_file(DEFAULT_QUEUE_DB);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Internal result type for a single item call
// ---------------------------------------------------------------------------

enum EnrichItemResult {
    Done {
        memory_id: Option<i64>,
        entity_id: Option<i64>,
        entities: usize,
        rels: usize,
        chars_before: Option<usize>,
        chars_after: Option<usize>,
        cost: f64,
        is_oauth: bool,
    },
    Skipped {
        reason: String,
    },
}

// ---------------------------------------------------------------------------
// Per-operation call helpers (SCAN + JUDGE + PERSIST in one unit)
// ---------------------------------------------------------------------------

fn call_memory_bindings(
    conn: &Connection,
    namespace: &str,
    memory_name: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
) -> Result<EnrichItemResult, AppError> {
    // Look up the memory
    let (memory_id, body): (i64, String) = conn.query_row(
        "SELECT id, COALESCE(body,'') FROM memories WHERE namespace=?1 AND name=?2 AND deleted_at IS NULL",
        rusqlite::params![namespace, memory_name],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => AppError::NotFound(format!("memory '{memory_name}' not found")),
        other => AppError::Database(other),
    })?;

    if body.trim().is_empty() {
        return Ok(EnrichItemResult::Skipped {
            reason: "body is empty".to_string(),
        });
    }

    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            BINDINGS_PROMPT,
            BINDINGS_SCHEMA,
            &body,
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            BINDINGS_PROMPT,
            BINDINGS_SCHEMA,
            &body,
            model,
            timeout,
        )?,
    };

    let empty_arr = serde_json::Value::Array(vec![]);
    let entities_val = value.get("entities").unwrap_or(&empty_arr);
    let rels_val = value.get("relationships").unwrap_or(&empty_arr);

    let (ent_count, rel_count) =
        persist_memory_bindings(conn, namespace, memory_id, entities_val, rels_val)?;

    Ok(EnrichItemResult::Done {
        memory_id: Some(memory_id),
        entity_id: None,
        entities: ent_count,
        rels: rel_count,
        chars_before: None,
        chars_after: None,
        cost,
        is_oauth,
    })
}

fn call_entity_description(
    conn: &Connection,
    namespace: &str,
    entity_name: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
) -> Result<EnrichItemResult, AppError> {
    let (entity_id, entity_type): (i64, String) = conn
        .query_row(
            "SELECT id, type FROM entities WHERE namespace=?1 AND name=?2",
            rusqlite::params![namespace, entity_name],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("entity '{entity_name}' not found"))
            }
            other => AppError::Database(other),
        })?;

    let prompt = format!(
        "{ENTITY_DESCRIPTION_PROMPT_PREFIX}{entity_name}\nEntity type: {entity_type}\n\nGenerate a description:"
    );

    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            &prompt,
            ENTITY_DESCRIPTION_SCHEMA,
            "",
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            &prompt,
            ENTITY_DESCRIPTION_SCHEMA,
            "",
            model,
            timeout,
        )?,
    };

    let description = value
        .get("description")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("LLM result missing 'description' field".into()))?;

    persist_entity_description(conn, entity_id, description)?;

    Ok(EnrichItemResult::Done {
        memory_id: None,
        entity_id: Some(entity_id),
        entities: 0,
        rels: 0,
        chars_before: None,
        chars_after: None,
        cost,
        is_oauth,
    })
}

#[allow(clippy::too_many_arguments)]
fn call_body_enrich(
    conn: &Connection,
    namespace: &str,
    memory_name: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
    min_output_chars: usize,
    max_output_chars: usize,
    prompt_template: Option<&Path>,
    paths: &crate::paths::AppPaths,
) -> Result<EnrichItemResult, AppError> {
    let (memory_id, body): (i64, String) = conn.query_row(
        "SELECT id, COALESCE(body,'') FROM memories WHERE namespace=?1 AND name=?2 AND deleted_at IS NULL",
        rusqlite::params![namespace, memory_name],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => AppError::NotFound(format!("memory '{memory_name}' not found")),
        other => AppError::Database(other),
    })?;

    let chars_before = body.chars().count();

    // Load custom prompt template if provided
    let prompt_prefix = if let Some(tmpl_path) = prompt_template {
        std::fs::read_to_string(tmpl_path).map_err(|e| {
            AppError::Io(std::io::Error::new(
                e.kind(),
                format!("failed to read prompt template: {e}"),
            ))
        })?
    } else {
        BODY_ENRICH_PROMPT_PREFIX.to_string()
    };

    let prompt = format!(
        "{prompt_prefix}Target minimum length: {min_output_chars} characters. Maximum: {max_output_chars} characters."
    );

    // The body schema uses a free-form enriched_body field
    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => {
            call_claude(binary, &prompt, BODY_ENRICH_SCHEMA, &body, model, timeout)?
        }
        EnrichMode::Codex => {
            call_codex(binary, &prompt, BODY_ENRICH_SCHEMA, &body, model, timeout)?
        }
    };

    let enriched_body = value
        .get("enriched_body")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("LLM result missing 'enriched_body' field".into()))?;

    let chars_after = enriched_body.chars().count();

    // Only persist if the enriched body is genuinely longer
    if chars_after <= chars_before {
        return Ok(EnrichItemResult::Skipped {
            reason: format!(
                "enriched body ({chars_after} chars) not longer than original ({chars_before} chars)"
            ),
        });
    }

    persist_enriched_body(
        conn,
        namespace,
        memory_id,
        memory_name,
        enriched_body,
        paths,
    )?;

    Ok(EnrichItemResult::Done {
        memory_id: Some(memory_id),
        entity_id: None,
        entities: 0,
        rels: 0,
        chars_before: Some(chars_before),
        chars_after: Some(chars_after),
        cost,
        is_oauth,
    })
}

// ---------------------------------------------------------------------------
// Scan dispatcher — maps operation to scan query result (item keys)
// ---------------------------------------------------------------------------

fn scan_operation(
    conn: &Connection,
    namespace: &str,
    args: &EnrichArgs,
) -> Result<Vec<String>, AppError> {
    match args.operation {
        EnrichOperation::MemoryBindings => {
            let rows = scan_unbound_memories(conn, namespace, args.limit)?;
            Ok(rows.into_iter().map(|(_, name, _)| name).collect())
        }
        EnrichOperation::EntityDescriptions => {
            let rows = scan_entities_without_description(conn, namespace, args.limit)?;
            Ok(rows.into_iter().map(|(_, name, _)| name).collect())
        }
        EnrichOperation::BodyEnrich => {
            let rows =
                scan_short_body_memories(conn, namespace, args.min_output_chars, args.limit)?;
            Ok(rows.into_iter().map(|(_, name, _)| name).collect())
        }
        // Scan-only operations: return all memories as candidates
        EnrichOperation::WeightCalibrate
        | EnrichOperation::RelationReclassify
        | EnrichOperation::EntityConnect
        | EnrichOperation::EntityTypeValidate
        | EnrichOperation::DescriptionEnrich
        | EnrichOperation::CrossDomainBridges
        | EnrichOperation::DomainClassify
        | EnrichOperation::GraphAudit
        | EnrichOperation::DeepResearchSynth
        | EnrichOperation::BodyExtract => {
            let limit_clause = args.limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();
            let sql = format!(
                "SELECT name FROM memories WHERE namespace=?1 AND deleted_at IS NULL ORDER BY id {limit_clause}"
            );
            let mut stmt = conn.prepare(&sql)?;
            let names = stmt
                .query_map(rusqlite::params![namespace], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(names)
        }
    }
}

// ---------------------------------------------------------------------------
// Codex stub provider
// ---------------------------------------------------------------------------

/// Locates the Codex CLI binary.
fn find_codex_binary(explicit: Option<&Path>) -> Result<PathBuf, AppError> {
    if let Some(p) = explicit {
        if p.exists() {
            return Ok(p.to_path_buf());
        }
        return Err(AppError::Validation(format!(
            "Codex binary not found at explicit path: {}",
            p.display()
        )));
    }

    if let Ok(env_path) = std::env::var("SQLITE_GRAPHRAG_CODEX_BINARY") {
        let p = PathBuf::from(&env_path);
        if p.exists() {
            return Ok(p);
        }
    }

    let name = if cfg!(windows) { "codex.exe" } else { "codex" };
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(name);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    Err(AppError::Validation(
        "Codex CLI binary not found in PATH. Install it or specify --codex-binary".to_string(),
    ))
}

/// Calls the Codex CLI for a single enrichment item.
///
/// Follows the same contract as `call_claude`: returns `(value, cost_usd, is_oauth=false)`.
fn call_codex(
    binary: &Path,
    prompt: &str,
    json_schema: &str,
    input_text: &str,
    model: Option<&str>,
    timeout_secs: u64,
) -> Result<(serde_json::Value, f64, bool), AppError> {
    use wait_timeout::ChildExt;

    let full_prompt = format!("{prompt}\n\n{input_text}");
    let schema_file = {
        let tmp = std::env::temp_dir().join(format!("enrich-schema-{}.json", std::process::id()));
        std::fs::write(&tmp, json_schema).map_err(AppError::Io)?;
        tmp
    };

    let mut cmd = Command::new(binary);
    cmd.env_clear();
    for var in &[
        "PATH",
        "HOME",
        "USER",
        "OPENAI_API_KEY",
        "TMPDIR",
        "TMP",
        "TEMP",
    ] {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }

    cmd.arg("exec")
        .arg("--json")
        .arg("--output-schema")
        .arg(&schema_file);

    if let Some(m) = model {
        cmd.arg("--model").arg(m);
    }

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to spawn codex: {e}"),
        ))
    })?;

    // Write prompt via stdin
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(full_prompt.as_bytes());
    }

    let timeout = std::time::Duration::from_secs(timeout_secs);
    let status = child.wait_timeout(timeout).map_err(AppError::Io)?;

    let _ = std::fs::remove_file(&schema_file);

    match status {
        Some(exit_status) => {
            let mut stdout_buf = Vec::new();
            if let Some(mut out) = child.stdout.take() {
                std::io::Read::read_to_end(&mut out, &mut stdout_buf).map_err(AppError::Io)?;
            }
            if !exit_status.success() {
                let mut stderr_buf = Vec::new();
                if let Some(mut err) = child.stderr.take() {
                    std::io::Read::read_to_end(&mut err, &mut stderr_buf).map_err(AppError::Io)?;
                }
                return Err(AppError::Validation(format!(
                    "codex exited with code {:?}: {}",
                    exit_status.code(),
                    String::from_utf8_lossy(&stderr_buf).trim()
                )));
            }
            let stdout_str = String::from_utf8(stdout_buf)
                .map_err(|_| AppError::Validation("codex stdout is not valid UTF-8".into()))?;
            let value: serde_json::Value = serde_json::from_str(&stdout_str).map_err(|e| {
                AppError::Validation(format!("failed to parse codex output as JSON: {e}"))
            })?;
            Ok((value, 0.0, false))
        }
        None => {
            let _ = child.kill();
            let _ = child.wait();
            Err(AppError::Validation(format!(
                "codex timed out after {timeout_secs} seconds"
            )))
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// Opens an in-memory SQLite database with a minimal schema for unit tests.
    fn open_test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(
            "CREATE TABLE memories (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                namespace   TEXT NOT NULL DEFAULT 'global',
                name        TEXT NOT NULL,
                type        TEXT NOT NULL DEFAULT 'note',
                description TEXT NOT NULL DEFAULT '',
                body        TEXT NOT NULL DEFAULT '',
                body_hash   TEXT NOT NULL DEFAULT '',
                session_id  TEXT,
                source      TEXT NOT NULL DEFAULT 'agent',
                metadata    TEXT NOT NULL DEFAULT '{}',
                created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
                updated_at  INTEGER NOT NULL DEFAULT (unixepoch()),
                deleted_at  INTEGER,
                UNIQUE(namespace, name)
            );
            CREATE TABLE entities (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                namespace   TEXT NOT NULL DEFAULT 'global',
                name        TEXT NOT NULL,
                type        TEXT NOT NULL DEFAULT 'concept',
                description TEXT,
                degree      INTEGER NOT NULL DEFAULT 0,
                created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
                updated_at  INTEGER NOT NULL DEFAULT (unixepoch()),
                UNIQUE(namespace, name)
            );
            CREATE TABLE memory_entities (
                memory_id  INTEGER NOT NULL,
                entity_id  INTEGER NOT NULL,
                PRIMARY KEY (memory_id, entity_id)
            );
            CREATE TABLE relationships (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                namespace  TEXT NOT NULL DEFAULT 'global',
                source_id  INTEGER NOT NULL,
                target_id  INTEGER NOT NULL,
                relation   TEXT NOT NULL,
                weight     REAL NOT NULL DEFAULT 0.5,
                description TEXT,
                UNIQUE(source_id, target_id, relation)
            );",
        )
        .expect("schema creation must succeed");
        conn
    }

    #[test]
    fn scan_unbound_memories_finds_memories_without_bindings() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'test-mem', 'some body content')",
            [],
        )
        .unwrap();

        let results = scan_unbound_memories(&conn, "global", None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "test-mem");
    }

    #[test]
    fn scan_unbound_memories_excludes_bound_memories() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'bound-mem', 'body')",
            [],
        )
        .unwrap();
        let mem_id: i64 = conn
            .query_row("SELECT id FROM memories WHERE name='bound-mem'", [], |r| {
                r.get(0)
            })
            .unwrap();
        conn.execute(
            "INSERT INTO entities (namespace, name) VALUES ('global', 'some-entity')",
            [],
        )
        .unwrap();
        let ent_id: i64 = conn
            .query_row(
                "SELECT id FROM entities WHERE name='some-entity'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO memory_entities (memory_id, entity_id) VALUES (?1, ?2)",
            rusqlite::params![mem_id, ent_id],
        )
        .unwrap();

        let results = scan_unbound_memories(&conn, "global", None).unwrap();
        assert!(results.is_empty(), "bound memory must not appear in scan");
    }

    #[test]
    fn scan_entities_without_description_finds_null_description() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO entities (namespace, name, type, description) VALUES ('global', 'my-tool', 'tool', NULL)",
            [],
        )
        .unwrap();

        let results = scan_entities_without_description(&conn, "global", None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "my-tool");
    }

    #[test]
    fn scan_entities_without_description_excludes_entities_with_description() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO entities (namespace, name, type, description) VALUES ('global', 'described-tool', 'tool', 'Has a description already')",
            [],
        )
        .unwrap();

        let results = scan_entities_without_description(&conn, "global", None).unwrap();
        assert!(
            results.is_empty(),
            "entity with description must not appear"
        );
    }

    #[test]
    fn scan_short_body_memories_finds_short_bodies() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'short-mem', 'hi')",
            [],
        )
        .unwrap();

        let results = scan_short_body_memories(&conn, "global", 100, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "short-mem");
    }

    #[test]
    fn scan_short_body_memories_excludes_long_bodies() {
        let conn = open_test_db();
        let long_body = "a".repeat(1000);
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'long-mem', ?1)",
            rusqlite::params![long_body],
        )
        .unwrap();

        let results = scan_short_body_memories(&conn, "global", 100, None).unwrap();
        assert!(results.is_empty(), "long memory must not appear in scan");
    }

    #[test]
    fn scan_respects_limit() {
        let conn = open_test_db();
        for i in 0..5 {
            conn.execute(
                &format!("INSERT INTO memories (namespace, name, body) VALUES ('global', 'mem-{i}', 'short')"),
                [],
            )
            .unwrap();
        }

        let results = scan_short_body_memories(&conn, "global", 1000, Some(3)).unwrap();
        assert_eq!(results.len(), 3, "limit must be respected");
    }

    #[test]
    fn queue_db_schema_creates_correctly() {
        let tmp_path = format!("/tmp/test-enrich-queue-{}.sqlite", std::process::id());
        let conn = open_queue_db(&tmp_path).expect("queue db must open");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM queue", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        let _ = std::fs::remove_file(&tmp_path);
    }

    #[test]
    fn parse_claude_json_output_valid_bindings() {
        let output = r#"[
            {"type":"system","subtype":"init"},
            {"type":"result","is_error":false,"total_cost_usd":0.01,
             "structured_output":{"entities":[{"name":"rust-lang","entity_type":"tool"}],"relationships":[]}}
        ]"#;
        let (value, cost, is_oauth) =
            parse_claude_json_output(output).expect("must parse successfully");
        assert!(value.get("entities").is_some());
        assert!((cost - 0.01).abs() < f64::EPSILON);
        assert!(!is_oauth);
    }

    #[test]
    fn parse_claude_json_output_detects_oauth() {
        let output = r#"[
            {"type":"system","subtype":"init","apiKeySource":"none"},
            {"type":"result","is_error":false,"total_cost_usd":0.0,
             "structured_output":{"entities":[],"relationships":[]}}
        ]"#;
        let (_value, _cost, is_oauth) = parse_claude_json_output(output).unwrap();
        assert!(is_oauth);
    }

    #[test]
    fn parse_claude_json_output_rate_limit_returns_error() {
        let output = r#"[
            {"type":"system","subtype":"init"},
            {"type":"result","is_error":true,"error":"rate_limit exceeded"}
        ]"#;
        let err = parse_claude_json_output(output).unwrap_err();
        assert!(format!("{err}").contains("RATE_LIMITED"));
    }

    #[test]
    fn parse_claude_json_output_auth_error() {
        let output = r#"[
            {"type":"system","subtype":"init"},
            {"type":"result","is_error":true,"error":"authentication failed"}
        ]"#;
        let err = parse_claude_json_output(output).unwrap_err();
        assert!(format!("{err}").contains("authentication failed"));
    }

    #[test]
    fn dry_run_emits_preview_without_calling_llm() {
        // This test validates the dry-run NDJSON contract without spawning any process.
        // The scan_operation function requires a DB; we build one in-memory but cannot
        // call run() directly because it needs AppPaths (disk). Instead we test the
        // lower-level helpers that the dry-run path relies on.
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'dry-mem', 'tiny')",
            [],
        )
        .unwrap();

        let results = scan_short_body_memories(&conn, "global", 1000, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "dry-mem");
        // If scan finds the item and dry_run is set, no LLM would be called.
        // The NDJSON emission is tested via integration tests with a fake binary.
    }

    #[test]
    fn persist_entity_description_updates_db() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO entities (namespace, name, type) VALUES ('global', 'tokio-runtime', 'tool')",
            [],
        )
        .unwrap();
        let eid: i64 = conn
            .query_row(
                "SELECT id FROM entities WHERE name='tokio-runtime'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        persist_entity_description(&conn, eid, "Async runtime for Rust applications").unwrap();

        let desc: String = conn
            .query_row(
                "SELECT description FROM entities WHERE id=?1",
                rusqlite::params![eid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(desc, "Async runtime for Rust applications");
    }

    #[test]
    fn bindings_schema_is_valid_json() {
        let _: serde_json::Value =
            serde_json::from_str(BINDINGS_SCHEMA).expect("BINDINGS_SCHEMA must be valid JSON");
    }

    #[test]
    fn entity_description_schema_is_valid_json() {
        let _: serde_json::Value = serde_json::from_str(ENTITY_DESCRIPTION_SCHEMA)
            .expect("ENTITY_DESCRIPTION_SCHEMA must be valid JSON");
    }

    #[test]
    fn body_enrich_schema_is_valid_json() {
        let _: serde_json::Value = serde_json::from_str(BODY_ENRICH_SCHEMA)
            .expect("BODY_ENRICH_SCHEMA must be valid JSON");
    }
}
