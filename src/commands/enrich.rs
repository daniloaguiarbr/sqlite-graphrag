// TODO v1.0.89: este arquivo tem 4116 linhas — modularização planejada.
// Ver ADR-0046 seção "Known Tech Debt (v1.0.89+)".

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
    /// memory-bindings VINCULA cada memória às entidades EXISTENTES extraídas do
    /// seu corpo — não inventa um novo grafo, apenas conecta o que falta. Scans
    /// only UNBOUND memories (those with zero `memory_entities`).
    MemoryBindings,
    /// GAP-SG-24/26: additive augmentation — re-run binding extraction over
    /// memories that are ALREADY bound, filtered by `--names`/`--names-file`, to
    /// merge newly-discovered entities/relationships WITHOUT removing existing
    /// links. Requires a name filter (refuses to re-scan the whole namespace).
    AugmentBindings,
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
    /// Use locally installed OpenCode CLI.
    #[value(name = "opencode")]
    Opencode,
    /// Use the OpenRouter chat-completions REST API (no local CLI; v1.0.95).
    #[value(name = "openrouter")]
    OpenRouter,
}

impl std::fmt::Display for EnrichMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnrichMode::ClaudeCode => write!(f, "claude-code"),
            EnrichMode::Codex => write!(f, "codex"),
            EnrichMode::Opencode => write!(f, "opencode"),
            EnrichMode::OpenRouter => write!(f, "openrouter"),
        }
    }
}

/// Arguments for the `enrich` subcommand.
#[derive(clap::Args)]
#[command(
    about = "Enrich graph memories and entities using an LLM provider",
    after_long_help = "EXAMPLES:\n  \
    # Add missing entity bindings to all unbound memories\n  \
    sqlite-graphrag enrich --operation memory-bindings --mode codex --codex-model gpt-5.4-mini\n\n  \
    # Fill entity descriptions (dry-run preview, no tokens spent)\n  \
    sqlite-graphrag enrich --operation entity-descriptions --dry-run --json\n\n  \
    # Expand short memory bodies (GAP-18)\n  \
    sqlite-graphrag enrich --operation body-enrich --min-output-chars 600\n\n  \
    # Rebuild only missing memory embeddings without rewriting bodies\n  \
    sqlite-graphrag enrich --operation re-embed --limit 100\n\n  \
    # Resume an interrupted body-enrich run\n  \
    sqlite-graphrag enrich --operation body-enrich --resume --json\n\n  \
    # Retry only failed items from a previous run\n  \
    sqlite-graphrag enrich --operation memory-bindings --retry-failed --json\n\n  \
    # Converge the whole backlog (internal scan+drain loop, no bash wrapper)\n  \
    sqlite-graphrag enrich --operation memory-bindings --mode openrouter \\\n    \
      --openrouter-model deepseek/deepseek-v4-flash:nitro --until-empty --max-runtime 600\n\n  \
    # Inspect / resurrect dead-letter items\n  \
    sqlite-graphrag enrich --operation memory-bindings --list-dead\n  \
    sqlite-graphrag enrich --operation memory-bindings --requeue-dead\n\n  \
    # Read-only status (no LLM, no singleton)\n  \
    sqlite-graphrag enrich --operation memory-bindings --status\n\n\
    OPERATIONS NOTE:\n  \
    memory-bindings LINKS each memory to the EXISTING entities extracted from its\n  \
    body — it does not invent a new graph, it connects what is missing. It scans\n  \
    only UNBOUND memories. To re-run extraction over ALREADY-bound memories and\n  \
    MERGE newly-found entities/relationships additively (without removing links),\n  \
    use --operation augment-bindings with --names/--names-file.\n\n\
    DEAD-LETTER SIDECAR (.enrich-queue.sqlite):\n  \
    A SQLite sidecar tracks each work item across runs. Schema (table `queue`):\n  \
    item_key (UNIQUE name/id), item_type (memory|entity), operation, memory_id,\n  \
    status (pending|processing|done|skipped|dead), attempt, error, error_class,\n  \
    next_retry_at (backoff cooldown). --until-empty loops scan→drain internally\n  \
    until eligible items are exhausted; transient failures (incl. malformed/non-\n  \
    JSON LLM output, GAP-SG-09) reschedule with backoff until --max-attempts, then\n  \
    land in status='dead'. Use --status to see the queue, --list-dead to inspect\n  \
    the sink, --requeue-dead to retry it, and --ignore-backoff to skip cooldowns.\n  \
    --names/--names-file also remedy a cooldown by targeting a specific subset.\n\n\
    EXIT CODES:\n  \
    0  success\n  \
    1  validation error (bad args, binary not found)\n  \
    14 I/O error"
)]
pub struct EnrichArgs {
    /// Enrichment operation to run. Required for write operations; optional for
    /// the read-only queue inspectors (`--status` / `--list-dead` /
    /// `--requeue-dead`), where it defaults to `memory-bindings` when omitted
    /// (GAP-SG-31).
    #[arg(
        long,
        short = 'o',
        value_enum,
        value_name = "OPERATION",
        required_unless_present_any = ["status", "list_dead", "requeue_dead"]
    )]
    pub operation: Option<EnrichOperation>,

    /// LLM provider to use. Required for write operations; not needed for the
    /// read-only queue inspectors (`--status` / `--list-dead` /
    /// `--requeue-dead`), which never call the LLM (GAP-SG-31).
    #[arg(
        long,
        value_enum,
        required_unless_present_any = ["status", "list_dead", "requeue_dead"]
    )]
    pub mode: Option<EnrichMode>,

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

    // -- Provider flags (OpenCode) --
    /// Path to the OpenCode binary. Default: auto-detect from PATH.
    #[arg(long, value_name = "PATH", env = "SQLITE_GRAPHRAG_OPENCODE_BINARY")]
    pub opencode_binary: Option<PathBuf>,

    /// OpenCode model to use.
    #[arg(long, value_name = "MODEL", env = "SQLITE_GRAPHRAG_OPENCODE_MODEL")]
    pub opencode_model: Option<String>,

    /// Timeout per item in seconds when using OpenCode. Default: 300.
    #[arg(
        long,
        value_name = "SECONDS",
        env = "SQLITE_GRAPHRAG_OPENCODE_TIMEOUT",
        default_value_t = 300
    )]
    pub opencode_timeout: u64,

    // -- Provider flags (OpenRouter, v1.0.95) --
    /// OpenRouter text model to use (REQUIRED with --mode openrouter; no default).
    #[arg(long, value_name = "MODEL")]
    pub openrouter_model: Option<String>,

    /// OpenRouter API key. Falls back to OPENROUTER_API_KEY env or stored config.
    #[arg(long, value_name = "KEY", env = "OPENROUTER_API_KEY")]
    pub openrouter_api_key: Option<String>,

    /// Timeout per item in seconds when using OpenRouter. Default: 600.
    ///
    /// GAP-SG-17: raised from 300 to 600 because dense bodies (close to the
    /// ~32K-token context ceiling of the configured model) routinely take
    /// longer than five minutes to generate via `deepseek-v4-flash:nitro`.
    /// Raise it further for very large corpora; lower it for short snippets.
    #[arg(long, value_name = "SECONDS", default_value_t = 600)]
    pub openrouter_timeout: u64,

    /// Optional OpenRouter base URL override (reserved; defaults to the public API).
    #[arg(long, value_name = "URL")]
    pub openrouter_base_url: Option<String>,

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

    /// GAP-ENRICH-BACKLOG-CONVERGE: loop scan→drain internally until the queue
    /// empties of eligible items or --max-runtime elapses; removes the need for
    /// an external bash retry loop.
    #[arg(long)]
    pub until_empty: bool,

    /// GAP-ENRICH-BACKLOG-CONVERGE: wall-clock ceiling in seconds for
    /// --until-empty. Defaults to 3600 when omitted.
    #[arg(long, value_name = "SECONDS")]
    pub max_runtime: Option<u64>,

    /// GAP-ENRICH-BACKLOG-CONVERGE: attempts per item before it becomes a
    /// dead-letter (status='dead'). Range 1..=20. Default 8.
    ///
    /// GAP-SG-21: the default was raised from 5 to 8 because GAP-SG-09 now
    /// reclassifies malformed / non-JSON LLM output as TRANSIENT (retryable)
    /// rather than a permanent HardFailure. A flaky structured-output model
    /// (e.g. deepseek-v4-flash:nitro) may emit several bad generations in a row
    /// even after JSON repair (GAP-SG-10) recovers most of them; the extra
    /// attempts give the backlog room to converge before an item is parked in
    /// the dead-letter sink. Permanent faults (ProviderError, NotFound) still
    /// dead-letter on the first attempt regardless of this value.
    #[arg(long, value_name = "N", default_value_t = 8, value_parser = clap::value_parser!(u32).range(1..=20))]
    pub max_attempts: u32,

    /// GAP-ENRICH-BACKLOG-CONVERGE: read-only mode — report queue and backlog
    /// counts without calling the LLM or acquiring the singleton.
    #[arg(long)]
    pub status: bool,

    /// GAP-SG-23: list every dead-letter item (status='dead') for the current
    /// operation with its error_class, attempt count and last error message.
    /// Read-only — no LLM, no singleton. Use it to inspect what `--requeue-dead`
    /// would resurrect before running it.
    #[arg(long)]
    pub list_dead: bool,

    /// GAP-SG-11/14: resurrect dead-letter items — move every `status='dead'`
    /// row back to `pending`, zeroing `attempt`, `next_retry_at`, `error` and
    /// `error_class`. Distinct from `--retry-failed`, which only resets the
    /// legacy `status='failed'` rows; dead-letter rows are the terminal sink of
    /// the v1.0.96 converge loop and are never re-selected without this flag.
    /// No LLM call or singleton is taken — it only rewrites queue statuses.
    #[arg(long)]
    pub requeue_dead: bool,

    /// GAP-SG-16: ignore the per-item backoff cooldown (`next_retry_at`) when
    /// selecting candidates, so items waiting on exponential backoff are
    /// processed immediately. Use to drain a backlog whose cooldown windows are
    /// long but the provider has recovered. Without it, `--status` reports such
    /// items under `waiting` and they are skipped until their `next_retry_at`.
    #[arg(long)]
    pub ignore_backoff: bool,

    /// GAP-SG-28: read-only `body-extract` — extract entities/relationships into
    /// the graph WITHOUT rewriting (or truncating) the memory body. The default
    /// `body-extract` restructures the stored body in place; with this flag the
    /// body is left untouched and only graph bindings are persisted (additive,
    /// via the same upsert path as `memory-bindings`). Ignored for every other
    /// operation.
    #[arg(long)]
    pub body_extract_graph_only: bool,

    /// GAP-ENRICH-BACKLOG-CONVERGE: REST concurrency for --mode openrouter
    /// (clamp 1..=16, default 8). Distinct from the legacy --llm-parallelism.
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..=16))]
    pub rest_concurrency: Option<u32>,

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
    ///
    /// GAP-SG-18: also a cooldown remedy — when `--status` shows items under
    /// `waiting` (parked on `next_retry_at` backoff), pass the exact names here
    /// to re-enqueue and process just that subset on the next run instead of
    /// waiting for every cooldown to elapse. REQUIRED for `--operation
    /// augment-bindings`, which refuses to re-scan the whole namespace.
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

impl EnrichArgs {
    /// GAP-SG-31: resolved enrichment operation.
    ///
    /// `operation` is `Option` so the read-only queue inspectors
    /// (`--status` / `--list-dead` / `--requeue-dead`) can run without it.
    /// Write paths always carry a value (enforced by
    /// `required_unless_present_any` at parse time); the read-only paths fall
    /// back to `memory-bindings`, the most common queue, when it is omitted.
    fn operation(&self) -> EnrichOperation {
        self.operation
            .clone()
            .unwrap_or(EnrichOperation::MemoryBindings)
    }

    /// GAP-SG-31: resolved LLM provider. `mode` is `Option` for the read-only
    /// inspectors that never call the LLM; write paths always carry a value
    /// (enforced by `required_unless_present_any`). The fallback is only ever
    /// observed by read-only code that does not actually invoke the provider.
    fn mode(&self) -> EnrichMode {
        self.mode.clone().unwrap_or(EnrichMode::OpenRouter)
    }
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

/// GAP-SG-45: separates the SCAN metric (always serial — a single SQL sweep of
/// the candidate set) from the DRAIN metric (the parallel worker fan-out). The
/// legacy "scan" `PhaseEvent` reported `llm_parallelism` on the scan event,
/// conflating the two; this event makes the distinction explicit.
#[derive(Debug, Serialize)]
struct ConcurrencyEvent {
    phase: &'static str,
    scan_parallelism: u32,
    drain_parallelism: u32,
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
    /// GAP-SG-15: items still parked on backoff (`status='pending'` with a future
    /// `next_retry_at`) when the run ended. Non-zero means the backlog has NOT
    /// converged — those items are waiting on a cooldown, not done.
    waiting: i64,
    /// GAP-SG-15: dead-letter items (`status='dead'`) at the end of the run.
    /// Non-zero requires `--list-dead` to inspect and `--requeue-dead` to retry.
    dead: i64,
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
    // GAP-ENRICH-BACKLOG-CONVERGE (v1.0.96): dead-letter columns. The legacy
    // `.enrich-queue.sqlite` predates these columns and `CREATE TABLE IF NOT
    // EXISTS` never alters an existing table, so add them idempotently here.
    let mut has_error_class = false;
    let mut has_next_retry_at = false;
    // GAP-SG-12/42: the `operation` column scopes queue rows to the enrich
    // operation that enqueued them, so `--status` can segment counts per
    // operation instead of conflating a shared `item_key` space. Migrated
    // idempotently here for the same reason as the v1.0.96 columns.
    let mut has_operation = false;
    {
        let mut stmt = conn.prepare("PRAGMA table_info(queue)")?;
        let names = stmt.query_map([], |r| r.get::<_, String>(1))?;
        for name in names {
            match name?.as_str() {
                "error_class" => has_error_class = true,
                "next_retry_at" => has_next_retry_at = true,
                "operation" => has_operation = true,
                _ => {}
            }
        }
    }
    if !has_error_class {
        conn.execute_batch("ALTER TABLE queue ADD COLUMN error_class TEXT")?;
    }
    if !has_next_retry_at {
        conn.execute_batch("ALTER TABLE queue ADD COLUMN next_retry_at TEXT")?;
    }
    if !has_operation {
        conn.execute_batch("ALTER TABLE queue ADD COLUMN operation TEXT")?;
    }
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_enrich_queue_eligible ON queue(status, next_retry_at);
         CREATE INDEX IF NOT EXISTS idx_enrich_queue_operation ON queue(operation, status);
         CREATE INDEX IF NOT EXISTS idx_enrich_queue_memory ON queue(memory_id)",
    )?;
    Ok(conn)
}

/// GAP-SG-12: enqueue one scan candidate, linking it to its `memory_id` and
/// tagging it with the originating `operation`. For memory-keyed operations the
/// id is resolved from `main_conn` so the cascade cleanup (GAP-SG-13) can target
/// the queue row by `memory_id` even before the item is processed. Entity/id
/// keyed operations leave `memory_id` NULL (the `item_key` carries the link).
/// `INSERT OR IGNORE` preserves the v1.0.96 invariant that a dead-letter row is
/// never resurrected by re-enqueue (item_key is UNIQUE).
fn enqueue_candidate(
    queue_conn: &Connection,
    main_conn: &Connection,
    namespace: &str,
    key: &str,
    item_type: &str,
    operation: &str,
) {
    let memory_id: Option<i64> = if item_type == "memory" {
        main_conn
            .query_row(
                "SELECT id FROM memories WHERE namespace=?1 AND name=?2 AND deleted_at IS NULL",
                rusqlite::params![namespace, key],
                |r| r.get(0),
            )
            .ok()
    } else {
        None
    };
    if let Err(e) = queue_conn.execute(
        "INSERT OR IGNORE INTO queue (item_key, item_type, status, operation, memory_id) \
         VALUES (?1, ?2, 'pending', ?3, ?4)",
        rusqlite::params![key, item_type, operation, memory_id],
    ) {
        tracing::warn!(target: "enrich", error = %e, "queue insert failed");
    }
}

/// Queue `item_type` for an operation: entity-keyed operations use `"entity"`,
/// every other (memory/id-keyed) operation uses `"memory"`.
fn item_type_for(operation: &EnrichOperation) -> &'static str {
    match operation {
        EnrichOperation::EntityDescriptions => "entity",
        _ => "memory",
    }
}

/// GAP-SG-13: remove a memory's enrich-queue entry when the memory is deleted or
/// force-merged, so the dead-letter / pending sidecar never references a row
/// that no longer exists. Best-effort and a no-op when the queue file is absent
/// (the common case after a clean run, which removes it). Targets BOTH
/// `memory_id` (populated at enqueue for memory ops, GAP-SG-12) and `item_key`
/// (the memory name) so pending rows enqueued before id resolution are also
/// cleared. Errors are logged, never propagated — cleanup must not fail the
/// caller's delete/upsert.
pub fn cleanup_queue_entry(memory_id: i64, name: &str) {
    if !std::path::Path::new(DEFAULT_QUEUE_DB).exists() {
        return;
    }
    match open_queue_db(DEFAULT_QUEUE_DB) {
        Ok(conn) => {
            if let Err(e) = conn.execute(
                "DELETE FROM queue WHERE memory_id = ?1 OR item_key = ?2",
                rusqlite::params![memory_id, name],
            ) {
                tracing::warn!(target: "enrich", error = %e, memory_id, "enrich-queue cleanup failed");
            }
        }
        Err(e) => {
            tracing::warn!(target: "enrich", error = %e, "enrich-queue cleanup skipped (open failed)");
        }
    }
}

// ---------------------------------------------------------------------------
// GAP-ENRICH-BACKLOG-CONVERGE — dead-letter classification + queue failure sink
// ---------------------------------------------------------------------------

/// Read-only `enrich --status` report (no LLM, no singleton).
///
/// GAP-SG-42: all queue counts are scoped to the current `--operation` (rows
/// migrated before the `operation` column, which are NULL, are still counted so
/// a legacy queue is not silently reported as empty).
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct EnrichStatus {
    status_report: bool,
    operation: String,
    namespace: String,
    unbound_backlog: usize,
    queue_pending: i64,
    queue_processing: i64,
    queue_done: i64,
    queue_failed: i64,
    queue_skipped: i64,
    queue_dead: i64,
    eligible_now: i64,
    waiting: i64,
    /// GAP-SG-15/46: coarse backlog state, disambiguating an empty queue from a
    /// not-yet-scanned backlog and from a cooldown wait.
    /// `draining` (eligible items now) | `cooldown` (all pending items waiting on
    /// `next_retry_at`) | `pending-scan` (candidates exist but the queue is not
    /// populated — run enrich to scan) | `empty` (nothing left to do).
    state: &'static str,
    /// GAP-SG-16: per-item `next_retry_at` for every pending row currently in
    /// backoff, so an operator can see exactly when each will become eligible.
    waiting_items: Vec<WaitingItem>,
}

/// GAP-SG-16: one pending queue row waiting on its backoff cooldown.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct WaitingItem {
    item_key: String,
    attempt: i64,
    next_retry_at: Option<String>,
    error_class: Option<String>,
}

/// GAP-SG-23: one dead-letter row reported by `--list-dead`.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct DeadItem {
    dead_item: bool,
    item_key: String,
    item_type: String,
    attempt: i64,
    error_class: Option<String>,
    error: Option<String>,
}

/// GAP-SG-23/11: summary footer for `--list-dead` and `--requeue-dead`.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct DeadSummary {
    summary: bool,
    operation: String,
    namespace: String,
    /// `list-dead` | `requeue-dead`
    action: &'static str,
    dead_total: i64,
    requeued: i64,
}

/// Classifies an enrich item failure into a retry/dead-letter outcome.
///
/// Transient errors (rate-limit, timeout, db-busy, or a message that smells
/// like a recoverable network/5xx hiccup) are rescheduled with backoff.
/// Everything else — validation, parse, invalid body, unknown — is a permanent
/// `HardFailure` routed to the dead-letter sink so the backlog can converge.
fn classify_enrich_outcome(e: &AppError) -> crate::retry::AttemptOutcome {
    use crate::retry::AttemptOutcome;
    match e {
        AppError::RateLimited { .. } | AppError::Timeout { .. } | AppError::DbBusy(_) => {
            AttemptOutcome::Transient
        }
        // GAP-SG-09: errors that are genuinely PERMANENT for this item and must
        // dead-letter immediately (retrying cannot help): a structured provider
        // rejection (context-length overflow / refusal carried as ProviderError),
        // or a memory/entity that no longer exists (deleted between scan and
        // processing).
        AppError::ProviderError { .. }
        | AppError::NotFound(_)
        | AppError::MemoryNotFound { .. }
        | AppError::MemoryNotFoundById { .. } => AttemptOutcome::HardFailure,
        _ => {
            let msg = format!("{e}").to_lowercase();
            if msg.contains("server error")
                || msg.contains("timed out")
                || msg.contains("timeout")
                || msg.contains("connection")
                || msg.contains("5xx")
                || msg.contains("502")
                || msg.contains("503")
                || msg.contains("504")
            {
                AttemptOutcome::Transient
            } else if msg.contains("json")
                || msg.contains("no structured content")
                || msg.contains("non-object")
                || msg.contains("missing '")
            {
                // GAP-SG-09: malformed / non-JSON / shape-invalid LLM output is a
                // model HICCUP, not a permanent fault. deepseek-v4-flash:nitro
                // emits the occasional non-JSON or shape-wrong generation; with
                // strict-parse + repair (GAP-SG-10) most are recovered, and the
                // rest must be RESCHEDULED with backoff (bounded by
                // --max-attempts) instead of dead-lettering on the first try.
                AttemptOutcome::Transient
            } else {
                AttemptOutcome::HardFailure
            }
        }
    }
}

/// Applies a failure outcome to a single queue row. Shared by the parallel
/// worker and the serial loop (DRY). A `HardFailure`, or a transient failure
/// whose attempt count reached `max_attempts`, lands in the dead-letter status
/// (`status='dead'`) so it is never re-selected. A transient failure below the
/// cap is rescheduled to `pending` with an exponential-backoff `next_retry_at`.
/// Returns the [`crate::retry::AttemptOutcome`] so the caller can feed the
/// existing circuit breaker.
fn record_item_failure(
    queue_conn: &rusqlite::Connection,
    queue_id: i64,
    attempt: i64,
    max_attempts: u32,
    err: &AppError,
) -> crate::retry::AttemptOutcome {
    use crate::retry::AttemptOutcome;
    let outcome = classify_enrich_outcome(err);
    let err_str = format!("{err}");
    let error_class = match outcome {
        AttemptOutcome::Transient => "transient",
        AttemptOutcome::HardFailure => "permanent",
        AttemptOutcome::Success => "success",
    };

    let terminal = matches!(outcome, AttemptOutcome::HardFailure) || attempt >= max_attempts as i64;
    if terminal {
        let _ = queue_conn.execute(
            "UPDATE queue SET status='dead', error=?1, error_class=?2, done_at=datetime('now') WHERE id=?3",
            rusqlite::params![err_str, error_class, queue_id],
        );
    } else {
        let delay = crate::retry::compute_delay(
            &crate::retry::RetryConfig::llm_rate_limit(),
            attempt.max(0) as u32,
        );
        let secs = delay.as_secs().max(1);
        let modifier = format!("+{secs} seconds");
        let _ = queue_conn.execute(
            "UPDATE queue SET status='pending', error=?1, error_class=?2, next_retry_at=datetime('now', ?3) WHERE id=?4",
            rusqlite::params![err_str, error_class, modifier, queue_id],
        );
    }
    outcome
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

/// v1.0.95 (ADR-0054): route a single JUDGE turn through the OpenRouter
/// chat-completions REST API. Unlike the subprocess runners there is no
/// `binary` argument: the process-wide chat client (initialised in `run()`
/// before scan) is fetched from the singleton and driven synchronously via
/// the shared tokio runtime. Returns `(value, cost_usd, is_oauth=false)`
/// where `cost_usd` is read from the response `usage.cost`.
fn call_openrouter(
    prompt: &str,
    json_schema: &str,
    input_text: &str,
    model: Option<&str>,
    timeout_secs: u64,
) -> Result<(serde_json::Value, f64, bool), AppError> {
    // `model` is bound into the client singleton at init; `timeout_secs` is
    // enforced by the reqwest builder. Both remain in the signature for
    // parity with the subprocess runners.
    let _ = (model, timeout_secs);
    let client = crate::embedder::openrouter_chat_client().ok_or_else(|| {
        AppError::Validation(
            "OpenRouter chat client not initialised before dispatch (internal error)".into(),
        )
    })?;
    let runtime = crate::embedder::shared_runtime()?;
    runtime.block_on(client.complete(prompt, input_text, json_schema, None))
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

    match args.mode() {
        EnrichMode::ClaudeCode => {
            let bin = match find_claude_binary(args.claude_binary.as_deref()) {
                Ok(b) => b,
                Err(e) => return PreflightOutcome::Error(e),
            };
            // v1.0.88 (BUG-3 fix, ADR-0046): write the empty MCP config to a
            // tempfile (Claude Code 2.1.177 rejects the inline `{}`
            // form) and run the preflight gate before spawn, mirroring
            // the canonical pattern in `claude_runner::build_claude_command`.
            let mcp_config_path = match crate::spawn::preflight::write_empty_mcp_config_tempfile() {
                Ok(p) => p,
                Err(e) => {
                    return PreflightOutcome::Error(AppError::Io(e));
                }
            };
            let mut cmd = std::process::Command::new(&bin);
            crate::spawn::env_whitelist::apply_env_whitelist(
                &mut cmd,
                crate::spawn::env_whitelist::is_strict_env_clear(),
            );
            if let Err(e) = crate::spawn::apply_cwd_isolation(&mut cmd) {
                return PreflightOutcome::Error(e);
            }
            cmd.arg("-p")
                .arg("ping")
                .arg("--max-turns")
                .arg("1")
                .arg("--strict-mcp-config")
                .arg("--mcp-config")
                .arg(mcp_config_path.as_os_str())
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
            let mut cmd = match super::codex_spawn::build_codex_command(&spawn_args) {
                Ok(c) => c,
                Err(e) => return PreflightOutcome::Error(e),
            };
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
        EnrichMode::Opencode => {
            let bin = match super::opencode_runner::find_opencode_binary_with_override(
                args.opencode_binary.as_deref(),
            ) {
                Ok(b) => b,
                Err(e) => return PreflightOutcome::Error(e),
            };
            let model =
                super::opencode_runner::resolve_opencode_model(args.opencode_model.as_deref());
            let mut cmd =
                match super::opencode_runner::build_opencode_command_sync(&bin, &model, "ping", "")
                {
                    Ok(c) => c,
                    Err(e) => return PreflightOutcome::Error(e),
                };
            let child = match super::opencode_runner::spawn_opencode(&mut cmd) {
                Ok(c) => c,
                Err(e) => return PreflightOutcome::Error(AppError::Io(e)),
            };
            let output = match wait_with_timeout(child, timeout) {
                Ok(out) => out,
                Err(e) => return PreflightOutcome::Error(e),
            };
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
        EnrichMode::OpenRouter => {
            // v1.0.95: the OpenRouter JUDGE has no subprocess to ping; the
            // preflight only confirms a usable API key resolves. The chat
            // client singleton is initialised in run() before scan.
            match crate::config::resolve_api_key("openrouter", args.openrouter_api_key.as_deref()) {
                Some(_) => PreflightOutcome::Healthy,
                None => PreflightOutcome::Error(AppError::Validation(
                    "OPENROUTER_API_KEY not found for --mode openrouter preflight".into(),
                )),
            }
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

/// GAP-SG-24/26: returns ALREADY-bound memory names for additive augmentation,
/// restricted to `name_filter`.
///
/// Unlike [`scan_unbound_memories`] this selects memories that DO have at least
/// one `memory_entities` binding, so a second extraction pass can merge newly
/// discovered entities/relationships without disturbing existing links (the
/// persist path is purely additive). A name filter is MANDATORY: re-running
/// extraction over an entire namespace is expensive and rarely intended, so an
/// empty filter is rejected rather than silently scanning everything.
fn scan_bound_memories_for_augment(
    conn: &Connection,
    namespace: &str,
    limit: Option<usize>,
    name_filter: &[String],
) -> Result<Vec<String>, AppError> {
    if name_filter.is_empty() {
        return Err(AppError::Validation(
            "augment-bindings requires an explicit subset: pass --names or \
             --names-file (it refuses to re-scan the whole namespace)"
                .into(),
        ));
    }
    let limit_clause = limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();
    let placeholders: Vec<String> = (2..=name_filter.len() + 1)
        .map(|i| format!("?{i}"))
        .collect();
    let in_clause = placeholders.join(", ");
    let sql = format!(
        "SELECT m.name
         FROM memories m
         WHERE m.namespace = ?1
           AND m.deleted_at IS NULL
           AND m.name IN ({in_clause})
           AND EXISTS (
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
            |r| r.get::<_, String>(0),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
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
    name_filter: &[String],
) -> Result<Vec<(i64, String, String)>, AppError> {
    let limit_clause = limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();

    if name_filter.is_empty() {
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
    } else {
        let placeholders: Vec<String> = (2..=name_filter.len() + 1)
            .map(|i| format!("?{i}"))
            .collect();
        let in_clause = placeholders.join(", ");
        let sql = format!(
            "SELECT id, name, type
             FROM entities
             WHERE namespace = ?1
               AND name IN ({in_clause})
               AND (description IS NULL OR description = '')
             ORDER BY id
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

/// Returns memories whose body length is below the configured minimum.
///
/// These are the targets for `body-enrich` (GAP-18).
fn scan_short_body_memories(
    conn: &Connection,
    namespace: &str,
    min_chars: usize,
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
    } else {
        let placeholders: Vec<String> = (3..=name_filter.len() + 2)
            .map(|i| format!("?{i}"))
            .collect();
        let in_clause = placeholders.join(", ");
        let sql = format!(
            "SELECT m.id, m.name, m.body
             FROM memories m
             WHERE m.namespace = ?1
               AND m.deleted_at IS NULL
               AND m.name IN ({in_clause})
               AND LENGTH(COALESCE(m.body,'')) < ?2
             ORDER BY m.id
             {limit_clause}"
        );
        let mut params_vec: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(2 + name_filter.len());
        let min_chars_i64 = min_chars as i64;
        params_vec.push(&namespace);
        params_vec.push(&min_chars_i64);
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
        // GAP-SG-47: fold non-canonical labels onto the nearest canonical kind
        // instead of discarding the entity (no silent data loss).
        let entity_type = EntityType::map_to_canonical(&item.entity_type);
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
        // GAP-SG-48: rewrite non-canonical relations to canonical instead of
        // accepting them raw with only a warning.
        let normalized = crate::parsers::map_to_canonical_relation(&rel.relation);

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
    embedding_backend: crate::cli::EmbeddingBackendChoice,
) -> Result<(), AppError> {
    let snippet: String = body.chars().take(200).collect();
    // v1.0.82 (GAP-003): forward --llm-backend to embed_with_fallback.
    // v1.0.84 (ADR-0042): tuple (Vec<f32>, LlmBackendKind) — extrai o
    // backend que efetivamente rodou e popula o accumulator para o
    // EnrichSummary agregado.
    // v1.0.93 (GAP-OR-PROPAGATION): honour --embedding-backend openrouter.
    let (embedding, backend_kind) = crate::embedder::embed_passage_with_embedding_choice(
        &paths.models,
        body,
        embedding_backend,
        llm_backend,
    )?;
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
#[allow(clippy::too_many_arguments)]
fn persist_enriched_body(
    conn: &Connection,
    namespace: &str,
    memory_id: i64,
    memory_name: &str,
    new_body: &str,
    paths: &crate::paths::AppPaths,
    llm_backend: crate::cli::LlmBackendChoice,
    embedding_backend: crate::cli::EmbeddingBackendChoice,
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
        embedding_backend,
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

    match args.mode() {
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
        EnrichMode::Opencode => {
            if args.claude_binary.is_some() {
                conflicts.push("--claude-binary is ignored when --mode=opencode".to_string());
            }
            if args.claude_model.is_some() {
                conflicts.push("--claude-model is ignored when --mode=opencode".to_string());
            }
            if !is_at_default(args.claude_timeout, DEFAULT_TIMEOUT) {
                conflicts.push(format!(
                    "--claude-timeout={} is ignored when --mode=opencode (remove the flag to use the default 300s)",
                    args.claude_timeout
                ));
            }
            if args.max_cost_usd.is_some() {
                conflicts.push(
                    "--max-cost-usd is ignored when --mode=opencode (OAuth-first; cost is metered by your subscription, not the call)"
                        .to_string(),
                );
            }
        }
        EnrichMode::OpenRouter => {
            if args.claude_binary.is_some() {
                conflicts.push("--claude-binary is ignored when --mode=openrouter".to_string());
            }
            if args.claude_model.is_some() {
                conflicts.push("--claude-model is ignored when --mode=openrouter".to_string());
            }
            if args.codex_binary.is_some() {
                conflicts.push("--codex-binary is ignored when --mode=openrouter".to_string());
            }
            if args.codex_model.is_some() {
                conflicts.push("--codex-model is ignored when --mode=openrouter".to_string());
            }
            if args.opencode_binary.is_some() {
                conflicts.push("--opencode-binary is ignored when --mode=openrouter".to_string());
            }
            if args.opencode_model.is_some() {
                conflicts.push("--opencode-model is ignored when --mode=openrouter".to_string());
            }
            if !is_at_default(args.claude_timeout, DEFAULT_TIMEOUT) {
                conflicts.push(format!(
                    "--claude-timeout={} is ignored when --mode=openrouter (remove the flag to use the default 300s)",
                    args.claude_timeout
                ));
            }
            if !is_at_default(args.codex_timeout, DEFAULT_TIMEOUT) {
                conflicts.push(format!(
                    "--codex-timeout={} is ignored when --mode=openrouter (remove the flag to use the default 300s)",
                    args.codex_timeout
                ));
            }
            if !is_at_default(args.opencode_timeout, DEFAULT_TIMEOUT) {
                conflicts.push(format!(
                    "--opencode-timeout={} is ignored when --mode=openrouter (remove the flag to use the default 300s)",
                    args.opencode_timeout
                ));
            }
        }
    }

    if !conflicts.is_empty() {
        return Err(AppError::Validation(format!(
            "G20: mode-conditional flag conflicts detected for --mode={}:\n  - {}",
            args.mode(),
            conflicts.join("\n  - ")
        )));
    }

    Ok(())
}

// ---------------------------------------------------------------------------

/// Main entry point for the `enrich` command.
pub fn run(
    args: &EnrichArgs,
    llm_backend: crate::cli::LlmBackendChoice,
    embedding_backend: crate::cli::EmbeddingBackendChoice,
) -> Result<(), AppError> {
    // G20: mode-conditional flag validation BEFORE any DB access.
    // Surfaces flags that the wrong mode would silently discard.
    validate_mode_conditional_flags_enrich(args)?;

    // GAP-ENRICH-BACKLOG-CONVERGE: --status is a read-only report. It never
    // calls the LLM, never initialises the OpenRouter client, and never
    // acquires the job singleton, so it is safe to run while a real enrich is
    // in flight (it only reads the queue DB and the unbound backlog).
    // GAP-SG-23/11: --list-dead (inspect dead-letter rows) and --requeue-dead
    // (resurrect them) are queue-only operations — no LLM, no main-DB write, no
    // singleton. Both are scoped to the current --operation so a shared queue is
    // not cross-contaminated. Handled before any provider setup.
    if args.list_dead || args.requeue_dead {
        let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
        let op_label = format!("{:?}", args.operation());
        let queue_conn = open_queue_db(DEFAULT_QUEUE_DB)?;
        if args.list_dead {
            let mut stmt = queue_conn.prepare(
                "SELECT item_key, item_type, attempt, error_class, error FROM queue \
                 WHERE status='dead' AND (operation = ?1 OR operation IS NULL) ORDER BY id",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![op_label], |r| {
                    Ok(DeadItem {
                        dead_item: true,
                        item_key: r.get(0)?,
                        item_type: r.get(1)?,
                        attempt: r.get(2)?,
                        error_class: r.get(3)?,
                        error: r.get(4)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let dead_total = rows.len() as i64;
            for item in &rows {
                emit_json(item);
            }
            emit_json(&DeadSummary {
                summary: true,
                operation: op_label,
                namespace,
                action: "list-dead",
                dead_total,
                requeued: 0,
            });
            return Ok(());
        }
        // --requeue-dead: move dead -> pending, clearing the failure bookkeeping.
        let dead_total: i64 = queue_conn
            .query_row(
                "SELECT COUNT(*) FROM queue WHERE status='dead' \
                 AND (operation = ?1 OR operation IS NULL)",
                rusqlite::params![op_label],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let requeued = queue_conn
            .execute(
                "UPDATE queue SET status='pending', attempt=0, next_retry_at=NULL, \
                 error=NULL, error_class=NULL \
                 WHERE status='dead' AND (operation = ?1 OR operation IS NULL)",
                rusqlite::params![op_label],
            )
            .map_err(|e| AppError::Validation(format!("requeue-dead failed: {e}")))?
            as i64;
        let _ = queue_conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        emit_json(&DeadSummary {
            summary: true,
            operation: op_label,
            namespace,
            action: "requeue-dead",
            dead_total,
            requeued,
        });
        return Ok(());
    }

    if args.status {
        let paths = AppPaths::resolve(args.db.as_deref())?;
        ensure_db_ready(&paths)?;
        let conn = open_rw(&paths.db)?;
        let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
        let unbound_backlog = scan_unbound_memories(&conn, &namespace, None, &[])?.len();
        let queue_conn = open_queue_db(DEFAULT_QUEUE_DB)?;
        let op_label = format!("{:?}", args.operation());
        // GAP-SG-42: scope every count to the current operation. Rows migrated
        // before the `operation` column (NULL) are still counted so a legacy
        // queue is never reported as spuriously empty.
        let count_status = |st: &str, op: &str| -> i64 {
            queue_conn
                .query_row(
                    "SELECT COUNT(*) FROM queue WHERE status=?1 \
                     AND (operation = ?2 OR operation IS NULL)",
                    rusqlite::params![st, op],
                    |r| r.get(0),
                )
                .unwrap_or(0)
        };
        let eligible_now: i64 = queue_conn
            .query_row(
                "SELECT COUNT(*) FROM queue WHERE status='pending' \
                 AND (operation = ?1 OR operation IS NULL) \
                 AND (next_retry_at IS NULL OR next_retry_at <= datetime('now'))",
                rusqlite::params![op_label],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let waiting: i64 = queue_conn
            .query_row(
                "SELECT COUNT(*) FROM queue WHERE status='pending' \
                 AND (operation = ?1 OR operation IS NULL) \
                 AND next_retry_at IS NOT NULL AND next_retry_at > datetime('now')",
                rusqlite::params![op_label],
                |r| r.get(0),
            )
            .unwrap_or(0);
        // GAP-SG-16: enumerate the items currently in backoff with their ETA.
        let waiting_items = {
            let mut stmt = queue_conn.prepare(
                "SELECT item_key, attempt, next_retry_at, error_class FROM queue \
                 WHERE status='pending' AND (operation = ?1 OR operation IS NULL) \
                 AND next_retry_at IS NOT NULL AND next_retry_at > datetime('now') \
                 ORDER BY next_retry_at",
            )?;
            let items: Vec<WaitingItem> = stmt
                .query_map(rusqlite::params![op_label], |r| {
                    Ok(WaitingItem {
                        item_key: r.get(0)?,
                        attempt: r.get(1)?,
                        next_retry_at: r.get(2)?,
                        error_class: r.get(3)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            items
        };
        let queue_pending = count_status("pending", &op_label);
        let queue_processing = count_status("processing", &op_label);
        let queue_done = count_status("done", &op_label);
        let queue_failed = count_status("failed", &op_label);
        let queue_skipped = count_status("skipped", &op_label);
        let queue_dead = count_status("dead", &op_label);
        // GAP-SG-15/46: distinguish empty from cooldown from not-yet-scanned.
        let state = if eligible_now > 0 {
            "draining"
        } else if waiting > 0 {
            "cooldown"
        } else if queue_pending == 0 && unbound_backlog > 0 {
            "pending-scan"
        } else {
            "empty"
        };
        emit_json(&EnrichStatus {
            status_report: true,
            operation: op_label,
            namespace,
            unbound_backlog,
            queue_pending,
            queue_processing,
            queue_done,
            queue_failed,
            queue_skipped,
            queue_dead,
            eligible_now,
            waiting,
            state,
            waiting_items,
        });
        return Ok(());
    }

    // v1.0.95 (ADR-0054): when the JUDGE is OpenRouter the model is mandatory
    // (no default) and the API key must resolve BEFORE any network or DB work.
    // The chat client singleton is initialised here so every per-item dispatch
    // fetches it without re-threading the key.
    if args.mode() == EnrichMode::OpenRouter {
        let model = args.openrouter_model.as_deref().ok_or_else(|| {
            AppError::Validation(
                "--mode openrouter requires --openrouter-model (no default model is allowed)"
                    .into(),
            )
        })?;
        let resolved =
            crate::config::resolve_api_key("openrouter", args.openrouter_api_key.as_deref())
                .ok_or_else(|| {
                    AppError::Validation(
                        "OPENROUTER_API_KEY not found; set the env var, store it via \
                         `config add-key --provider openrouter`, or pass --openrouter-api-key"
                            .into(),
                    )
                })?;
        crate::embedder::get_openrouter_chat_client(
            resolved.value,
            model,
            args.openrouter_timeout,
        )?;
    }

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
    let provider_binary = if matches!(args.operation(), EnrichOperation::ReEmbed) {
        None
    } else {
        Some(match args.mode() {
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
            EnrichMode::Opencode => {
                let bin = super::opencode_runner::find_opencode_binary_with_override(
                    args.opencode_binary.as_deref(),
                )?;
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
            EnrichMode::OpenRouter => {
                // v1.0.95: the OpenRouter JUDGE is a REST call, not a spawned
                // binary. The chat client singleton was initialised at the top
                // of run(); this placeholder path threads through the dispatch
                // but is never dereferenced by the OpenRouter arm.
                emit_json(&PhaseEvent {
                    phase: "validate",
                    binary_path: None,
                    version: None,
                    items_total: None,
                    items_pending: None,
                    llm_parallelism: None,
                });
                PathBuf::new()
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
    if args.preflight_check
        && !args.dry_run
        && !matches!(args.operation(), EnrichOperation::ReEmbed)
    {
        let preflight_result = run_preflight_probe(args);
        match preflight_result {
            PreflightOutcome::Healthy => {
                tracing::info!(target: "enrich", mode = ?args.mode(), "preflight probe healthy");
            }
            PreflightOutcome::RateLimited { reason, suggestion } => {
                if let Some(fallback) = args.fallback_mode.clone() {
                    if fallback != args.mode() {
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
                            mode = args.mode()
                        )));
                    }
                    return Err(AppError::Validation(format!(
                        "preflight detected rate limit on {mode:?}: {reason}; \
                         --fallback-mode matches --mode, no recovery possible",
                        mode = args.mode()
                    )));
                }
                return Err(AppError::Validation(format!(
                    "preflight detected rate limit on {mode:?}: {reason}; \
                     {suggestion}; pass --fallback-mode codex to recover",
                    mode = args.mode()
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
            operation: format!("{:?}", args.operation()),
            items_total: total,
            completed: 0,
            failed: 0,
            skipped: 0,
            cost_usd: 0.0,
            elapsed_ms: started.elapsed().as_millis() as u64,
            backend_invoked: take_enrich_backend(),
            waiting: 0,
            dead: 0,
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

    if !args.resume && !args.retry_failed && !args.until_empty {
        queue_conn
            .execute("DELETE FROM queue", [])
            .map_err(|e| AppError::Validation(format!("queue clear failed: {e}")))?;
    }

    // Populate queue (GAP-SG-12: tag rows with the operation + link memory_id).
    let op_label = format!("{:?}", args.operation());
    let item_type = item_type_for(&args.operation());
    for key in scan_result.iter() {
        enqueue_candidate(&queue_conn, &conn, &namespace, key, item_type, &op_label);
    }

    // G19: parallel LLM processing via std::thread::scope when parallelism > 1.
    // Clamp enforces the range even if the caller bypasses clap validation.
    let parallelism = if args.mode() == EnrichMode::OpenRouter {
        let rest = args.rest_concurrency.unwrap_or(8).clamp(1, 16) as usize;
        tracing::info!(
            target: "enrich",
            concurrency = rest,
            source = "rest_concurrency",
            "OpenRouter REST concurrency (clamp 1..=16)"
        );
        rest
    } else {
        let p = args.llm_parallelism.clamp(1, 32) as usize;
        tracing::info!(
            target: "enrich",
            concurrency = p,
            source = "llm_parallelism",
            "LLM subprocess parallelism (clamp 1..=32)"
        );
        p
    };
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
        match args.mode() {
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
            EnrichMode::Opencode if parallelism > 16 => {
                tracing::warn!(
                    target: "enrich",
                    llm_parallelism = parallelism,
                    recommended_max = 16,
                    mode = "opencode",
                    "llm_parallelism above 16 risks OAuth rate-limit on OpenCode; \
                     consider --llm-parallelism 8 for safer concurrency"
                );
            }
            EnrichMode::Opencode => {
                // No warning: opencode does not spawn MCP children.
            }
            EnrichMode::OpenRouter => {
                // No warning: OpenRouter is a bounded HTTP fan-out (no
                // subprocess); --llm-parallelism is respected as-is.
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

    let provider_timeout = match args.mode() {
        EnrichMode::ClaudeCode => args.claude_timeout,
        EnrichMode::Codex => args.codex_timeout,
        EnrichMode::Opencode => args.opencode_timeout,
        EnrichMode::OpenRouter => args.openrouter_timeout,
    };

    let provider_model: Option<&str> = match args.mode() {
        EnrichMode::ClaudeCode => args.claude_model.as_deref(),
        EnrichMode::Codex => args.codex_model.as_deref(),
        EnrichMode::Opencode => args.opencode_model.as_deref(),
        EnrichMode::OpenRouter => args.openrouter_model.as_deref(),
    };

    // GAP-SG-16: when --ignore-backoff is set, drop the per-item cooldown filter
    // from candidate selection so items parked on `next_retry_at` are eligible
    // immediately. Shared by the parallel workers and the serial loop.
    let backoff_clause: &str = if args.ignore_backoff {
        ""
    } else {
        "AND (next_retry_at IS NULL OR next_retry_at <= datetime('now'))"
    };

    // GAP-SG-45: announce the scan-vs-drain concurrency split (scan is always
    // serial; drain uses `parallelism` workers).
    emit_json(&ConcurrencyEvent {
        phase: "concurrency",
        scan_parallelism: 1,
        drain_parallelism: parallelism as u32,
    });

    // GAP-ENRICH-BACKLOG-CONVERGE: --until-empty wraps the scan→populate→drain
    // cycle in an internal loop so the external bash retry loop is unnecessary.
    // Without --until-empty the loop body runs exactly once (legacy behaviour).
    let until_deadline = std::time::Instant::now()
        + std::time::Duration::from_secs(args.max_runtime.unwrap_or(3600));
    loop {
        if args.until_empty {
            // Re-scan and re-enqueue eligible candidates each iteration.
            // INSERT OR IGNORE never resurrects a dead-letter row (item_key is
            // UNIQUE), so the backlog converges instead of looping forever.
            let rescan = scan_operation(&conn, &namespace, args)?;
            for key in &rescan {
                enqueue_candidate(&queue_conn, &conn, &namespace, key, item_type, &op_label);
            }
        }
        let completed_before = completed;

        // G19: when parallelism > 1, spawn bounded worker threads.
        // Each worker opens its own DB connections (WAL supports concurrent readers + serialized writers).
        // The queue DB claim is atomic via UPDATE...RETURNING — no external lock needed.
        if parallelism > 1 {
            let stdout_mu = parking_lot::Mutex::new(());
            let budget = args.max_cost_usd;
            let operation = args.operation().clone();
            let mode = args.mode().clone();
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
                            // GAP-SG-16: --ignore-backoff drops the next_retry_at
                            // cooldown filter so items waiting on backoff are
                            // claimed immediately.
                            let dequeue_sql = format!(
                                "UPDATE queue SET status='processing', attempt=attempt+1 \
                                 WHERE id = (SELECT id FROM queue WHERE status='pending' {backoff_clause} \
                                             ORDER BY id LIMIT 1) \
                                 RETURNING id, item_key, item_type, attempt"
                            );
                            let pending: Option<(i64, String, String, i64)> = w_queue
                                .query_row(
                                    &dequeue_sql,
                                    [],
                                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                                )
                                .ok();
                            let (queue_id, item_key, _item_type, attempt_current) = match pending {
                                Some(p) => p,
                                None => break,
                            };
                            let item_started = Instant::now();
                            let current_index = w_completed + w_failed + w_skipped;

                            let call_result = match operation {
                                EnrichOperation::MemoryBindings | EnrichOperation::AugmentBindings => call_memory_bindings(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode),
                                EnrichOperation::EntityDescriptions => call_entity_description(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode),
                                EnrichOperation::BodyEnrich => call_body_enrich(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode, min_oc, max_oc, prompt_tpl, args.preserve_threshold, paths, llm_backend, embedding_backend),
                                EnrichOperation::ReEmbed => call_reembed(&w_conn, namespace, &item_key, paths, llm_backend, embedding_backend),
                                EnrichOperation::WeightCalibrate => call_weight_calibrate(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode),
                                EnrichOperation::RelationReclassify => call_relation_reclassify(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode),
                                EnrichOperation::EntityConnect | EnrichOperation::CrossDomainBridges => call_entity_connect(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode),
                                EnrichOperation::EntityTypeValidate => call_entity_type_validate(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode),
                                EnrichOperation::DescriptionEnrich => call_description_enrich(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode),
                                EnrichOperation::DomainClassify => call_domain_classify(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode),
                                EnrichOperation::GraphAudit => call_graph_audit(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode),
                                EnrichOperation::DeepResearchSynth => call_deep_research_synth(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode),
                                EnrichOperation::BodyExtract => call_body_extract(&w_conn, namespace, &item_key, provider_binary.expect("provider binary required"), provider_model, provider_timeout, mode, args.body_extract_graph_only),
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
                                    let outcome = record_item_failure(&w_queue, queue_id, attempt_current, args.max_attempts, &e);
                                    let _guard = stdout_mu.lock();
                                    emit_json(&ItemEvent { item: &item_key, status: "failed", memory_id: None, entity_id: None, entities: None, rels: None, chars_before: None, chars_after: None, cost_usd: None, elapsed_ms: Some(item_started.elapsed().as_millis() as u64), error: Some(err_str), index: current_index, total });
                                    // G28-D: feed the classified outcome to the breaker (transient
                                    // failures do not count toward opening it).
                                    let breaker_opened = w_breaker.record(outcome);
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

                // Dequeue next pending item (GAP-SG-16: --ignore-backoff drops
                // the next_retry_at cooldown filter).
                let dequeue_sql = format!(
                    "UPDATE queue SET status='processing', attempt=attempt+1 \
                     WHERE id = (SELECT id FROM queue WHERE status='pending' {backoff_clause} \
                                 ORDER BY id LIMIT 1) \
                     RETURNING id, item_key, item_type, attempt"
                );
                let pending: Option<(i64, String, String, i64)> = queue_conn
                    .query_row(&dequeue_sql, [], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                    })
                    .ok();

                let (queue_id, item_key, item_type, attempt_current) = match pending {
                    Some(p) => p,
                    None => break,
                };

                let item_started = Instant::now();
                let current_index = completed + failed + skipped;

                let call_result = match args.operation() {
                    EnrichOperation::MemoryBindings | EnrichOperation::AugmentBindings => {
                        call_memory_bindings(
                            &conn,
                            &namespace,
                            &item_key,
                            provider_binary
                                .as_deref()
                                .expect("provider binary required"),
                            provider_model,
                            provider_timeout,
                            &args.mode(),
                        )
                    }
                    EnrichOperation::EntityDescriptions => call_entity_description(
                        &conn,
                        &namespace,
                        &item_key,
                        provider_binary
                            .as_deref()
                            .expect("provider binary required"),
                        provider_model,
                        provider_timeout,
                        &args.mode(),
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
                        &args.mode(),
                        args.min_output_chars,
                        args.max_output_chars,
                        args.prompt_template.as_deref(),
                        args.preserve_threshold,
                        &paths,
                        llm_backend,
                        embedding_backend,
                    ),
                    EnrichOperation::ReEmbed => call_reembed(
                        &conn,
                        &namespace,
                        &item_key,
                        &paths,
                        llm_backend,
                        embedding_backend,
                    ),
                    EnrichOperation::WeightCalibrate => call_weight_calibrate(
                        &conn,
                        &namespace,
                        &item_key,
                        provider_binary
                            .as_deref()
                            .expect("provider binary required"),
                        provider_model,
                        provider_timeout,
                        &args.mode(),
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
                        &args.mode(),
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
                            &args.mode(),
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
                        &args.mode(),
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
                        &args.mode(),
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
                        &args.mode(),
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
                        &args.mode(),
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
                        &args.mode(),
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
                        &args.mode(),
                        args.body_extract_graph_only,
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
                        let persist_err: Option<String> = match args.operation() {
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
                        let _outcome = record_item_failure(
                            &queue_conn,
                            queue_id,
                            attempt_current,
                            args.max_attempts,
                            &e,
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
        } // end else (serial path)

        if !args.until_empty {
            break;
        }
        let eligible_remaining: i64 = queue_conn
            .query_row(
                &format!("SELECT COUNT(*) FROM queue WHERE status='pending' {backoff_clause}"),
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let progressed = completed > completed_before;
        if std::time::Instant::now() >= until_deadline {
            tracing::info!(target: "enrich", "until-empty: max-runtime reached, stopping");
            break;
        }
        if !progressed && eligible_remaining == 0 {
            tracing::info!(target: "enrich", "until-empty: converged (no eligible items remain)");
            break;
        }
        if eligible_remaining == 0 {
            // Remaining pending items are waiting on backoff; nap and re-check.
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    } // end until-empty loop

    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    let _ = queue_conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");

    // GAP-SG-15: report items still in cooldown (waiting) and dead-lettered
    // alongside completed, so `--until-empty` makes the convergence state
    // explicit (cooldown vs. dead vs. truly empty) instead of just "done".
    let waiting_final: i64 = queue_conn
        .query_row(
            "SELECT COUNT(*) FROM queue WHERE status='pending' \
             AND (operation = ?1 OR operation IS NULL) \
             AND next_retry_at IS NOT NULL AND next_retry_at > datetime('now')",
            rusqlite::params![op_label],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let dead_final: i64 = queue_conn
        .query_row(
            "SELECT COUNT(*) FROM queue WHERE status='dead' \
             AND (operation = ?1 OR operation IS NULL)",
            rusqlite::params![op_label],
            |r| r.get(0),
        )
        .unwrap_or(0);

    emit_json(&EnrichSummary {
        summary: true,
        operation: format!("{:?}", args.operation()),
        items_total: total,
        completed,
        failed,
        skipped,
        cost_usd: cost_total,
        elapsed_ms: started.elapsed().as_millis() as u64,
        backend_invoked: take_enrich_backend(),
        waiting: waiting_final,
        dead: dead_final,
    });

    if failed == 0 {
        // GAP-ENRICH-BACKLOG-CONVERGE: keep the queue file when dead-letter rows
        // exist so `enrich --status` can still report them on the next run.
        let dead: i64 = queue_conn
            .query_row("SELECT COUNT(*) FROM queue WHERE status='dead'", [], |r| {
                r.get(0)
            })
            .unwrap_or(0);
        if dead == 0 {
            let _ = std::fs::remove_file(DEFAULT_QUEUE_DB);
        }
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
        EnrichMode::Opencode => call_opencode(
            binary,
            BINDINGS_PROMPT,
            BINDINGS_SCHEMA,
            &body,
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => {
            call_openrouter(BINDINGS_PROMPT, BINDINGS_SCHEMA, &body, model, timeout)?
        }
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
        EnrichMode::Opencode => call_opencode(
            binary,
            &prompt,
            ENTITY_DESCRIPTION_SCHEMA,
            "",
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => {
            call_openrouter(&prompt, ENTITY_DESCRIPTION_SCHEMA, "", model, timeout)?
        }
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
    embedding_backend: crate::cli::EmbeddingBackendChoice,
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
        EnrichMode::Opencode => {
            call_opencode(binary, &prompt, BODY_ENRICH_SCHEMA, &body, model, timeout)?
        }
        EnrichMode::OpenRouter => {
            call_openrouter(&prompt, BODY_ENRICH_SCHEMA, &body, model, timeout)?
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
        embedding_backend,
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
    embedding_backend: crate::cli::EmbeddingBackendChoice,
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
        embedding_backend,
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
    match args.operation() {
        EnrichOperation::MemoryBindings => {
            let rows = scan_unbound_memories(conn, namespace, args.limit, &name_filter)?;
            Ok(rows.into_iter().map(|(_, name, _)| name).collect())
        }
        // GAP-SG-24/26: additive augmentation processes ALREADY-bound memories,
        // restricted to an explicit name filter so it never re-scans the whole
        // namespace.
        EnrichOperation::AugmentBindings => {
            scan_bound_memories_for_augment(conn, namespace, args.limit, &name_filter)
        }
        EnrichOperation::EntityDescriptions => {
            let rows =
                scan_entities_without_description(conn, namespace, args.limit, &name_filter)?;
            Ok(rows.into_iter().map(|(_, name, _)| name).collect())
        }
        EnrichOperation::BodyEnrich => {
            let rows = scan_short_body_memories(
                conn,
                namespace,
                args.min_output_chars,
                args.limit,
                &name_filter,
            )?;
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
            let mut names = stmt
                .query_map(rusqlite::params![namespace], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            // GAP-SG-27: honour --names/--names-file for body-extract (and the
            // sibling whole-namespace scans), which previously ignored it and
            // scanned every memory by id.
            if !name_filter.is_empty() {
                names.retain(|n| name_filter.iter().any(|f| f == n));
            }
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
        EnrichMode::Opencode => call_opencode(
            binary,
            WEIGHT_CALIBRATE_PROMPT,
            WEIGHT_CALIBRATE_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => call_openrouter(
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
        EnrichMode::Opencode => call_opencode(
            binary,
            RELATION_RECLASSIFY_PROMPT,
            RELATION_RECLASSIFY_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => call_openrouter(
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
        EnrichMode::Opencode => call_opencode(
            binary,
            ENTITY_CONNECT_PROMPT,
            ENTITY_CONNECT_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => call_openrouter(
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
        EnrichMode::Opencode => call_opencode(
            binary,
            ENTITY_TYPE_VALIDATE_PROMPT,
            ENTITY_TYPE_VALIDATE_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => call_openrouter(
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
        EnrichMode::Opencode => call_opencode(
            binary,
            DESCRIPTION_ENRICH_PROMPT,
            DESCRIPTION_ENRICH_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => call_openrouter(
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
    let old_name: String = conn.query_row(
        "SELECT name FROM memories WHERE id = ?1",
        rusqlite::params![mem_id],
        |r| r.get(0),
    )?;
    conn.execute(
        "UPDATE memories SET description = ?1 WHERE id = ?2",
        rusqlite::params![new_desc, mem_id],
    )?;
    memories::sync_fts_after_update(
        conn, mem_id, &old_name, &old_desc, &body, &old_name, new_desc, &body,
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
        EnrichMode::Opencode => call_opencode(
            binary,
            DOMAIN_CLASSIFY_PROMPT,
            DOMAIN_CLASSIFY_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => call_openrouter(
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
        EnrichMode::Opencode => call_opencode(
            binary,
            GRAPH_AUDIT_PROMPT,
            GRAPH_AUDIT_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => call_openrouter(
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
        EnrichMode::Opencode => call_opencode(
            binary,
            DEEP_RESEARCH_SYNTH_PROMPT,
            DEEP_RESEARCH_SYNTH_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => call_openrouter(
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
///
/// GAP-SG-28: when `graph_only` is set, the memory body is left UNTOUCHED and
/// the extraction instead pulls entities/relationships into the graph (additive,
/// via the same upsert path as `memory-bindings`). This is the read-only mode —
/// it never rewrites or truncates the stored body.
#[allow(clippy::too_many_arguments)]
fn call_body_extract(
    conn: &Connection,
    namespace: &str,
    item_key: &str,
    binary: &Path,
    model: Option<&str>,
    timeout: u64,
    mode: &EnrichMode,
    graph_only: bool,
) -> Result<EnrichItemResult, AppError> {
    // GAP-SG-28: read-only graph extraction. Reuse the bindings prompt/schema
    // and the additive persist path; the body is never modified.
    if graph_only {
        let (memory_id, body): (i64, String) = conn
            .query_row(
                "SELECT id, COALESCE(body,'') FROM memories WHERE namespace=?1 AND name=?2 AND deleted_at IS NULL",
                rusqlite::params![namespace, item_key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    AppError::NotFound(format!("memory '{item_key}' not found"))
                }
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
            EnrichMode::Opencode => call_opencode(
                binary,
                BINDINGS_PROMPT,
                BINDINGS_SCHEMA,
                &body,
                model,
                timeout,
            )?,
            EnrichMode::OpenRouter => {
                call_openrouter(BINDINGS_PROMPT, BINDINGS_SCHEMA, &body, model, timeout)?
            }
        };
        let empty_arr = serde_json::Value::Array(vec![]);
        let entities_val = value.get("entities").unwrap_or(&empty_arr);
        let rels_val = value.get("relationships").unwrap_or(&empty_arr);
        let (ent_count, rel_count) =
            persist_memory_bindings(conn, namespace, memory_id, entities_val, rels_val)?;
        return Ok(EnrichItemResult::Done {
            memory_id: Some(memory_id),
            entity_id: None,
            entities: ent_count,
            rels: rel_count,
            chars_before: None,
            chars_after: None,
            cost,
            is_oauth,
        });
    }

    let (mem_id, body, old_desc): (i64, String, String) = conn
        .query_row(
            "SELECT id, body, description FROM memories WHERE name = ?1 AND deleted_at IS NULL",
            rusqlite::params![item_key],
            |r| Ok((r.get(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
        )
        .map_err(|_| AppError::NotFound(format!("memory '{item_key}' not found")))?;
    let old_name: String = conn.query_row(
        "SELECT name FROM memories WHERE id = ?1",
        rusqlite::params![mem_id],
        |r| r.get(0),
    )?;
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
        EnrichMode::Opencode => call_opencode(
            binary,
            BODY_EXTRACT_PROMPT,
            BODY_EXTRACT_SCHEMA,
            &input_text,
            model,
            timeout,
        )?,
        EnrichMode::OpenRouter => call_openrouter(
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
    memories::sync_fts_after_update(
        conn,
        mem_id,
        &old_name,
        &old_desc,
        &body,
        &old_name,
        &old_desc,
        restructured,
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
    let mut cmd = super::codex_spawn::build_codex_command(&args)?;

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

fn call_opencode(
    binary: &Path,
    prompt: &str,
    json_schema: &str,
    input_text: &str,
    model: Option<&str>,
    timeout_secs: u64,
) -> Result<(serde_json::Value, f64, bool), AppError> {
    use wait_timeout::ChildExt;

    let resolved_model = super::opencode_runner::resolve_opencode_model(model);

    let augmented_prompt = if json_schema.is_empty() {
        prompt.to_string()
    } else {
        format!(
            "{prompt}\n\nIMPORTANT: You MUST respond with ONLY valid JSON (no markdown, no explanation, no code fences). \
             The JSON MUST match this schema:\n{json_schema}"
        )
    };

    let mut cmd = super::opencode_runner::build_opencode_command_sync(
        binary,
        &resolved_model,
        &augmented_prompt,
        input_text,
    )?;

    let mut child = super::opencode_runner::spawn_opencode(&mut cmd).map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to spawn opencode: {e}"),
        ))
    })?;

    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let status = child.wait_timeout(timeout).map_err(AppError::Io)?;

    match status {
        Some(exit_status) => {
            tracing::debug!(
                target: "process",
                exit_code = ?exit_status.code(),
                elapsed_ms = start.elapsed().as_millis() as u64,
                "opencode process completed"
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
                    "opencode process failed"
                );
                return Err(AppError::Validation(format!(
                    "opencode exited with code {:?}: {}",
                    exit_status.code(),
                    stderr_str.trim()
                )));
            }
            let stdout_str = String::from_utf8(stdout_buf)
                .map_err(|_| AppError::Validation("opencode stdout is not valid UTF-8".into()))?;
            let (text, cost, _tokens) = super::opencode_runner::parse_opencode_output(&stdout_str)?;
            let value: serde_json::Value =
                super::opencode_runner::parse_json_from_opencode_text(&text).map_err(|e| {
                    AppError::Validation(format!("opencode response is not valid JSON: {e}"))
                })?;
            Ok((value, cost, false))
        }
        None => {
            let _ = child.kill();
            let _ = child.wait();
            Err(AppError::Validation(format!(
                "opencode timed out after {timeout_secs} seconds"
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

        let results = scan_entities_without_description(&conn, "global", None, &[]).unwrap();
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

        let results = scan_entities_without_description(&conn, "global", None, &[]).unwrap();
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

        let results = scan_short_body_memories(&conn, "global", 100, None, &[]).unwrap();
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

        let results = scan_short_body_memories(&conn, "global", 100, None, &[]).unwrap();
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

        let results = scan_short_body_memories(&conn, "global", 1000, Some(3), &[]).unwrap();
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

        let results = scan_short_body_memories(&conn, "global", 1000, None, &[]).unwrap();
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

    // -- GAP-ENRICH-BACKLOG-CONVERGE: dead-letter + backoff tests ------------

    fn open_temp_queue() -> (Connection, String) {
        let path = format!(
            "/tmp/test-enrich-dl-{}-{}.sqlite",
            std::process::id(),
            fastrand::u64(..)
        );
        let conn = open_queue_db(&path).expect("queue db must open");
        (conn, path)
    }

    fn insert_pending(conn: &Connection, key: &str) -> i64 {
        conn.execute(
            "INSERT INTO queue (item_key, item_type, status) VALUES (?1, 'memory', 'pending')",
            rusqlite::params![key],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn classify_rate_limit_is_transient() {
        let e = AppError::RateLimited {
            detail: "429".into(),
        };
        assert_eq!(
            classify_enrich_outcome(&e),
            crate::retry::AttemptOutcome::Transient
        );
    }

    #[test]
    fn classify_timeout_and_dbbusy_are_transient() {
        let t = AppError::Timeout {
            operation: "judge".into(),
            duration_secs: 30,
        };
        let b = AppError::DbBusy("locked".into());
        assert_eq!(
            classify_enrich_outcome(&t),
            crate::retry::AttemptOutcome::Transient
        );
        assert_eq!(
            classify_enrich_outcome(&b),
            crate::retry::AttemptOutcome::Transient
        );
    }

    #[test]
    fn classify_validation_and_parse_are_hard_failure() {
        let v = AppError::Validation("failed to parse entities array: bad".into());
        assert_eq!(
            classify_enrich_outcome(&v),
            crate::retry::AttemptOutcome::HardFailure
        );
    }

    #[test]
    fn open_queue_db_alter_is_idempotent() {
        let path = format!(
            "/tmp/test-enrich-idem-{}-{}.sqlite",
            std::process::id(),
            fastrand::u64(..)
        );
        // First open creates the table + dead-letter columns.
        let _ = open_queue_db(&path).expect("first open");
        // Second open must not error on the already-present columns.
        let conn = open_queue_db(&path).expect("second open is idempotent");
        let cols: Vec<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info(queue)").unwrap();
            stmt.query_map([], |r| r.get::<_, String>(1))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert!(cols.iter().any(|c| c == "error_class"));
        assert!(cols.iter().any(|c| c == "next_retry_at"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn record_item_failure_hard_marks_dead() {
        let (conn, path) = open_temp_queue();
        let id = insert_pending(&conn, "mem-hard");
        let outcome = record_item_failure(
            &conn,
            id,
            1,
            5,
            &AppError::Validation("invalid body".into()),
        );
        assert_eq!(outcome, crate::retry::AttemptOutcome::HardFailure);
        let status: String = conn
            .query_row(
                "SELECT status FROM queue WHERE id=?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "dead");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn record_item_failure_transient_reschedules_pending() {
        let (conn, path) = open_temp_queue();
        let id = insert_pending(&conn, "mem-transient");
        let outcome = record_item_failure(
            &conn,
            id,
            1,
            5,
            &AppError::RateLimited {
                detail: "429".into(),
            },
        );
        assert_eq!(outcome, crate::retry::AttemptOutcome::Transient);
        let (status, future): (String, i64) = conn
            .query_row(
                "SELECT status, (next_retry_at > datetime('now')) FROM queue WHERE id=?1",
                rusqlite::params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "pending");
        assert_eq!(future, 1, "next_retry_at must be in the future");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn record_item_failure_transient_at_cap_marks_dead() {
        let (conn, path) = open_temp_queue();
        let id = insert_pending(&conn, "mem-cap");
        // attempt == max_attempts forces dead-letter even for a transient error.
        let outcome = record_item_failure(
            &conn,
            id,
            5,
            5,
            &AppError::RateLimited {
                detail: "429".into(),
            },
        );
        assert_eq!(outcome, crate::retry::AttemptOutcome::Transient);
        let status: String = conn
            .query_row(
                "SELECT status FROM queue WHERE id=?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "dead");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn dequeue_skips_future_retry_and_dead() {
        let (conn, path) = open_temp_queue();
        // Eligible now.
        let eligible = insert_pending(&conn, "mem-eligible");
        // Pending but scheduled in the future.
        let waiting = insert_pending(&conn, "mem-waiting");
        conn.execute(
            "UPDATE queue SET next_retry_at=datetime('now', '+3600 seconds') WHERE id=?1",
            rusqlite::params![waiting],
        )
        .unwrap();
        // Dead-letter must never be selected.
        let dead = insert_pending(&conn, "mem-dead");
        conn.execute(
            "UPDATE queue SET status='dead' WHERE id=?1",
            rusqlite::params![dead],
        )
        .unwrap();

        let claimed: Option<i64> = conn
            .query_row(
                "UPDATE queue SET status='processing', attempt=attempt+1 \
                 WHERE id = (SELECT id FROM queue WHERE status='pending' \
                               AND (next_retry_at IS NULL OR next_retry_at <= datetime('now')) \
                             ORDER BY id LIMIT 1) \
                 RETURNING id",
                [],
                |r| r.get(0),
            )
            .ok();
        assert_eq!(claimed, Some(eligible));

        // A second claim finds nothing eligible (waiting is future, dead excluded).
        let second: Option<i64> = conn
            .query_row(
                "UPDATE queue SET status='processing', attempt=attempt+1 \
                 WHERE id = (SELECT id FROM queue WHERE status='pending' \
                               AND (next_retry_at IS NULL OR next_retry_at <= datetime('now')) \
                             ORDER BY id LIMIT 1) \
                 RETURNING id",
                [],
                |r| r.get(0),
            )
            .ok();
        assert_eq!(second, None);
        let _ = std::fs::remove_file(&path);
    }

    // GAP-SG-09: malformed / non-JSON / shape-invalid LLM output is a transient
    // model hiccup (retryable with backoff), NOT a permanent dead-letter.
    #[test]
    fn classify_non_json_and_shape_errors_are_transient() {
        for msg in [
            "model 'x' returned non-object JSON after repair (got string)",
            "model 'x' returned content that could not be parsed even after JSON repair",
            "model 'x' returned no structured content",
            "LLM result missing 'description' field",
            "LLM result missing 'enriched_body' field",
        ] {
            assert_eq!(
                classify_enrich_outcome(&AppError::Validation(msg.into())),
                crate::retry::AttemptOutcome::Transient,
                "expected transient for: {msg}"
            );
        }
    }

    // GAP-SG-09: genuinely permanent faults still dead-letter on the first try.
    #[test]
    fn classify_provider_error_and_not_found_are_hard() {
        assert_eq!(
            classify_enrich_outcome(&AppError::ProviderError {
                code: "400".into(),
                message: "context length exceeded".into(),
            }),
            crate::retry::AttemptOutcome::HardFailure
        );
        assert_eq!(
            classify_enrich_outcome(&AppError::NotFound("memory 'gone' not found".into())),
            crate::retry::AttemptOutcome::HardFailure
        );
    }

    // GAP-SG-12/42: the queue gains an `operation` column, migrated idempotently.
    #[test]
    fn open_queue_db_migrates_operation_column() {
        let (conn, path) = open_temp_queue();
        // Re-open to prove the ALTER is idempotent on an existing file.
        drop(conn);
        let conn = open_queue_db(&path).expect("second open is idempotent");
        let cols: Vec<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info(queue)").unwrap();
            stmt.query_map([], |r| r.get::<_, String>(1))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert!(cols.iter().any(|c| c == "operation"));
        assert!(cols.iter().any(|c| c == "memory_id"));
        let _ = std::fs::remove_file(&path);
    }

    // GAP-SG-12: enqueue_candidate tags the row with its operation and links the
    // resolved memory_id so the cascade cleanup can target it.
    #[test]
    fn enqueue_candidate_tags_operation_and_memory_id() {
        let main = open_test_db();
        main.execute(
            "INSERT INTO memories (namespace, name, body) VALUES ('global', 'mem-x', 'body')",
            [],
        )
        .unwrap();
        let mem_id: i64 = main
            .query_row("SELECT id FROM memories WHERE name='mem-x'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let (queue, path) = open_temp_queue();
        enqueue_candidate(&queue, &main, "global", "mem-x", "memory", "MemoryBindings");
        let (op, mid): (String, i64) = queue
            .query_row(
                "SELECT operation, memory_id FROM queue WHERE item_key='mem-x'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(op, "MemoryBindings");
        assert_eq!(mid, mem_id);
        let _ = std::fs::remove_file(&path);
    }

    // GAP-SG-11: --requeue-dead moves dead rows back to pending and zeroes the
    // failure bookkeeping; --retry-failed (status='failed') leaves dead rows.
    #[test]
    fn requeue_dead_resurrects_dead_rows() {
        let (conn, path) = open_temp_queue();
        conn.execute(
            "INSERT INTO queue (item_key, item_type, status, operation, attempt, error, error_class, next_retry_at) \
             VALUES ('mem-dead', 'memory', 'dead', 'MemoryBindings', 8, 'boom', 'permanent', datetime('now'))",
            [],
        )
        .unwrap();
        // The --requeue-dead UPDATE (scoped to the operation).
        let n = conn
            .execute(
                "UPDATE queue SET status='pending', attempt=0, next_retry_at=NULL, \
                 error=NULL, error_class=NULL \
                 WHERE status='dead' AND (operation = ?1 OR operation IS NULL)",
                rusqlite::params!["MemoryBindings"],
            )
            .unwrap();
        assert_eq!(n, 1);
        let (status, attempt, nra): (String, i64, Option<String>) = conn
            .query_row(
                "SELECT status, attempt, next_retry_at FROM queue WHERE item_key='mem-dead'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(status, "pending");
        assert_eq!(attempt, 0);
        assert!(nra.is_none());
        let _ = std::fs::remove_file(&path);
    }

    // GAP-SG-13: the cascade-cleanup DELETE removes the queue row by memory_id
    // AND by item_key (name), covering both done (id-linked) and pending rows.
    #[test]
    fn cascade_cleanup_delete_targets_memory_id_and_name() {
        let (conn, path) = open_temp_queue();
        conn.execute(
            "INSERT INTO queue (item_key, item_type, status, memory_id) VALUES ('by-id', 'memory', 'done', 42)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO queue (item_key, item_type, status) VALUES ('by-name', 'memory', 'pending')",
            [],
        )
        .unwrap();
        // Same DELETE that cleanup_queue_entry issues.
        let removed = conn
            .execute(
                "DELETE FROM queue WHERE memory_id = ?1 OR item_key = ?2",
                rusqlite::params![42_i64, "by-name"],
            )
            .unwrap();
        assert_eq!(removed, 2);
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM queue", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 0);
        let _ = std::fs::remove_file(&path);
    }

    // GAP-SG-24/26: augment scan requires an explicit name filter and selects
    // ONLY already-bound memories.
    #[test]
    fn scan_bound_memories_for_augment_requires_names_and_finds_bound() {
        let conn = open_test_db();
        conn.execute(
            "INSERT INTO memories (id, namespace, name, body) VALUES (1, 'global', 'bound', 'b')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memories (id, namespace, name, body) VALUES (2, 'global', 'unbound', 'b')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO entities (id, namespace, name) VALUES (10, 'global', 'e')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memory_entities (memory_id, entity_id) VALUES (1, 10)",
            [],
        )
        .unwrap();

        // Empty filter is rejected.
        assert!(scan_bound_memories_for_augment(&conn, "global", None, &[]).is_err());

        // With a filter, only the bound memory in the filter is returned.
        let names = scan_bound_memories_for_augment(
            &conn,
            "global",
            None,
            &["bound".to_string(), "unbound".to_string()],
        )
        .unwrap();
        assert_eq!(names, vec!["bound".to_string()]);
    }

    #[test]
    fn item_type_for_maps_entity_and_memory() {
        assert_eq!(
            item_type_for(&EnrichOperation::EntityDescriptions),
            "entity"
        );
        assert_eq!(item_type_for(&EnrichOperation::MemoryBindings), "memory");
        assert_eq!(item_type_for(&EnrichOperation::AugmentBindings), "memory");
        assert_eq!(item_type_for(&EnrichOperation::BodyExtract), "memory");
    }
}
