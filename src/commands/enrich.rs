//! Handler for the `enrich` CLI subcommand (GAP-14 + GAP-18).
//!
//! Enriches the knowledge graph by running LLM-powered analysis over memories
//! and entities that are missing key structural data. Operations are:
//!
//! - `memory-bindings`: memories without `memory_entities` rows get entity extraction
//! - `entity-descriptions`: entities with NULL/empty descriptions get LLM descriptions
//! - `body-enrich`: memories with short bodies get expanded by the LLM (GAP-18)
//! - `re-embed`: memories without a vector row get re-embedded without rewriting body
//!
//! Architecture mirrors `ingest_claude.rs`: SCAN → JUDGE (LLM) → PERSIST, with a
//! SQLite queue DB (`.enrich-queue.sqlite`) for resume/retry support.
// Workload: Subprocess I/O-bound (claude/codex API calls with network wait)
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
use crate::constants::MAX_MEMORY_BODY_LEN;
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
use std::time::Instant;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

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

// G27 P1: weight-calibrate
const WEIGHT_CALIBRATE_PROMPT: &str = "You are a knowledge graph quality auditor. Evaluate whether this relationship weight is correctly calibrated.\n\n\
Scale:\n\
- 0.9 = vital hard dependency (A cannot function without B)\n\
- 0.7 = important design relationship (A strongly supports/enables B)\n\
- 0.5 = useful contextual link (A and B share relevant context)\n\
- 0.3 = weak reference (A mentions B without strong coupling)\n\n\
Respond with the calibrated weight and brief reasoning.";

const WEIGHT_CALIBRATE_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "calibrated_weight": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
    "reasoning": { "type": "string" }
  },
  "required": ["calibrated_weight", "reasoning"],
  "additionalProperties": false
}"#;

// G27 P1: relation-reclassify
const RELATION_RECLASSIFY_PROMPT: &str = "You are a knowledge graph quality auditor. The relationship between these entities uses a generic type. Determine the REAL semantic relationship.\n\n\
Valid canonical relations (pick exactly one):\n\
- depends-on: A cannot function without B\n\
- uses: A utilizes B but could substitute it\n\
- supports: A reinforces or enables B\n\
- causes: A triggers or produces B\n\
- fixes: A resolves a problem in B\n\
- contradicts: A conflicts with or invalidates B\n\
- applies-to: A is relevant to or scoped within B\n\
- follows: A comes after B in sequence\n\
- replaces: A substitutes B\n\
- tracked-in: A is monitored in B\n\
- related: A and B share context (use sparingly)\n\n\
Respond with the correct relation, strength, and reasoning.";

const RELATION_RECLASSIFY_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "relation": { "type": "string" },
    "strength": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
    "reasoning": { "type": "string" }
  },
  "required": ["relation", "strength", "reasoning"],
  "additionalProperties": false
}"#;

// G27 P2: entity-connect — suggest relationships between isolated entities
const ENTITY_CONNECT_PROMPT: &str = "You are a knowledge graph quality auditor. Two entities exist in the same graph but have no relationship between them. Determine if a meaningful relationship exists.\n\n\
Valid canonical relations: depends-on, uses, supports, causes, fixes, contradicts, applies-to, follows, replaces, tracked-in, related.\n\n\
If NO meaningful relationship exists, set relation to \"none\".\n\
Respond with the relation (or \"none\"), strength, and reasoning.";

const ENTITY_CONNECT_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "relation": { "type": "string" },
    "strength": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
    "reasoning": { "type": "string" }
  },
  "required": ["relation", "strength", "reasoning"],
  "additionalProperties": false
}"#;

// G27 P2: entity-type-validate — verify entity type assignments
const ENTITY_TYPE_VALIDATE_PROMPT: &str = "You are a knowledge graph quality auditor. Verify whether this entity's type is correct.\n\n\
Valid entity types: project, tool, person, file, concept, incident, decision, organization, location, date.\n\n\
If the current type is correct, keep it. If wrong, suggest the correct type.\n\
Respond with the validated type and reasoning.";

const ENTITY_TYPE_VALIDATE_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "validated_type": { "type": "string" },
    "was_correct": { "type": "boolean" },
    "reasoning": { "type": "string" }
  },
  "required": ["validated_type", "was_correct", "reasoning"],
  "additionalProperties": false
}"#;

// G27 P2: description-enrich — improve generic memory descriptions
const DESCRIPTION_ENRICH_PROMPT: &str = "You are a knowledge graph quality auditor. This memory has a generic or auto-generated description. Write a concise, semantic description (10-20 words) that captures WHAT this memory is about and WHY it matters.\n\n\
BAD: 'ingested from docs/auth.md'\n\
GOOD: 'JWT token rotation strategy with 15-min expiry and refresh flow'\n\n\
Respond with the improved description and reasoning.";

const DESCRIPTION_ENRICH_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "description": { "type": "string" },
    "reasoning": { "type": "string" }
  },
  "required": ["description", "reasoning"],
  "additionalProperties": false
}"#;

// G27 P2: domain-classify — classify memory into domain category
const DOMAIN_CLASSIFY_PROMPT: &str = "You are a knowledge graph quality auditor. Classify this memory into its primary domain category.\n\n\
Respond with the domain name (kebab-case, 2-4 words) and reasoning.";

const DOMAIN_CLASSIFY_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "domain": { "type": "string" },
    "confidence": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
    "reasoning": { "type": "string" }
  },
  "required": ["domain", "confidence", "reasoning"],
  "additionalProperties": false
}"#;

// G27 P2: graph-audit — audit graph for quality issues
const GRAPH_AUDIT_PROMPT: &str = "You are a knowledge graph quality auditor. Analyze this memory and its entity bindings for quality issues.\n\n\
Check for: missing entities, wrong entity types, redundant relationships, orphaned entities, generic descriptions, low-signal relationships.\n\n\
Respond with a list of issues found (or empty if none) and an overall quality score.";

const GRAPH_AUDIT_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "quality_score": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
    "issues": { "type": "array", "items": { "type": "object", "properties": { "kind": { "type": "string" }, "detail": { "type": "string" } }, "required": ["kind", "detail"] } },
    "reasoning": { "type": "string" }
  },
  "required": ["quality_score", "issues", "reasoning"],
  "additionalProperties": false
}"#;

// G27 P2: deep-research-synth — synthesize research findings into graph
const DEEP_RESEARCH_SYNTH_PROMPT: &str = "You are a knowledge graph synthesizer. Given this memory body, extract key findings and synthesize them into structured entities and relationships.\n\n\
Entity names: lowercase kebab-case, domain-specific.\n\
Relations: depends-on, uses, supports, causes, fixes, contradicts, applies-to, follows, related, replaces, tracked-in.\n\n\
Respond with extracted entities, relationships, and a synthesis summary.";

const DEEP_RESEARCH_SYNTH_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "entities": { "type": "array", "items": { "type": "object", "properties": { "name": { "type": "string" }, "entity_type": { "type": "string" } }, "required": ["name", "entity_type"] } },
    "relationships": { "type": "array", "items": { "type": "object", "properties": { "source": { "type": "string" }, "target": { "type": "string" }, "relation": { "type": "string" }, "strength": { "type": "number" } }, "required": ["source", "target", "relation", "strength"] } },
    "summary": { "type": "string" }
  },
  "required": ["entities", "relationships", "summary"],
  "additionalProperties": false
}"#;

// G27 P2: body-extract — extract structured content from unstructured text
const BODY_EXTRACT_PROMPT: &str = "You are a structured data extractor. Given this memory body (which may be unstructured text, raw notes, or a transcript), extract and restructure the content into a clean, well-organized markdown body.\n\n\
Preserve all factual content. Remove noise, fix formatting, add section headers where appropriate.\n\
Respond with the restructured body and a brief summary of changes.";

const BODY_EXTRACT_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "restructured_body": { "type": "string" },
    "changes_summary": { "type": "string" }
  },
  "required": ["restructured_body", "changes_summary"],
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
    /// Rebuild missing memory embeddings without rewriting the memory body.
    ReEmbed,
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
    # Rebuild only missing memory embeddings without rewriting bodies\n  \
    sqlite-graphrag enrich --operation re-embed --limit 100\n\n  \
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

    /// Number of parallel LLM workers (default 1 = serial).
    /// Each worker claims items atomically from the queue DB via UPDATE...RETURNING.
    /// Range: 1–32. For 2321 entities, --llm-parallelism 4 reduces wall time ~4×.
    #[arg(long, default_value_t = 1, value_name = "N", value_parser = clap::value_parser!(u32).range(1..=32))]
    pub llm_parallelism: u32,

    // -- Output / infra --
    /// Emit NDJSON output. Always true; flag accepted for compatibility.
    #[arg(long)]
    pub json: bool,

    /// Database path override.
    #[arg(long, env = "SQLITE_GRAPHRAG_DB_PATH")]
    pub db: Option<String>,

    /// G30: poll for the job singleton every second for up to N seconds
    /// when another invocation holds the lock. Default: 0 (fail fast).
    #[arg(long, value_name = "SECONDS")]
    pub wait_job_singleton: Option<u64>,

    /// G30: force acquisition of the singleton lock by removing a stale
    /// lock file from a previously crashed invocation. Use only when you
    /// are certain no other `enrich`/`ingest` is running.
    #[arg(long, default_value_t = false)]
    pub force_job_singleton: bool,

    /// G37: select a specific subset of memory names to enrich instead of
    /// the full candidate set. Comma-separated, e.g. `--names a,b,c`.
    /// Empty when omitted (processes all candidates).
    #[arg(long, value_name = "NAMES", value_delimiter = ',')]
    pub names: Vec<String>,

    /// G37: read the subset of memory names from a file (one per line).
    /// Lines starting with `#` and empty lines are ignored. Combined with
    /// `--names` (union) when both are set.
    #[arg(long, value_name = "PATH")]
    pub names_file: Option<PathBuf>,

    /// G35: probe the LLM provider with a 1-turn ping before processing
    /// the batch. Aborts with a clear error if the rate-limit window is
    /// closed (avoids burning N turns only to fail on item 1).
    #[arg(long, default_value_t = false)]
    pub preflight_check: bool,

    /// G35: if a preflight probe or in-flight call hits the Claude rate
    /// limit, fall back to `--fallback-mode` (typically `codex`) instead
    /// of failing the batch. Ignored when `--mode` is already `codex`.
    #[arg(long, value_enum)]
    pub fallback_mode: Option<EnrichMode>,

    /// G35: number of seconds before the OAuth rate-limit reset at which
    /// the preflight probe should refuse to start. Default 300 (5 min).
    #[arg(long, value_name = "SECONDS", default_value_t = 300)]
    pub rate_limit_buffer: u64,

    /// G28-D: refuse to start when the 1-minute load average exceeds
    /// `2 × ncpus` (or `SQLITE_GRAPHRAG_MAX_LOAD_PER_NCPU` if set).
    /// Set to false to skip the check on contended CI runners.
    #[arg(long, default_value_t = true)]
    pub max_load_check: bool,

    /// G28-D: when the system is saturated, abort the job after this
    /// many consecutive HardFailure outcomes. Default 5.
    #[arg(long, value_name = "N", default_value_t = 5)]
    pub circuit_breaker_threshold: u32,

    /// G29 Passo 4: minimum trigram-Jaccard similarity between the
    /// original body and the LLM-rewritten body for the rewrite to be
    /// accepted. Scores below the threshold are rejected and emitted as
    /// `EnrichItemResult::PreservationFailed`. Default 0.7 (per the G29
    /// gap specification). Ignored when `--operation` is not
    /// `body-enrich`.
    #[arg(long, value_name = "FLOAT", default_value_t = 0.7)]
    pub preserve_threshold: f64,

    /// G33 Passo 3: when set, validate `--codex-model` against the
    /// ChatGPT Pro OAuth accepted-model list and abort with a
    /// suggestion when the value is unknown. Default true (fail fast
    /// to avoid burning OAuth turns). Set to false to opt out.
    #[arg(long, default_value_t = true)]
    pub codex_model_validate: bool,

    /// G33 Passo 3: when set together with an invalid `--codex-model`,
    /// automatically substitute the supplied default (e.g. `gpt-5.5`)
    /// instead of aborting. The substitution is recorded in the NDJSON
    /// stream as `provider_substituted: true` for traceability.
    #[arg(long, value_name = "MODEL")]
    pub codex_model_fallback: Option<String>,
}

// ---------------------------------------------------------------------------
// Internal types — raw LLM output structs
// ---------------------------------------------------------------------------

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
    /// Active parallel LLM worker count (1 = serial). Present only on the "scan" phase event.
    #[serde(skip_serializing_if = "Option::is_none")]
    llm_parallelism: Option<u32>,
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
    /// v1.0.84 (ADR-0042): discriminador do backend LLM que efetivamente
    /// executou o re-embedding durante o enrich. `"claude" | "codex" | "none"`.
    /// Absent on the wire when `None` (kept for happy-path envelope cleanliness,
    /// ou quando a operação não envolveu re-embed).
    #[serde(skip_serializing_if = "Option::is_none")]
    backend_invoked: Option<&'static str>,
}

use crate::output::emit_json_line as emit_json;

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
// LLM invocation — Claude Code
// ---------------------------------------------------------------------------

/// Calls `claude -p` via the shared `claude_runner` module (G02).
///
/// Returns `(output_value, cost_usd, is_oauth)`.
fn call_claude(
    binary: &Path,
    prompt: &str,
    json_schema: &str,
    input_text: &str,
    model: Option<&str>,
    timeout_secs: u64,
) -> Result<(serde_json::Value, f64, bool), AppError> {
    let result = crate::commands::claude_runner::run_claude(
        binary,
        prompt,
        json_schema,
        input_text,
        model,
        timeout_secs,
        7,
    )?;
    Ok((result.value, result.cost_usd, result.is_oauth))
}

// ---------------------------------------------------------------------------
// Preflight probe (G35) — single-turn ping to verify the LLM provider
// ---------------------------------------------------------------------------

/// Result of a single preflight ping (G35).
enum PreflightOutcome {
    /// The provider accepted the ping without rate-limit or other errors.
    Healthy,
    /// The provider rejected the ping due to OAuth rate limit. The
    /// `suggestion` field is a human hint that callers can embed in the
    /// user-facing error.
    RateLimited {
        reason: String,
        suggestion: &'static str,
    },
    /// Any other provider error (binary missing, auth failure, etc.).
    Error(AppError),
}

/// Probes the configured LLM provider with a 1-turn ping.
///
/// - Claude: `claude -p "ping" --max-turns 1 --strict-mcp-config --mcp-config '{}'`
/// - Codex:  `codex exec -c mcp_servers='{}' "ping" --json`
///
/// The probe intentionally avoids spawning any MCP server children (G28-A)
/// to keep its own process footprint at the minimum.
fn run_preflight_probe(args: &EnrichArgs) -> PreflightOutcome {
    let timeout = std::time::Duration::from_secs(args.rate_limit_buffer.max(60));

    match args.mode {
        EnrichMode::ClaudeCode => {
            let bin = match find_claude_binary(args.claude_binary.as_deref()) {
                Ok(b) => b,
                Err(e) => return PreflightOutcome::Error(e),
            };
            let mut cmd = std::process::Command::new(&bin);
            cmd.env_clear();
            for var in &["PATH", "HOME", "USER"] {
                if let Ok(val) = std::env::var(var) {
                    cmd.env(var, val);
                }
            }
            cmd.arg("-p")
                .arg("ping")
                .arg("--max-turns")
                .arg("1")
                .arg("--strict-mcp-config")
                .arg("--mcp-config")
                .arg("{}")
                .arg("--dangerously-skip-permissions")
                .arg("--settings")
                .arg("{\"hooks\":{}}")
                .arg("--output-format")
                .arg("json")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());

            let child = match super::claude_runner::spawn_with_memory_limit(&mut cmd) {
                Ok(c) => c,
                Err(e) => {
                    return PreflightOutcome::Error(AppError::Io(e));
                }
            };
            let output = match wait_with_timeout(child, timeout) {
                Ok(out) => out,
                Err(e) => return PreflightOutcome::Error(e),
            };
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("hit your session limit")
                    || stderr.contains("rate_limit")
                    || stderr.contains("429")
                {
                    return PreflightOutcome::RateLimited {
                        reason: stderr.trim().to_string(),
                        suggestion:
                            "wait for the OAuth window to reset or use --fallback-mode codex",
                    };
                }
                return PreflightOutcome::Error(AppError::Validation(format!(
                    "preflight probe failed: {stderr}",
                    stderr = stderr.trim()
                )));
            }
            PreflightOutcome::Healthy
        }
        EnrichMode::Codex => {
            let bin = match find_codex_binary(args.codex_binary.as_deref()) {
                Ok(b) => b,
                Err(e) => return PreflightOutcome::Error(e),
            };
            super::codex_spawn::validate_codex_model(args.codex_model.as_deref())
                .map_err(PreflightOutcome::Error)
                .ok();
            let schema = "{}";
            let schema_path = match super::codex_spawn::trusted_schema_path() {
                Ok(p) => p,
                Err(e) => return PreflightOutcome::Error(e),
            };
            let spawn_args = super::codex_spawn::CodexSpawnArgs {
                binary: &bin,
                prompt: "ping",
                json_schema: schema,
                input_text: "",
                model: args.codex_model.as_deref(),
                timeout_secs: args.rate_limit_buffer.max(60),
                schema_path: schema_path.clone(),
            };
            let mut cmd = super::codex_spawn::build_codex_command(&spawn_args);
            let child = match super::claude_runner::spawn_with_memory_limit(&mut cmd) {
                Ok(c) => c,
                Err(e) => return PreflightOutcome::Error(AppError::Io(e)),
            };
            let output = match wait_with_timeout(child, timeout) {
                Ok(out) => out,
                Err(e) => return PreflightOutcome::Error(e),
            };
            let _ = std::fs::remove_file(&schema_path);
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("rate_limit")
                    || stderr.contains("429")
                    || stderr.contains("Too Many Requests")
                {
                    return PreflightOutcome::RateLimited {
                        reason: stderr.trim().to_string(),
                        suggestion: "wait for the rate-limit window to reset",
                    };
                }
                return PreflightOutcome::Error(AppError::Validation(format!(
                    "preflight probe failed: {stderr}",
                    stderr = stderr.trim()
                )));
            }
            PreflightOutcome::Healthy
        }
    }
}

/// Cross-platform wait with timeout (no extra crate dependency).
fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: std::time::Duration,
) -> Result<std::process::Output, AppError> {
    use wait_timeout::ChildExt;
    let start = std::time::Instant::now();
    let status = child.wait_timeout(timeout).map_err(AppError::Io)?;
    if status.is_none() {
        let _ = child.kill();
        let _ = child.wait();
        return Err(AppError::Validation(format!(
            "preflight probe timed out after {}s",
            start.elapsed().as_secs()
        )));
    }
    let mut stdout = Vec::new();
    if let Some(mut out) = child.stdout.take() {
        std::io::Read::read_to_end(&mut out, &mut stdout).map_err(AppError::Io)?;
    }
    let mut stderr = Vec::new();
    if let Some(mut err) = child.stderr.take() {
        std::io::Read::read_to_end(&mut err, &mut stderr).map_err(AppError::Io)?;
    }
    let exit = status.unwrap();
    Ok(std::process::Output {
        status: exit,
        stdout,
        stderr,
    })
}

// ---------------------------------------------------------------------------
// SCAN helpers — SQL queries that find items needing enrichment
// ---------------------------------------------------------------------------

/// Returns memories without any `memory_entities` binding.
///
/// These are the targets for `memory-bindings` enrichment. When `name_filter`
/// is non-empty, restricts the scan to the given names (G37); unknown names
/// are silently skipped (the caller can detect them by comparing
/// requested vs. returned).
fn scan_unbound_memories(
    conn: &Connection,
    namespace: &str,
    limit: Option<usize>,
    name_filter: &[String],
) -> Result<Vec<(i64, String, String)>, AppError> {
    let limit_clause = limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();

    if name_filter.is_empty() {
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
    } else {
        // Build a parameterised IN clause: ?2, ?3, ..., ?{1+n}
        let placeholders: Vec<String> = (2..=name_filter.len() + 1)
            .map(|i| format!("?{i}"))
            .collect();
        let in_clause = placeholders.join(", ");
        let sql = format!(
            "SELECT m.id, m.name, m.body
             FROM memories m
             WHERE m.namespace = ?1
               AND m.deleted_at IS NULL
               AND m.name IN ({in_clause})
               AND NOT EXISTS (
                   SELECT 1 FROM memory_entities me WHERE me.memory_id = m.id
               )
             ORDER BY m.id
             {limit_clause}"
        );
        let mut params_vec: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(1 + name_filter.len());
        params_vec.push(&namespace);
        for n in name_filter {
            params_vec.push(n);
        }
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(
                rusqlite::params_from_iter(params_vec.iter().copied()),
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

/// Reads a list of memory names from a UTF-8 text file (G37).
///
/// Empty lines and lines beginning with `#` are skipped. Returns a
/// de-duplicated, order-preserving list of trimmed names.
fn read_names_file(path: &Path) -> Result<Vec<String>, AppError> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        AppError::Validation(format!("failed to read names file {}: {e}", path.display()))
    })?;
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            out.push(trimmed.to_string());
        }
    }
    Ok(out)
}

/// Resolves the union of `--names` and `--names-file` (G37).
fn resolve_name_filter(args: &EnrichArgs) -> Result<Vec<String>, AppError> {
    let mut combined: Vec<String> = args.names.clone();
    if let Some(p) = &args.names_file {
        let from_file = read_names_file(p)?;
        for n in from_file {
            if !combined.contains(&n) {
                combined.push(n);
            }
        }
    }
    Ok(combined)
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

/// Returns live memories that still have no row in `memory_embeddings`.
///
/// These are the targets for `re-embed`.
fn scan_memories_without_embeddings(
    conn: &Connection,
    namespace: &str,
    limit: Option<usize>,
    name_filter: &[String],
) -> Result<Vec<(i64, String, String)>, AppError> {
    let limit_clause = limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();

    if name_filter.is_empty() {
        let sql = format!(
            "SELECT m.id, m.name, COALESCE(m.body,'')
             FROM memories m
             LEFT JOIN memory_embeddings me ON me.memory_id = m.id
             WHERE m.namespace = ?1
               AND m.deleted_at IS NULL
               AND me.memory_id IS NULL
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
    } else {
        let placeholders: Vec<String> = (2..=name_filter.len() + 1)
            .map(|i| format!("?{i}"))
            .collect();
        let in_clause = placeholders.join(", ");
        let sql = format!(
            "SELECT m.id, m.name, COALESCE(m.body,'')
             FROM memories m
             LEFT JOIN memory_embeddings me ON me.memory_id = m.id
             WHERE m.namespace = ?1
               AND m.deleted_at IS NULL
               AND m.name IN ({in_clause})
               AND me.memory_id IS NULL
             ORDER BY m.id
             {limit_clause}"
        );
        let mut params_vec: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(1 + name_filter.len());
        params_vec.push(&namespace);
        for n in name_filter {
            params_vec.push(n);
        }
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(
                rusqlite::params_from_iter(params_vec.iter().copied()),
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

/// G27: Returns relationships with weight >= 0.7 that may need recalibration.
#[allow(clippy::type_complexity)]
fn scan_weight_candidates(
    conn: &Connection,
    namespace: &str,
    limit: Option<usize>,
) -> Result<Vec<(i64, String, String, String, f64)>, AppError> {
    let limit_clause = limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();
    let sql = format!(
        "SELECT r.id, e1.name, e2.name, r.relation, r.weight \
         FROM relationships r \
         JOIN entities e1 ON e1.id = r.source_id \
         JOIN entities e2 ON e2.id = r.target_id \
         WHERE r.weight >= 0.7 AND e1.namespace = ?1 \
         ORDER BY r.weight DESC {limit_clause}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params![namespace], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, f64>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// G27: Returns relationships with generic relation types (applies_to).
fn scan_generic_relations(
    conn: &Connection,
    namespace: &str,
    limit: Option<usize>,
) -> Result<Vec<(i64, String, String, String)>, AppError> {
    let limit_clause = limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();
    let sql = format!(
        "SELECT r.id, e1.name, e2.name, r.relation \
         FROM relationships r \
         JOIN entities e1 ON e1.id = r.source_id \
         JOIN entities e2 ON e2.id = r.target_id \
         WHERE r.relation = 'applies_to' AND e1.namespace = ?1 \
         ORDER BY r.id {limit_clause}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params![namespace], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
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

/// v1.0.84 (ADR-0042): on successful re-embed, records the active backend
/// into the shared accumulator (`ENRICH_LAST_BACKEND`) so the final
/// `EnrichSummary` can expose `backend_invoked` without changing every
/// caller's signature. Best-effort observability — concurrent enrich runs
/// may race, but `Mutex` keeps the mutation safe.
#[allow(clippy::too_many_arguments)]
fn reembed_memory_vector(
    conn: &Connection,
    namespace: &str,
    memory_id: i64,
    memory_name: &str,
    memory_type: &str,
    body: &str,
    paths: &crate::paths::AppPaths,
    llm_backend: crate::cli::LlmBackendChoice,
) -> Result<(), AppError> {
    let snippet: String = body.chars().take(200).collect();
    // v1.0.82 (GAP-003): forward --llm-backend to embed_with_fallback.
    // v1.0.84 (ADR-0042): tuple (Vec<f32>, LlmBackendKind) — extrai o
    // backend que efetivamente rodou e popula o accumulator para o
    // EnrichSummary agregado.
    let (embedding, backend_kind) =
        crate::embedder::embed_passage_with_choice(&paths.models, body, Some(llm_backend))?;
    record_enrich_backend(backend_kind.as_str());
    memories::upsert_vec(
        conn,
        memory_id,
        namespace,
        memory_type,
        &embedding,
        memory_name,
        &snippet,
    )?;
    Ok(())
}

/// v1.0.84 (ADR-0042): process-local accumulator of the last LLM backend
/// that successfully ran a re-embed during the current enrich invocation.
/// Read by `run` once at summary emission. Scoped to a single process —
/// cross-process enrichment is gated by the per-namespace singleton, so
/// there is no concurrency hazard across DBs.
fn record_enrich_backend(backend: &'static str) {
    if let Ok(mut guard) = ENRICH_LAST_BACKEND.lock() {
        *guard = Some(backend);
    }
}

fn take_enrich_backend() -> Option<&'static str> {
    ENRICH_LAST_BACKEND.lock().ok().and_then(|mut g| g.take())
}

static ENRICH_LAST_BACKEND: std::sync::Mutex<Option<&'static str>> = std::sync::Mutex::new(None);

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
    llm_backend: crate::cli::LlmBackendChoice,
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
        source: "agent".to_string(),
        metadata: serde_json::json!({
            "operation": "body-enrich",
            "orig_chars": old_body.chars().count(),
            "new_chars": new_body.chars().count(),
        }),
    };

    // G29 audit: insert a new immutable version BEFORE the update so the
    // enriched body is reachable through `history --name <X>` and
    // `restore --version N` can roll back to the pre-enrich state.
    let next_version = crate::storage::versions::next_version(conn, memory_id)?;
    let version_metadata = serde_json::json!({
        "operation": "body-enrich",
        "orig_chars": old_body.chars().count(),
        "new_chars": new_body.chars().count(),
    })
    .to_string();
    crate::storage::versions::insert_version(
        conn,
        memory_id,
        next_version,
        memory_name,
        &memory_type,
        &description,
        new_body,
        &version_metadata,
        Some("enrich"),
        "edit",
    )?;

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
    if let Err(e) = reembed_memory_vector(
        conn,
        namespace,
        memory_id,
        memory_name,
        &memory_type,
        new_body,
        paths,
        llm_backend,
    ) {
        tracing::warn!(target: "enrich", memory = %memory_name, error = %e, "vec upsert failed after body-enrich");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// G20: mode-conditional flag validation
// ---------------------------------------------------------------------------

/// True when a scalar value matches its declared default. Used to
/// distinguish "operator passed an explicit override" from "clap filled
/// the default" for flags with default_value_t.
fn is_at_default<T: PartialEq>(value: T, default: T) -> bool {
    value == default
}

/// G20: validate that flags for one LLM provider were not passed when
/// the operator selected a different provider. Flags silently discarded
/// by the wrong mode are surfaced as AppError::Validation BEFORE any
/// DB work, so the operator gets an actionable error instead of a
/// surprise at runtime.
///
/// Detection rules:
/// - For Option<PathBuf> / Option<String>: is_some() means explicit
/// - For scalar fields with default_value_t: value != default means explicit
/// - For boolean fields: true means explicit (default is false)
///
/// Mode-specific matrices:
/// - mode=claude-code rejects: codex_binary, codex_model, codex_timeout != 300
/// - mode=codex rejects: claude_binary, claude_model, claude_timeout != 300, max_cost_usd
fn validate_mode_conditional_flags_enrich(args: &EnrichArgs) -> Result<(), AppError> {
    const DEFAULT_TIMEOUT: u64 = 300;

    let mut conflicts: Vec<String> = Vec::new();

    match args.mode {
        EnrichMode::ClaudeCode => {
            if args.codex_binary.is_some() {
                conflicts.push("--codex-binary is ignored when --mode=claude-code".to_string());
            }
            if args.codex_model.is_some() {
                conflicts.push("--codex-model is ignored when --mode=claude-code".to_string());
            }
            if !is_at_default(args.codex_timeout, DEFAULT_TIMEOUT) {
                conflicts.push(format!(
                    "--codex-timeout={} is ignored when --mode=claude-code (remove the flag to use the default 300s)",
                    args.codex_timeout
                ));
            }
        }
        EnrichMode::Codex => {
            if args.claude_binary.is_some() {
                conflicts.push("--claude-binary is ignored when --mode=codex".to_string());
            }
            if args.claude_model.is_some() {
                conflicts.push("--claude-model is ignored when --mode=codex".to_string());
            }
            if !is_at_default(args.claude_timeout, DEFAULT_TIMEOUT) {
                conflicts.push(format!(
                    "--claude-timeout={} is ignored when --mode=codex (remove the flag to use the default 300s)",
                    args.claude_timeout
                ));
            }
            if args.max_cost_usd.is_some() {
                conflicts.push(
                    "--max-cost-usd is ignored when --mode=codex (OAuth-first; cost is metered by your subscription, not the call)"
                        .to_string(),
                );
            }
        }
    }

    if !conflicts.is_empty() {
        return Err(AppError::Validation(format!(
            "G20: mode-conditional flag conflicts detected for --mode={}:\n  - {}",
            args.mode,
            conflicts.join("\n  - ")
        )));
    }

    Ok(())
}

// ---------------------------------------------------------------------------

/// Main entry point for the `enrich` command.
pub fn run(args: &EnrichArgs, llm_backend: crate::cli::LlmBackendChoice) -> Result<(), AppError> {
    // G20: mode-conditional flag validation BEFORE any DB access.
    // Surfaces flags that the wrong mode would silently discard.
    validate_mode_conditional_flags_enrich(args)?;
    let started = Instant::now();

    let paths = AppPaths::resolve(args.db.as_deref())?;
    ensure_db_ready(&paths)?;
    let conn = open_rw(&paths.db)?;
    let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;

    // G28-B (v1.0.68) + G30 (v1.0.69): enforce singleton per
    // (job_type, namespace, db_hash) so two parallel `enrich` invocations
    // on the same DB cannot co-exist, but concurrent enrich on different
    // databases works as expected. The force flag (--force) breaks a
    // stale lock from a previously crashed invocation.
    let wait_secs = args.wait_job_singleton;
    let force_flag = args.force_job_singleton;
    let _singleton = crate::lock::acquire_job_singleton(
        crate::lock::JobType::Enrich,
        &namespace,
        &paths.db,
        wait_secs,
        force_flag,
    )?;

    // Validate provider binary upfront only for LLM-backed operations.
    let provider_binary = if matches!(args.operation, EnrichOperation::ReEmbed) {
        None
    } else {
        Some(match args.mode {
            EnrichMode::ClaudeCode => {
                let bin = find_claude_binary(args.claude_binary.as_deref())?;
                let version = super::claude_runner::validate_claude_version(&bin)?;
                tracing::info!(target: "enrich", binary = %bin.display(), version = %version, "Claude Code binary validated");
                emit_json(&PhaseEvent {
                    phase: "validate",
                    binary_path: bin.to_str(),
                    version: Some(&version),
                    items_total: None,
                    items_pending: None,
                    llm_parallelism: None,
                });
                bin
            }
            EnrichMode::Codex => {
                let bin = find_codex_binary(args.codex_binary.as_deref())?;
                emit_json(&PhaseEvent {
                    phase: "validate",
                    binary_path: bin.to_str(),
                    version: None,
                    items_total: None,
                    items_pending: None,
                    llm_parallelism: None,
                });
                bin
            }
        })
    };

    // G28-D: refuse to start when the system is saturated. This check
    // is BEFORE preflight so we never spend an OAuth turn on a host
    // that is already at the limit.
    if args.max_load_check && !args.dry_run && crate::system_load::is_system_saturated() {
        let load = crate::system_load::load_average_one();
        let n = crate::system_load::ncpus();
        return Err(AppError::Validation(format!(
            "system load average {load:.2} exceeds 2x ncpus ({n}); \
             pass --no-max-load-check to override (not recommended)"
        )));
    }

    // G35: preflight probe — issue a single ping turn to verify the
    // provider is healthy before scanning N candidates. If the probe
    // fails with a rate-limit error, optionally fall back to a
    // different mode (typically codex) instead of failing the entire
    // batch. The probe itself consumes 1 OAuth turn, so it stays
    // opt-in (default off) to keep --dry-run and CI flows zero-cost.
    if args.preflight_check && !args.dry_run && !matches!(args.operation, EnrichOperation::ReEmbed)
    {
        let preflight_result = run_preflight_probe(args);
        match preflight_result {
            PreflightOutcome::Healthy => {
                tracing::info!(target: "enrich", mode = ?args.mode, "preflight probe healthy");
            }
            PreflightOutcome::RateLimited { reason, suggestion } => {
                if let Some(fallback) = args.fallback_mode.clone() {
                    if fallback != args.mode {
                        // G35 (v1.0.69): the mid-batch mode switch is
                        // intentionally NOT applied because it would
                        // desynchronise the per-item rate-limit wait
                        // state (rate-limited items in the worker are
                        // timed against the original provider). Instead
                        // we abort cleanly so the operator can re-invoke
                        // with `--mode {fallback:?}`. This guarantees no
                        // OAuth window is wasted and no partial state
                        // is left in the queue.
                        return Err(AppError::Validation(format!(
                            "preflight detected rate limit on {mode:?}: {reason}; \
                             re-invoke with `--mode {fallback:?}` to use the fallback provider",
                            mode = args.mode
                        )));
                    }
                    return Err(AppError::Validation(format!(
                        "preflight detected rate limit on {mode:?}: {reason}; \
                         --fallback-mode matches --mode, no recovery possible",
                        mode = args.mode
                    )));
                }
                return Err(AppError::Validation(format!(
                    "preflight detected rate limit on {mode:?}: {reason}; \
                     {suggestion}; pass --fallback-mode codex to recover",
                    mode = args.mode
                )));
            }
            PreflightOutcome::Error(e) => {
                return Err(e);
            }
        }
    }

    // SCAN phase
    let scan_result = scan_operation(&conn, &namespace, args)?;
    let total = scan_result.len();

    emit_json(&PhaseEvent {
        phase: "scan",
        binary_path: None,
        version: None,
        items_total: Some(total),
        items_pending: Some(total),
        llm_parallelism: Some(args.llm_parallelism),
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
            backend_invoked: take_enrich_backend(),
        });
        return Ok(());
    }

    // All operations in this enum have an execution path.

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
        if let Err(e) = queue_conn.execute(
            "INSERT OR IGNORE INTO queue (item_key, item_type, status) VALUES (?1, ?2, 'pending')",
            rusqlite::params![key, item_type],
        ) {
            tracing::warn!(target: "enrich", error = %e, "queue insert failed");
        }
        let _ = idx; // suppress unused warning
    }

    // G19: parallel LLM processing via std::thread::scope when parallelism > 1.
    // Clamp enforces the range even if the caller bypasses clap validation.
    let parallelism = args.llm_parallelism.clamp(1, 32) as usize;
    if parallelism > 1 {
        tracing::info!(
            target: "enrich",
            llm_parallelism = parallelism,
            "parallel LLM processing with bounded thread pool"
        );
    }
    // G28-D (v1.0.68) + G34 (v1.0.69): warn above the recommended parallelism
    // ceiling. The threshold and message depend on the LLM mode because
    // Claude Code spawns MCP children (G28-A) while Codex does not.
    if parallelism > 4 {
        match args.mode {
            EnrichMode::ClaudeCode => {
                tracing::warn!(
                    target: "enrich",
                    llm_parallelism = parallelism,
                    recommended_max = 4,
                    mode = "claude-code",
                    "llm_parallelism above 4 multiplies Claude Code subprocess fan-out; \
                     consider combining with SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR \
                     to cut MCP children (G28-A)"
                );
            }
            EnrichMode::Codex if parallelism > 16 => {
                tracing::warn!(
                    target: "enrich",
                    llm_parallelism = parallelism,
                    recommended_max = 16,
                    mode = "codex",
                    "llm_parallelism above 16 risks OAuth rate-limit on Codex; \
                     consider --llm-parallelism 8 for safer concurrency"
                );
            }
            EnrichMode::Codex => {
                // No warning: codex does not spawn MCP children and was
                // validated at parallelism 8 in production (1161 items,
                // 0 failures) per the 2026-06-04 session audit.
            }
        }
    }

    let mut completed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut cost_total = 0.0f64;
    let mut oauth_detected = false;
    let mut backoff_secs = DEFAULT_RATE_LIMIT_WAIT;
    let rate_limit_deadline = std::time::Instant::now() + std::time::Duration::from_secs(3600);
    let enrich_started = std::time::Instant::now();

    let provider_timeout = match args.mode {
        EnrichMode::ClaudeCode => args.claude_timeout,
        EnrichMode::Codex => args.codex_timeout,
    };

    let provider_model: Option<&str> = match args.mode {
        EnrichMode::ClaudeCode => args.claude_model.as_deref(),
        EnrichMode::Codex => args.codex_model.as_deref(),
    };

    // G19: when parallelism > 1, spawn bounded worker threads.
    // Each worker opens its own DB connections (WAL supports concurrent readers + serialized writers).
    // The queue DB claim is atomic via UPDATE...RETURNING — no external lock needed.
    if parallelism > 1 {
        let stdout_mu = parking_lot::Mutex::new(());
        let budget = args.max_cost_usd;
        let operation = args.operation.clone();
        let mode = args.mode.clone();
        let min_oc = args.min_output_chars;
        let max_oc = args.max_output_chars;
        let prompt_tpl = args.prompt_template.as_deref().map(|p| p.to_path_buf());

        struct WorkerResult {
            completed: usize,
            failed: usize,
            skipped: usize,
            cost: f64,
            oauth: bool,
        }

        let results: Vec<WorkerResult> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..parallelism)
                .map(|worker_id| {
                    let stdout_mu = &stdout_mu;
                    let paths = &paths;
                    let namespace = &namespace;
                    let provider_binary = provider_binary.as_deref();
                    let operation = &operation;
                    let mode = &mode;
                    let prompt_tpl = prompt_tpl.as_deref();
                    s.spawn(move || {
                        let w_conn = match open_rw(&paths.db) {
                            Ok(c) => c,
                            Err(e) => {
                                tracing::error!(target: "enrich", worker = worker_id, error = %e, "worker failed to open DB");
                                return WorkerResult { completed: 0, failed: 0, skipped: 0, cost: 0.0, oauth: false };
                            }
                        };
                        let w_queue = match open_queue_db(DEFAULT_QUEUE_DB) {
                            Ok(c) => c,
                            Err(e) => {
                                tracing::error!(target: "enrich", worker = worker_id, error = %e, "worker failed to open queue DB");
                                return WorkerResult { completed: 0, failed: 0, skipped: 0, cost: 0.0, oauth: false };
                            }
                        };
                        let mut w_completed = 0usize;
                        let mut w_failed = 0usize;
                        let mut w_skipped = 0usize;
                        let mut w_cost = 0.0f64;
                        let mut w_oauth = false;
                        let mut w_backoff = DEFAULT_RATE_LIMIT_WAIT;
                        let w_deadline = std::time::Instant::now() + std::time::Duration::from_secs(3600);
                        // G28-D: per-worker circuit breaker that aborts the
                        // loop after `circuit_breaker_threshold` consecutive
                        // HardFailure outcomes (transient/rate-limited errors
                        // do NOT count, so a recovering provider is not
                        // penalised).
                        let mut w_breaker = crate::retry::CircuitBreaker::new(
                            args.circuit_breaker_threshold.max(1),
                            std::time::Duration::from_secs(60),
                        );

                        loop {
                            if crate::shutdown_requested() {
                                tracing::info!(target: "enrich", "shutdown requested, worker stopping");
                                break;
                            }
                            if let Some(b) = budget {
                                if !w_oauth && w_cost >= b {
                                    break;
                                }
                            }
                            let pending: Option<(i64, String, String)> = w_queue
                                .query_row(
                                    "UPDATE queue SET status='processing', attempt=attempt+1 \
                                     WHERE id = (SELECT id FROM queue WHERE status='pending' ORDER BY id LIMIT 1) \
                                     RETURNING id, item_key, item_type",
                                    [],
                                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                                )
                                .ok();
                            let (queue_id, item_key, _item_type) = match pending {
                                Some(p) => p,
                                None => break,
                            };
                            let item_started = Instant::now();
                            let current_index = w_completed + w_failed + w_skipped;

                            let call_result = match operation {
                                EnrichOperation::MemoryBindings => call_memory_bindings(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode),
                                EnrichOperation::EntityDescriptions => call_entity_description(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode),
                                EnrichOperation::BodyEnrich => call_body_enrich(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode, min_oc, max_oc, prompt_tpl, args.preserve_threshold, paths, llm_backend),
                                EnrichOperation::ReEmbed => call_reembed(&w_conn, namespace, &item_key, paths, llm_backend),
                                EnrichOperation::WeightCalibrate => call_weight_calibrate(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode),
                                EnrichOperation::RelationReclassify => call_relation_reclassify(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode),
                                EnrichOperation::EntityConnect | EnrichOperation::CrossDomainBridges => call_entity_connect(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode),
                                EnrichOperation::EntityTypeValidate => call_entity_type_validate(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode),
                                EnrichOperation::DescriptionEnrich => call_description_enrich(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode),
                                EnrichOperation::DomainClassify => call_domain_classify(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode),
                                EnrichOperation::GraphAudit => call_graph_audit(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode),
                                EnrichOperation::DeepResearchSynth => call_deep_research_synth(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode),
                                EnrichOperation::BodyExtract => call_body_extract(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode),
                            };

                            match call_result {
                                Ok(EnrichItemResult::Done { cost, is_oauth, memory_id, entity_id, entities, rels, chars_before, chars_after }) => {
                                    if is_oauth { w_oauth = true; }
                                    w_backoff = DEFAULT_RATE_LIMIT_WAIT;
                                    let _ = w_queue.execute(
                                        "UPDATE queue SET status='done', memory_id=?1, entity_id=?2, entities=?3, rels=?4, cost_usd=?5, elapsed_ms=?6, done_at=datetime('now') WHERE id=?7",
                                        rusqlite::params![memory_id, entity_id, entities as i64, rels as i64, cost, item_started.elapsed().as_millis() as i64, queue_id],
                                    );
                                    w_completed += 1;
                                    if !is_oauth { w_cost += cost; }
                                    // G28-D: count success; resets breaker.
                                    let _ = w_breaker
                                        .record(crate::retry::AttemptOutcome::Success);
                                    let _guard = stdout_mu.lock();
                                    emit_json(&ItemEvent { item: &item_key, status: "done", memory_id, entity_id, entities: Some(entities), rels: Some(rels), chars_before, chars_after, cost_usd: if is_oauth { None } else { Some(cost) }, elapsed_ms: Some(item_started.elapsed().as_millis() as u64), error: None, index: current_index, total });
                                }
                                Ok(EnrichItemResult::Skipped { reason }) => {
                                    w_skipped += 1;
                                    let _ = w_queue.execute("UPDATE queue SET status='skipped', error=?1, done_at=datetime('now') WHERE id=?2", rusqlite::params![reason, queue_id]);
                                    let _guard = stdout_mu.lock();
                                    emit_json(&ItemEvent { item: &item_key, status: "skipped", memory_id: None, entity_id: None, entities: None, rels: None, chars_before: None, chars_after: None, cost_usd: None, elapsed_ms: Some(item_started.elapsed().as_millis() as u64), error: None, index: current_index, total });
                                }
                                Ok(EnrichItemResult::PreservationFailed { score, threshold, chars_before, chars_after }) => {
                                    // G29 Passo 4: worker mirror of the
                                    // serial path. Counted as a soft
                                    // skip so the queue surface shows
                                    // a quality issue rather than a
                                    // transport failure.
                                    w_skipped += 1;
                                    let reason = format!(
                                        "preservation_failed: jaccard={score:.3} threshold={threshold:.3} (orig={chars_before} chars, new={chars_after} chars)"
                                    );
                                    let _ = w_queue.execute(
                                        "UPDATE queue SET status='skipped', error=?1, done_at=datetime('now') WHERE id=?2",
                                        rusqlite::params![reason, queue_id],
                                    );
                                    let _guard = stdout_mu.lock();
                                    emit_json(&ItemEvent {
                                        item: &item_key,
                                        status: "preservation_failed",
                                        memory_id: None,
                                        entity_id: None,
                                        entities: None,
                                        rels: None,
                                        chars_before: Some(chars_before),
                                        chars_after: Some(chars_after),
                                        cost_usd: None,
                                        elapsed_ms: Some(item_started.elapsed().as_millis() as u64),
                                        error: Some(reason),
                                        index: current_index,
                                        total,
                                    });
                                }
                                Err(e) => {
                                    let err_str = format!("{e}");
                                    if matches!(e, AppError::RateLimited { .. }) {
                                        if crate::retry::is_kill_switch_active() {
                                            tracing::warn!(target: "enrich", "SQLITE_GRAPHRAG_DISABLE_RETRY=1, skipping rate-limit retry");
                                        } else if std::time::Instant::now() >= w_deadline {
                                            tracing::error!(target: "enrich", "rate-limit retry deadline (1h) exhausted in worker");
                                        } else {
                                            let half = w_backoff / 2;
                                            let jitter = if half == 0 { 0 } else { fastrand::u64(0..half) };
                                            let actual_wait = half + jitter;
                                            tracing::warn!(target: "enrich", delay_secs = actual_wait, error_kind = "rate_limited", "rate limited in worker, backing off");
                                            let _ = w_queue.execute("UPDATE queue SET status='pending' WHERE id=?1", rusqlite::params![queue_id]);
                                            std::thread::sleep(std::time::Duration::from_secs(actual_wait));
                                            w_backoff = (w_backoff * 2).min(900);
                                            continue;
                                        }
                                    }
                                    w_failed += 1;
                                    let _ = w_queue.execute("UPDATE queue SET status='failed', error=?1, done_at=datetime('now') WHERE id=?2", rusqlite::params![err_str, queue_id]);
                                    let _guard = stdout_mu.lock();
                                    emit_json(&ItemEvent { item: &item_key, status: "failed", memory_id: None, entity_id: None, entities: None, rels: None, chars_before: None, chars_after: None, cost_usd: None, elapsed_ms: Some(item_started.elapsed().as_millis() as u64), error: Some(err_str), index: current_index, total });
                                    // G28-D: count hard failure against breaker.
                                    let breaker_opened = w_breaker
                                        .record(crate::retry::AttemptOutcome::HardFailure);
                                    if breaker_opened {
                                        tracing::error!(target: "enrich",
                                            consecutive_failures = w_breaker.consecutive_failures(),
                                            "circuit breaker opened — aborting worker"
                                        );
                                        break;
                                    }
                                }
                            }
                        }
                        WorkerResult { completed: w_completed, failed: w_failed, skipped: w_skipped, cost: w_cost, oauth: w_oauth }
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join().unwrap_or(WorkerResult {
                        completed: 0,
                        failed: 0,
                        skipped: 0,
                        cost: 0.0,
                        oauth: false,
                    })
                })
                .collect()
        });

        for r in &results {
            completed += r.completed;
            failed += r.failed;
            skipped += r.skipped;
            cost_total += r.cost;
            if r.oauth && !oauth_detected {
                oauth_detected = true;
            }
        }
    } else {
        // Serial path (parallelism == 1) — original loop
        loop {
            if crate::shutdown_requested() {
                tracing::info!(target: "enrich", "shutdown requested, stopping enrichment");
                break;
            }

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
                    provider_binary
                        .as_deref()
                        .expect("provider binary required"),
                    provider_model,
                    provider_timeout,
                    &args.mode,
                ),
                EnrichOperation::EntityDescriptions => call_entity_description(
                    &conn,
                    &namespace,
                    &item_key,
                    provider_binary
                        .as_deref()
                        .expect("provider binary required"),
                    provider_model,
                    provider_timeout,
                    &args.mode,
                ),
                EnrichOperation::BodyEnrich => call_body_enrich(
                    &conn,
                    &namespace,
                    &item_key,
                    provider_binary
                        .as_deref()
                        .expect("provider binary required"),
                    provider_model,
                    provider_timeout,
                    &args.mode,
                    args.min_output_chars,
                    args.max_output_chars,
                    args.prompt_template.as_deref(),
                    args.preserve_threshold,
                    &paths,
                    llm_backend,
                ),
                EnrichOperation::ReEmbed => {
                    call_reembed(&conn, &namespace, &item_key, &paths, llm_backend)
                }
                EnrichOperation::WeightCalibrate => call_weight_calibrate(
                    &conn,
                    &namespace,
                    &item_key,
                    provider_binary
                        .as_deref()
                        .expect("provider binary required"),
                    provider_model,
                    provider_timeout,
                    &args.mode,
                ),
                EnrichOperation::RelationReclassify => call_relation_reclassify(
                    &conn,
                    &namespace,
                    &item_key,
                    provider_binary
                        .as_deref()
                        .expect("provider binary required"),
                    provider_model,
                    provider_timeout,
                    &args.mode,
                ),
                EnrichOperation::EntityConnect | EnrichOperation::CrossDomainBridges => {
                    call_entity_connect(
                        &conn,
                        &namespace,
                        &item_key,
                        provider_binary
                            .as_deref()
                            .expect("provider binary required"),
                        provider_model,
                        provider_timeout,
                        &args.mode,
                    )
                }
                EnrichOperation::EntityTypeValidate => call_entity_type_validate(
                    &conn,
                    &namespace,
                    &item_key,
                    provider_binary
                        .as_deref()
                        .expect("provider binary required"),
                    provider_model,
                    provider_timeout,
                    &args.mode,
                ),
                EnrichOperation::DescriptionEnrich => call_description_enrich(
                    &conn,
                    &namespace,
                    &item_key,
                    provider_binary
                        .as_deref()
                        .expect("provider binary required"),
                    provider_model,
                    provider_timeout,
                    &args.mode,
                ),
                EnrichOperation::DomainClassify => call_domain_classify(
                    &conn,
                    &namespace,
                    &item_key,
                    provider_binary
                        .as_deref()
                        .expect("provider binary required"),
                    provider_model,
                    provider_timeout,
                    &args.mode,
                ),
                EnrichOperation::GraphAudit => call_graph_audit(
                    &conn,
                    &namespace,
                    &item_key,
                    provider_binary
                        .as_deref()
                        .expect("provider binary required"),
                    provider_model,
                    provider_timeout,
                    &args.mode,
                ),
                EnrichOperation::DeepResearchSynth => call_deep_research_synth(
                    &conn,
                    &namespace,
                    &item_key,
                    provider_binary
                        .as_deref()
                        .expect("provider binary required"),
                    provider_model,
                    provider_timeout,
                    &args.mode,
                ),
                EnrichOperation::BodyExtract => call_body_extract(
                    &conn,
                    &namespace,
                    &item_key,
                    provider_binary
                        .as_deref()
                        .expect("provider binary required"),
                    provider_model,
                    provider_timeout,
                    &args.mode,
                ),
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
                        _ => {
                            // All G27 operations persist inside their call_* function
                            None
                        }
                    };

                    if let Err(e) = queue_conn.execute(
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
                ) {
                        tracing::warn!(target: "enrich", error = %e, "queue done update failed");
                    }

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
                    if let Err(e) = queue_conn.execute(
                    "UPDATE queue SET status='skipped', error=?1, done_at=datetime('now') WHERE id=?2",
                    rusqlite::params![reason, queue_id],
                ) {
                        tracing::warn!(target: "enrich", error = %e, "queue skipped update failed");
                    }
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
                Ok(EnrichItemResult::PreservationFailed {
                    score,
                    threshold,
                    chars_before,
                    chars_after,
                }) => {
                    // G29 Passo 4: the LLM rewrite diverged too far from
                    // the original body. Count as a soft failure (not
                    // `failed`) so the queue surfaces it as a quality
                    // issue, not a transport error. The reason is
                    // structured so the operator can audit why a body
                    // was rejected.
                    skipped += 1;
                    let reason = format!(
                        "preservation_failed: jaccard={score:.3} threshold={threshold:.3} (orig={chars_before} chars, new={chars_after} chars)"
                    );
                    if let Err(qe) = queue_conn.execute(
                        "UPDATE queue SET status='skipped', error=?1, done_at=datetime('now') WHERE id=?2",
                        rusqlite::params![reason, queue_id],
                    ) {
                        tracing::warn!(target: "enrich", error = %qe, "queue preservation_failed update failed");
                    }
                    emit_json(&ItemEvent {
                        item: &item_key,
                        status: "preservation_failed",
                        memory_id: None,
                        entity_id: None,
                        entities: None,
                        rels: None,
                        chars_before: Some(chars_before),
                        chars_after: Some(chars_after),
                        cost_usd: None,
                        elapsed_ms: Some(item_started.elapsed().as_millis() as u64),
                        error: Some(reason),
                        index: current_index,
                        total,
                    });
                }
                Err(e) => {
                    let err_str = format!("{e}");
                    if matches!(e, AppError::RateLimited { .. }) {
                        if crate::retry::is_kill_switch_active() {
                            tracing::warn!(target: "enrich", "SQLITE_GRAPHRAG_DISABLE_RETRY=1, skipping rate-limit retry");
                        } else if std::time::Instant::now() >= rate_limit_deadline {
                            tracing::error!(target: "enrich", total_elapsed_secs = enrich_started.elapsed().as_secs(), "rate-limit retry deadline (1h) exhausted");
                        } else {
                            let half = backoff_secs / 2;
                            let jitter = if half == 0 { 0 } else { fastrand::u64(0..half) };
                            let actual_wait = half + jitter;
                            tracing::warn!(target: "enrich", delay_secs = actual_wait, error_kind = "rate_limited", "rate limited, backing off");
                            if let Err(qe) = queue_conn.execute(
                                "UPDATE queue SET status='pending' WHERE id=?1",
                                rusqlite::params![queue_id],
                            ) {
                                tracing::warn!(target: "enrich", error = %qe, "queue pending update failed");
                            }
                            std::thread::sleep(std::time::Duration::from_secs(actual_wait));
                            backoff_secs = (backoff_secs * 2).min(900);
                            continue;
                        }
                    }

                    failed += 1;
                    if let Err(qe) = queue_conn.execute(
                    "UPDATE queue SET status='failed', error=?1, done_at=datetime('now') WHERE id=?2",
                    rusqlite::params![err_str, queue_id],
                ) {
                        tracing::warn!(target: "enrich", error = %qe, "queue failed update failed");
                    }
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
    } // end else (serial path)

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
        backend_invoked: take_enrich_backend(),
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
    /// G29 Passo 4 (v1.0.69): the LLM rewrite diverged from the original
    /// body beyond the configured `--preserve-threshold` and was rejected
    /// before persistence. The trigram-Jaccard score and threshold are
    /// emitted in the NDJSON stream for operator audit.
    PreservationFailed {
        score: f64,
        threshold: f64,
        chars_before: usize,
        chars_after: usize,
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
    preserve_threshold: f64,
    paths: &crate::paths::AppPaths,
    llm_backend: crate::cli::LlmBackendChoice,
) -> Result<EnrichItemResult, AppError> {
    let (memory_id, body, description, memory_type): (i64, String, String, String) = conn
        .query_row(
            "SELECT id, COALESCE(body,''), COALESCE(description,''), COALESCE(type,'note') \
         FROM memories WHERE namespace=?1 AND name=?2 AND deleted_at IS NULL",
            rusqlite::params![namespace, memory_name],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("memory '{memory_name}' not found"))
            }
            other => AppError::Database(other),
        })?;

    let chars_before = body.chars().count();

    // G26: gather graph context for contextualized enrichment
    let linked_entities: Vec<String> = {
        let mut stmt = conn.prepare_cached(
            "SELECT e.name FROM memory_entities me \
             JOIN entities e ON e.id = me.entity_id \
             WHERE me.memory_id = ?1 LIMIT 10",
        )?;
        let result: Vec<String> = stmt
            .query_map(rusqlite::params![memory_id], |r| r.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);
        result
    };

    // Load custom prompt template if provided
    let prompt_prefix = if let Some(tmpl_path) = prompt_template {
        let file_size = std::fs::metadata(tmpl_path)
            .map_err(|e| {
                AppError::Io(std::io::Error::new(
                    e.kind(),
                    format!("failed to stat prompt template: {e}"),
                ))
            })?
            .len();
        if file_size > MAX_MEMORY_BODY_LEN as u64 {
            return Err(AppError::LimitExceeded(
                crate::i18n::validation::body_exceeds(MAX_MEMORY_BODY_LEN),
            ));
        }
        std::fs::read_to_string(tmpl_path).map_err(|e| {
            AppError::Io(std::io::Error::new(
                e.kind(),
                format!("failed to read prompt template: {e}"),
            ))
        })?
    } else {
        BODY_ENRICH_PROMPT_PREFIX.to_string()
    };

    // G26: build contextualized prompt with graph data
    let context_section = if !linked_entities.is_empty() || !description.is_empty() {
        let mut ctx = String::new();
        ctx.push_str(&format!(
            "\nContext:\n- Memory name: {memory_name}\n- Type: {memory_type}\n"
        ));
        if !description.is_empty() {
            ctx.push_str(&format!("- Description: {description}\n"));
        }
        ctx.push_str(&format!("- Domain: {namespace}\n"));
        if !linked_entities.is_empty() {
            ctx.push_str(&format!(
                "- Linked entities: {}\n",
                linked_entities.join(", ")
            ));
        }
        ctx
    } else {
        String::new()
    };

    let prompt = format!(
        "{prompt_prefix}{context_section}\nTarget minimum length: {min_output_chars} characters. Maximum: {max_output_chars} characters."
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

    // G29 Passo 4 (v1.0.69): preservation check. Before persisting, run
    // a trigram-Jaccard similarity between the original body and the
    // LLM-rewritten body. When the score falls below
    // `args.preserve_threshold` (default 0.7 per the G29 gap), reject the
    // rewrite as a likely hallucination. The result is recorded in the
    // NDJSON stream so operators can audit what the LLM tried to do.
    let threshold = preserve_threshold;
    let verdict =
        crate::preservation::PreservationVerdict::evaluate(&body, enriched_body, threshold);
    if !verdict.is_accepted() {
        return Ok(EnrichItemResult::PreservationFailed {
            score: match verdict {
                crate::preservation::PreservationVerdict::Preserved { score, .. } => score,
                crate::preservation::PreservationVerdict::Rejected { score, .. } => score,
                crate::preservation::PreservationVerdict::Unchanged { .. } => 1.0,
            },
            threshold,
            chars_before,
            chars_after,
        });
    }

    // G29 Passo 5 (v1.0.69): idempotency via blake3 hash. Before persisting,
    // compare the hash of the original body against the hash of the enriched
    // body. Identical hashes mean the LLM produced a byte-for-byte identical
    // body (rare but possible) — treat as `Skipped` so re-running the batch
    // is safe and the queue does not get re-persisted entries.
    let old_hash = blake3::hash(body.as_bytes()).to_hex().to_string();
    let new_hash = blake3::hash(enriched_body.as_bytes()).to_hex().to_string();
    if old_hash == new_hash {
        return Ok(EnrichItemResult::Skipped {
            reason: format!(
                "enriched body hash matches original (blake3:{old_hash}); idempotency skip"
            ),
        });
    }

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
        llm_backend,
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

fn call_reembed(
    conn: &Connection,
    namespace: &str,
    memory_name: &str,
    paths: &crate::paths::AppPaths,
    llm_backend: crate::cli::LlmBackendChoice,
) -> Result<EnrichItemResult, AppError> {
    let (memory_id, body, memory_type): (i64, String, String) = conn
        .query_row(
            "SELECT id, COALESCE(body,''), COALESCE(type,'note')
             FROM memories
             WHERE namespace=?1 AND name=?2 AND deleted_at IS NULL",
            rusqlite::params![namespace, memory_name],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("memory '{memory_name}' not found"))
            }
            other => AppError::Database(other),
        })?;

    if body.trim().is_empty() {
        return Ok(EnrichItemResult::Skipped {
            reason: "body is empty".to_string(),
        });
    }

    reembed_memory_vector(
        conn,
        namespace,
        memory_id,
        memory_name,
        &memory_type,
        &body,
        paths,
        llm_backend,
    )?;

    Ok(EnrichItemResult::Done {
        memory_id: Some(memory_id),
        entity_id: None,
        entities: 0,
        rels: 0,
        chars_before: Some(body.chars().count()),
        chars_after: Some(body.chars().count()),
        cost: 0.0,
        is_oauth: true,
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
    // G37: resolve --names + --names-file once and apply to every scan path.
    let name_filter = resolve_name_filter(args)?;
    match args.operation {
        EnrichOperation::MemoryBindings => {
            let rows = scan_unbound_memories(conn, namespace, args.limit, &name_filter)?;
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
        EnrichOperation::ReEmbed => {
            let rows = scan_memories_without_embeddings(conn, namespace, args.limit, &name_filter)?;
            Ok(rows.into_iter().map(|(_, name, _)| name).collect())
        }
        EnrichOperation::WeightCalibrate => {
            let rows = scan_weight_candidates(conn, namespace, args.limit)?;
            Ok(rows
                .into_iter()
                .map(|(id, _, _, _, _)| id.to_string())
                .collect())
        }
        EnrichOperation::RelationReclassify => {
            let rows = scan_generic_relations(conn, namespace, args.limit)?;
            Ok(rows
                .into_iter()
                .map(|(id, _, _, _)| id.to_string())
                .collect())
        }
        EnrichOperation::EntityConnect | EnrichOperation::CrossDomainBridges => {
            let pairs = scan_isolated_entity_pairs(conn, namespace, args.limit)?;
            Ok(pairs.into_iter().map(|(_, name, _, _)| name).collect())
        }
        EnrichOperation::EntityTypeValidate => {
            let rows = scan_entities_for_type_validation(conn, namespace, args.limit)?;
            Ok(rows.into_iter().map(|(_, name, _)| name).collect())
        }
        EnrichOperation::DescriptionEnrich => {
            let rows = scan_generic_descriptions(conn, namespace, args.limit)?;
            Ok(rows.into_iter().map(|(_, name, _)| name).collect())
        }
        EnrichOperation::DomainClassify
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
                return Ok(crate::extract::llm_embedding::resolve_real_binary(
                    &candidate,
                ));
            }
        }
    }

    Err(AppError::Validation(
        "Codex CLI binary not found in PATH. Install it or specify --codex-binary".to_string(),
    ))
}

/// G27: Calibrate weight of a single relationship via LLM.
fn call_weight_calibrate(
    conn: &Connection,
    _namespace: &str,
    item_key: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
) -> Result<EnrichItemResult, AppError> {
    let rel_id: i64 = item_key
        .parse()
        .map_err(|_| AppError::Validation(format!("invalid relationship id: {item_key}")))?;
    let (source_name, target_name, relation, current_weight): (String, String, String, f64) = conn
        .query_row(
            "SELECT e1.name, e2.name, r.relation, r.weight \
             FROM relationships r \
             JOIN entities e1 ON e1.id = r.source_id \
             JOIN entities e2 ON e2.id = r.target_id \
             WHERE r.id = ?1",
            rusqlite::params![rel_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|_| AppError::NotFound(format!("relationship {rel_id} not found")))?;

    let input_text = format!(
        "Source: {source_name}\nTarget: {target_name}\nRelation: {relation}\nCurrent weight: {current_weight}"
    );
    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            WEIGHT_CALIBRATE_PROMPT,
            WEIGHT_CALIBRATE_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            WEIGHT_CALIBRATE_PROMPT,
            WEIGHT_CALIBRATE_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
    };

    let calibrated = value
        .get("calibrated_weight")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| AppError::Validation("LLM result missing 'calibrated_weight'".into()))?;

    conn.execute(
        "UPDATE relationships SET weight = ?1 WHERE id = ?2",
        rusqlite::params![calibrated, rel_id],
    )?;

    Ok(EnrichItemResult::Done {
        memory_id: None,
        entity_id: None,
        entities: 0,
        rels: 1,
        chars_before: None,
        chars_after: None,
        cost,
        is_oauth,
    })
}

/// G27: Reclassify a generic relationship type via LLM.
fn call_relation_reclassify(
    conn: &Connection,
    _namespace: &str,
    item_key: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
) -> Result<EnrichItemResult, AppError> {
    let rel_id: i64 = item_key
        .parse()
        .map_err(|_| AppError::Validation(format!("invalid relationship id: {item_key}")))?;
    let (source_name, target_name, current_relation): (String, String, String) = conn
        .query_row(
            "SELECT e1.name, e2.name, r.relation \
             FROM relationships r \
             JOIN entities e1 ON e1.id = r.source_id \
             JOIN entities e2 ON e2.id = r.target_id \
             WHERE r.id = ?1",
            rusqlite::params![rel_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|_| AppError::NotFound(format!("relationship {rel_id} not found")))?;

    let input_text = format!(
        "Source entity: {source_name}\nTarget entity: {target_name}\nCurrent relation: {current_relation}"
    );
    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            RELATION_RECLASSIFY_PROMPT,
            RELATION_RECLASSIFY_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            RELATION_RECLASSIFY_PROMPT,
            RELATION_RECLASSIFY_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
    };

    let new_relation = value
        .get("relation")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("LLM result missing 'relation'".into()))?;
    let new_strength = value
        .get("strength")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5);

    conn.execute(
        "UPDATE relationships SET relation = ?1, weight = ?2 WHERE id = ?3",
        rusqlite::params![new_relation, new_strength, rel_id],
    )?;

    Ok(EnrichItemResult::Done {
        memory_id: None,
        entity_id: None,
        entities: 0,
        rels: 1,
        chars_before: None,
        chars_after: None,
        cost,
        is_oauth,
    })
}

/// G27 P2: Connect isolated entities via LLM-suggested relationship.
fn call_entity_connect(
    conn: &Connection,
    namespace: &str,
    item_key: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
) -> Result<EnrichItemResult, AppError> {
    let pairs = scan_isolated_entity_pairs(conn, namespace, Some(1))?;
    let (e1_id, e1_name, e2_id, e2_name) =
        match pairs.into_iter().find(|(_, n, _, _)| n == item_key) {
            Some(p) => p,
            None => {
                return Ok(EnrichItemResult::Skipped {
                    reason: "pair no longer isolated".into(),
                })
            }
        };
    let input_text = format!("Entity A: {e1_name}\nEntity B: {e2_name}");
    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            ENTITY_CONNECT_PROMPT,
            ENTITY_CONNECT_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            ENTITY_CONNECT_PROMPT,
            ENTITY_CONNECT_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
    };
    let relation = value
        .get("relation")
        .and_then(|v| v.as_str())
        .unwrap_or("none");
    if relation == "none" {
        return Ok(EnrichItemResult::Skipped {
            reason: "LLM determined no relationship".into(),
        });
    }
    let strength = value
        .get("strength")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5);
    conn.execute(
        "INSERT OR IGNORE INTO relationships (namespace, source_id, target_id, relation, weight) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![namespace, e1_id, e2_id, relation, strength],
    )?;
    Ok(EnrichItemResult::Done {
        memory_id: None,
        entity_id: None,
        entities: 0,
        rels: 1,
        chars_before: None,
        chars_after: None,
        cost,
        is_oauth,
    })
}

/// G27 P2: Validate entity type assignment via LLM.
fn call_entity_type_validate(
    conn: &Connection,
    _namespace: &str,
    item_key: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
) -> Result<EnrichItemResult, AppError> {
    let (ent_id, ent_name, ent_type): (i64, String, String) = conn
        .query_row(
            "SELECT id, name, type FROM entities WHERE name = ?1",
            rusqlite::params![item_key],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|_| AppError::NotFound(format!("entity '{item_key}' not found")))?;
    let input_text = format!("Entity: {ent_name}\nCurrent type: {ent_type}");
    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            ENTITY_TYPE_VALIDATE_PROMPT,
            ENTITY_TYPE_VALIDATE_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            ENTITY_TYPE_VALIDATE_PROMPT,
            ENTITY_TYPE_VALIDATE_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
    };
    let validated_type = value
        .get("validated_type")
        .and_then(|v| v.as_str())
        .unwrap_or(&ent_type);
    let was_correct = value
        .get("was_correct")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    if !was_correct {
        conn.execute(
            "UPDATE entities SET type = ?1 WHERE id = ?2",
            rusqlite::params![validated_type, ent_id],
        )?;
    }
    Ok(EnrichItemResult::Done {
        memory_id: None,
        entity_id: Some(ent_id),
        entities: 1,
        rels: 0,
        chars_before: None,
        chars_after: None,
        cost,
        is_oauth,
    })
}

/// G27 P2: Enrich generic memory description via LLM.
fn call_description_enrich(
    conn: &Connection,
    _namespace: &str,
    item_key: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
) -> Result<EnrichItemResult, AppError> {
    let (mem_id, body, old_desc): (i64, String, String) = conn
        .query_row(
            "SELECT id, body, description FROM memories WHERE name = ?1 AND deleted_at IS NULL",
            rusqlite::params![item_key],
            |r| Ok((r.get(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
        )
        .map_err(|_| AppError::NotFound(format!("memory '{item_key}' not found")))?;
    let snippet: String = body.chars().take(500).collect();
    let input_text = format!(
        "Memory name: {item_key}\nCurrent description: {old_desc}\nBody preview: {snippet}"
    );
    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            DESCRIPTION_ENRICH_PROMPT,
            DESCRIPTION_ENRICH_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            DESCRIPTION_ENRICH_PROMPT,
            DESCRIPTION_ENRICH_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
    };
    let new_desc = value
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or(&old_desc);
    conn.execute(
        "UPDATE memories SET description = ?1 WHERE id = ?2",
        rusqlite::params![new_desc, mem_id],
    )?;
    Ok(EnrichItemResult::Done {
        memory_id: Some(mem_id),
        entity_id: None,
        entities: 0,
        rels: 0,
        chars_before: Some(old_desc.len()),
        chars_after: Some(new_desc.len()),
        cost,
        is_oauth,
    })
}

/// G27 P2: Classify memory into domain category via LLM.
fn call_domain_classify(
    conn: &Connection,
    _namespace: &str,
    item_key: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
) -> Result<EnrichItemResult, AppError> {
    let (mem_id, body, desc): (i64, String, String) = conn
        .query_row(
            "SELECT id, body, description FROM memories WHERE name = ?1 AND deleted_at IS NULL",
            rusqlite::params![item_key],
            |r| Ok((r.get(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
        )
        .map_err(|_| AppError::NotFound(format!("memory '{item_key}' not found")))?;
    let snippet: String = body.chars().take(500).collect();
    let input_text = format!("Memory: {item_key}\nDescription: {desc}\nBody preview: {snippet}");
    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            DOMAIN_CLASSIFY_PROMPT,
            DOMAIN_CLASSIFY_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            DOMAIN_CLASSIFY_PROMPT,
            DOMAIN_CLASSIFY_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
    };
    let domain = value
        .get("domain")
        .and_then(|v| v.as_str())
        .unwrap_or("uncategorized");
    let metadata = format!(r#"{{"domain":"{}"}}"#, domain.replace('"', "\\\""));
    conn.execute(
        "UPDATE memories SET metadata = ?1 WHERE id = ?2",
        rusqlite::params![metadata, mem_id],
    )?;
    Ok(EnrichItemResult::Done {
        memory_id: Some(mem_id),
        entity_id: None,
        entities: 0,
        rels: 0,
        chars_before: None,
        chars_after: None,
        cost,
        is_oauth,
    })
}

/// G27 P2: Audit memory graph quality via LLM.
fn call_graph_audit(
    conn: &Connection,
    _namespace: &str,
    item_key: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
) -> Result<EnrichItemResult, AppError> {
    let (mem_id, body, desc): (i64, String, String) = conn
        .query_row(
            "SELECT id, body, description FROM memories WHERE name = ?1 AND deleted_at IS NULL",
            rusqlite::params![item_key],
            |r| Ok((r.get(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
        )
        .map_err(|_| AppError::NotFound(format!("memory '{item_key}' not found")))?;
    let snippet: String = body.chars().take(500).collect();
    let ent_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_entities WHERE memory_id = ?1",
            rusqlite::params![mem_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let input_text = format!("Memory: {item_key}\nDescription: {desc}\nEntity bindings: {ent_count}\nBody preview: {snippet}");
    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            GRAPH_AUDIT_PROMPT,
            GRAPH_AUDIT_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            GRAPH_AUDIT_PROMPT,
            GRAPH_AUDIT_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
    };
    let issues = value
        .get("issues")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    Ok(EnrichItemResult::Done {
        memory_id: Some(mem_id),
        entity_id: None,
        entities: 0,
        rels: issues,
        chars_before: None,
        chars_after: None,
        cost,
        is_oauth,
    })
}

/// G27 P2: Synthesize research findings into graph entities/relationships via LLM.
fn call_deep_research_synth(
    conn: &Connection,
    namespace: &str,
    item_key: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
) -> Result<EnrichItemResult, AppError> {
    let (mem_id, body): (i64, String) = conn
        .query_row(
            "SELECT id, body FROM memories WHERE name = ?1 AND deleted_at IS NULL",
            rusqlite::params![item_key],
            |r| Ok((r.get(0)?, r.get::<_, String>(1)?)),
        )
        .map_err(|_| AppError::NotFound(format!("memory '{item_key}' not found")))?;
    let snippet: String = body.chars().take(2000).collect();
    let input_text = format!("Memory: {item_key}\nBody:\n{snippet}");
    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            DEEP_RESEARCH_SYNTH_PROMPT,
            DEEP_RESEARCH_SYNTH_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            DEEP_RESEARCH_SYNTH_PROMPT,
            DEEP_RESEARCH_SYNTH_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
    };
    let mut ent_count = 0usize;
    let mut rel_count = 0usize;
    if let Some(ents) = value.get("entities").and_then(|v| v.as_array()) {
        for e in ents {
            let name = e.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            let etype_str = e
                .get("entity_type")
                .and_then(|v| v.as_str())
                .unwrap_or("concept");
            let etype: EntityType = etype_str.parse().unwrap_or(EntityType::Concept);
            if name.len() >= 2 {
                let ne = NewEntity {
                    name: name.to_string(),
                    entity_type: etype,
                    description: None,
                };
                let _ = entities::upsert_entity(conn, namespace, &ne);
                ent_count += 1;
            }
        }
    }
    if let Some(rels) = value.get("relationships").and_then(|v| v.as_array()) {
        for r in rels {
            let src = r.get("source").and_then(|v| v.as_str()).unwrap_or_default();
            let tgt = r.get("target").and_then(|v| v.as_str()).unwrap_or_default();
            if src.is_empty() || tgt.is_empty() {
                continue;
            }
            let rel = r
                .get("relation")
                .and_then(|v| v.as_str())
                .unwrap_or("related");
            let str_ = r.get("strength").and_then(|v| v.as_f64()).unwrap_or(0.5);
            if let (Some(sid), Some(tid)) = (
                entities::find_entity_id(conn, namespace, src)?,
                entities::find_entity_id(conn, namespace, tgt)?,
            ) {
                let _ = entities::create_or_fetch_relationship(
                    conn, namespace, sid, tid, rel, str_, None,
                );
                rel_count += 1;
            }
        }
    }
    Ok(EnrichItemResult::Done {
        memory_id: Some(mem_id),
        entity_id: None,
        entities: ent_count,
        rels: rel_count,
        chars_before: None,
        chars_after: None,
        cost,
        is_oauth,
    })
}

/// G27 P2: Extract structured body from unstructured text via LLM.
fn call_body_extract(
    conn: &Connection,
    _namespace: &str,
    item_key: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
) -> Result<EnrichItemResult, AppError> {
    let (mem_id, body): (i64, String) = conn
        .query_row(
            "SELECT id, body FROM memories WHERE name = ?1 AND deleted_at IS NULL",
            rusqlite::params![item_key],
            |r| Ok((r.get(0)?, r.get::<_, String>(1)?)),
        )
        .map_err(|_| AppError::NotFound(format!("memory '{item_key}' not found")))?;
    let input_text = format!("Memory: {item_key}\nBody:\n{body}");
    let (value, cost, is_oauth) = match mode {
        EnrichMode::ClaudeCode => call_claude(
            binary,
            BODY_EXTRACT_PROMPT,
            BODY_EXTRACT_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::Codex => call_codex(
            binary,
            BODY_EXTRACT_PROMPT,
            BODY_EXTRACT_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
    };
    let restructured = value
        .get("restructured_body")
        .and_then(|v| v.as_str())
        .unwrap_or(&body);
    let chars_before = body.len();
    let chars_after = restructured.len();
    let new_hash = blake3::hash(restructured.as_bytes()).to_hex().to_string();
    conn.execute(
        "UPDATE memories SET body = ?1, body_hash = ?2, updated_at = unixepoch() WHERE id = ?3",
        rusqlite::params![restructured, new_hash, mem_id],
    )?;
    Ok(EnrichItemResult::Done {
        memory_id: Some(mem_id),
        entity_id: None,
        entities: 0,
        rels: 0,
        chars_before: Some(chars_before),
        chars_after: Some(chars_after),
        cost,
        is_oauth,
    })
}

/// Scan for pairs of entities that share no direct relationship.
#[allow(clippy::type_complexity)]
fn scan_isolated_entity_pairs(
    conn: &Connection,
    namespace: &str,
    limit: Option<usize>,
) -> Result<Vec<(i64, String, i64, String)>, AppError> {
    let limit_val = limit.unwrap_or(50) as i64;
    let mut stmt = conn.prepare_cached(
        "SELECT e1.id, e1.name, e2.id, e2.name FROM entities e1, entities e2 \
         WHERE e1.namespace = ?1 AND e2.namespace = ?1 AND e1.id < e2.id \
         AND NOT EXISTS (SELECT 1 FROM relationships r WHERE \
           (r.source_id = e1.id AND r.target_id = e2.id) OR \
           (r.source_id = e2.id AND r.target_id = e1.id)) \
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![namespace, limit_val], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Scan for entities with non-validated types (all entities for type audit).
fn scan_entities_for_type_validation(
    conn: &Connection,
    namespace: &str,
    limit: Option<usize>,
) -> Result<Vec<(i64, String, String)>, AppError> {
    let limit_clause = limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();
    let sql = format!(
        "SELECT id, name, type FROM entities WHERE namespace = ?1 ORDER BY id {limit_clause}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params![namespace], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Scan for memories with generic descriptions (ingested, imported, etc).
fn scan_generic_descriptions(
    conn: &Connection,
    namespace: &str,
    limit: Option<usize>,
) -> Result<Vec<(i64, String, String)>, AppError> {
    let limit_clause = limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();
    let sql = format!(
        "SELECT id, name, description FROM memories WHERE namespace = ?1 AND deleted_at IS NULL \
         AND (description LIKE '%ingested%' OR description LIKE '%imported%' OR description LIKE '%added%' OR length(description) < 30) \
         ORDER BY id {limit_clause}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params![namespace], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
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

    // G31+G32+G33 (v1.0.69): validate the model BEFORE spawn, write the
    // schema to a trusted cache path (not /tmp), and reuse the
    // consolidated JSONL parser. See `codex_spawn.rs` for the canonical
    // hardening rationale.
    super::codex_spawn::validate_codex_model(model)?;
    let schema_file = super::codex_spawn::trusted_schema_path()?;

    let args = super::codex_spawn::CodexSpawnArgs {
        binary,
        prompt,
        json_schema,
        input_text,
        model,
        timeout_secs,
        schema_path: schema_file.clone(),
    };
    let mut cmd = super::codex_spawn::build_codex_command(&args);

    let mut child = super::claude_runner::spawn_with_memory_limit(&mut cmd).map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to spawn codex: {e}"),
        ))
    })?;

    let full_prompt = format!("{prompt}\n\n{input_text}");
    let stdin_bytes = full_prompt.into_bytes();
    let mut child_stdin = child
        .stdin
        .take()
        .ok_or_else(|| AppError::Validation("failed to open codex stdin".into()))?;
    let stdin_thread = std::thread::spawn(move || -> Result<(), std::io::Error> {
        child_stdin.write_all(&stdin_bytes)?;
        drop(child_stdin);
        Ok(())
    });

    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let status = child.wait_timeout(timeout).map_err(AppError::Io)?;
    let _ = std::fs::remove_file(&schema_file);

    match status {
        Some(exit_status) => {
            stdin_thread
                .join()
                .map_err(|_| AppError::Validation("stdin thread panicked".into()))?
                .map_err(AppError::Io)?;

            tracing::debug!(
                target: "process",
                exit_code = ?exit_status.code(),
                elapsed_ms = start.elapsed().as_millis() as u64,
                "external process completed"
            );

            let mut stdout_buf = Vec::new();
            if let Some(mut out) = child.stdout.take() {
                std::io::Read::read_to_end(&mut out, &mut stdout_buf).map_err(AppError::Io)?;
            }
            if !exit_status.success() {
                let mut stderr_buf = Vec::new();
                if let Some(mut err) = child.stderr.take() {
                    std::io::Read::read_to_end(&mut err, &mut stderr_buf).map_err(AppError::Io)?;
                }
                let stderr_str = String::from_utf8_lossy(&stderr_buf);
                tracing::warn!(
                    target: "enrich",
                    exit_code = ?exit_status.code(),
                    stderr = %stderr_str.trim(),
                    "codex process failed"
                );
                return Err(AppError::Validation(format!(
                    "codex exited with code {:?}: {}",
                    exit_status.code(),
                    stderr_str.trim()
                )));
            }
            let stdout_str = String::from_utf8(stdout_buf)
                .map_err(|_| AppError::Validation("codex stdout is not valid UTF-8".into()))?;
            // G32: use the JSONL parser, NOT serde_json::from_str on the
            // entire stdout (codex emits one event per line).
            let result = super::codex_spawn::parse_codex_jsonl(&stdout_str)?;
            // Return the raw agent_message text parsed as JSON. Different
            // operations (memory-bindings, body-enrich) use different
            // output schemas, so we let the caller pick which fields to
            // extract. The previous implementation hardcoded
            // `{entities, urls}` which broke body-enrich.
            let value: serde_json::Value =
                serde_json::from_str(&result.last_agent_text).map_err(|e| {
                    AppError::Validation(format!(
                        "codex agent_message is not valid JSON: {e}; raw={}",
                        result.last_agent_text
                    ))
                })?;
            Ok((value, 0.0, false))
        }
        None => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdin_thread.join();
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
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

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
            );
            CREATE TABLE memory_embeddings (
                memory_id   INTEGER PRIMARY KEY,
                namespace   TEXT NOT NULL,
                embedding   BLOB NOT NULL,
                source      TEXT NOT NULL,
                model       TEXT NOT NULL DEFAULT '',
                dim         INTEGER NOT NULL DEFAULT 384,
                created_at  INTEGER NOT NULL DEFAULT (unixepoch())
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

        let results = scan_unbound_memories(&conn, "global", None, &[]).unwrap();
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

        let results = scan_unbound_memories(&conn, "global", None, &[]).unwrap();
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
    fn scan_memories_without_embeddings_finds_only_missing_rows() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'missing-vec', 'body one')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'has-vec', 'body two')",
            [],
        )
        .unwrap();
        let memory_id: i64 = conn
            .query_row(
                "SELECT id FROM memories WHERE namespace='global' AND name='has-vec'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let embedding = vec![0.0_f32; crate::constants::embedding_dim()];
        memories::upsert_vec(
            &conn, memory_id, "global", "note", &embedding, "has-vec", "body two",
        )
        .unwrap();

        let results = scan_memories_without_embeddings(&conn, "global", None, &[]).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "missing-vec");
    }

    #[test]
    fn scan_memories_without_embeddings_respects_name_filter() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'match-me', 'body one')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'skip-me', 'body two')",
            [],
        )
        .unwrap();

        let results =
            scan_memories_without_embeddings(&conn, "global", None, &["match-me".to_string()])
                .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "match-me");
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
    fn parse_claude_output_valid_bindings() {
        let output = r#"[
            {"type":"system","subtype":"init"},
            {"type":"result","is_error":false,"total_cost_usd":0.01,
             "structured_output":{"entities":[{"name":"rust-lang","entity_type":"tool"}],"relationships":[]}}
        ]"#;
        let result = crate::commands::claude_runner::parse_claude_output(output)
            .expect("must parse successfully");
        assert!(result.value.get("entities").is_some());
        assert!((result.cost_usd - 0.01).abs() < f64::EPSILON);
        assert!(!result.is_oauth);
    }

    #[test]
    fn parse_claude_output_detects_oauth() {
        let output = r#"[
            {"type":"system","subtype":"init","apiKeySource":"none"},
            {"type":"result","is_error":false,"total_cost_usd":0.0,
             "structured_output":{"entities":[],"relationships":[]}}
        ]"#;
        let result = crate::commands::claude_runner::parse_claude_output(output).unwrap();
        assert!(result.is_oauth);
    }

    #[test]
    fn parse_claude_output_rate_limit_returns_error() {
        let output = r#"[
            {"type":"system","subtype":"init"},
            {"type":"result","is_error":true,"error":"rate_limit exceeded"}
        ]"#;
        let err = crate::commands::claude_runner::parse_claude_output(output).unwrap_err();
        assert!(matches!(err, AppError::RateLimited { .. }));
    }

    #[test]
    fn parse_claude_output_auth_error() {
        let output = r#"[
            {"type":"system","subtype":"init"},
            {"type":"result","is_error":true,"error":"authentication failed"}
        ]"#;
        let err = crate::commands::claude_runner::parse_claude_output(output).unwrap_err();
        assert!(format!("{err}").contains("authentication failed"));
    }

    #[cfg(unix)]
    #[test]
    fn call_codex_returns_raw_json_for_body_enrich_schema() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let binary = tmp.path().join("codex-mock");
        std::fs::write(
            &binary,
            r#"#!/usr/bin/env bash
set -euo pipefail
cat <<'JSONL'
{"type":"thread.started","thread_id":"mock-thread-0"}
{"type":"item.completed","item":{"type":"agent_message","text":"{\"enriched_body\":\"expanded body\"}"}}
{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}
JSONL
"#,
        )
        .expect("mock codex write");
        let mut perms = std::fs::metadata(&binary).expect("metadata").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&binary, perms).expect("chmod");

        let (value, cost, is_oauth) =
            call_codex(&binary, "prompt", BODY_ENRICH_SCHEMA, "body", None, 5)
                .expect("call_codex must accept body-enrich payload");

        assert_eq!(value["enriched_body"], "expanded body");
        assert_eq!(cost, 0.0);
        assert!(!is_oauth);
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
