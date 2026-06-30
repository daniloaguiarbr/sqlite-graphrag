// v1.0.97: modularised into queue.rs, scan.rs, postprocess.rs, extraction.rs.
// See ADR-0056 (closes the ADR-0046 "Known Tech Debt (v1.0.89+)" item).

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
//! SQLite queue DB derived next to `--db` (GAP-SG-64) for resume/retry support.
// Workload: Subprocess I/O-bound (claude/codex API calls with network wait)
//!
//! # DRY note
//!
//! v1.0.97: `claude_runner.rs` now hosts the shared Claude invocation helpers
//! (`run_claude`, `parse_claude_output`, `spawn_with_memory_limit`). The queue
//! DB schema in `ingest_claude.rs` still duplicates `open_queue_db` here — a
//! future pass can unify them.

mod extraction;
mod postprocess;
mod queue;
mod scan;
use extraction::{
    call_body_enrich, call_body_extract, call_deep_research_synth, call_description_enrich,
    call_domain_classify, call_entity_connect, call_entity_description, call_entity_type_validate,
    call_graph_audit, call_memory_bindings, call_reembed, call_relation_reclassify,
    call_weight_calibrate, find_codex_binary, EnrichItemResult,
};
use postprocess::{
    persist_enriched_body, persist_entity_description, persist_memory_bindings,
    reembed_memory_vector, take_enrich_backend,
};
pub use queue::{cleanup_queue_entry, DeadItem, DeadSummary, EnrichStatus, WaitingItem};
use queue::{
    enqueue_candidate, item_type_for, open_queue_db, prune_dead_orphans, record_item_failure,
    skipped_item_keys,
};
use scan::{scan_isolated_entity_pairs, scan_operation, scan_unbound_memories};

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
    /// memory-bindings LINKS each memory to the EXISTING entities extracted from
    /// its body — it does not invent a new graph, it only connects what is missing. Scans
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
        required_unless_present_any = ["status", "list_dead", "requeue_dead", "prune_dead_orphans"]
    )]
    pub operation: Option<EnrichOperation>,

    /// LLM provider to use. Required for write operations; not needed for the
    /// read-only queue inspectors (`--status` / `--list-dead` /
    /// `--requeue-dead`), which never call the LLM (GAP-SG-31).
    #[arg(
        long,
        value_enum,
        required_unless_present_any = ["status", "list_dead", "requeue_dead", "prune_dead_orphans"]
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

    /// GAP-SG-66: prune ORPHAN dead-letter rows — remove every `status='dead'`
    /// memory row whose `item_key` (the memory name) no longer exists in the
    /// main DB for this namespace. These are terminal "not found" failures that
    /// `--requeue-dead` can never recover (re-processing re-fails the same way),
    /// so they inflate `queue_dead` forever. Read-only on the main DB; deletes
    /// only confirmed-orphan rows from the queue sidecar. Entity-keyed dead rows
    /// are left untouched. No LLM, no singleton — like `--list-dead`.
    #[arg(long)]
    pub prune_dead_orphans: bool,

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

    /// G29 Step 4: minimum trigram-Jaccard similarity between the
    /// original body and the LLM-rewritten body for the rewrite to be
    /// accepted. Scores below the threshold are rejected and emitted as
    /// `EnrichItemResult::PreservationFailed`. Default 0.7 (per the G29
    /// gap specification). Ignored when `--operation` is not
    /// `body-enrich`.
    #[arg(long, value_name = "FLOAT", default_value_t = 0.7)]
    pub preserve_threshold: f64,

    /// G33 Step 3: when set, validate `--codex-model` against the
    /// ChatGPT Pro OAuth accepted-model list and abort with a
    /// suggestion when the value is unknown. Default true (fail fast
    /// to avoid burning OAuth turns). Set to false to opt out.
    #[arg(long, default_value_t = true)]
    pub codex_model_validate: bool,

    /// G33 Step 3: when set together with an invalid `--codex-model`,
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
    /// v1.0.84 (ADR-0042): discriminator of the LLM backend that actually
    /// ran the re-embedding during enrich. `"claude" | "codex" | "none"`.
    /// Absent on the wire when `None` (kept for happy-path envelope cleanliness,
    /// or when the operation did not involve a re-embed).
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

// Queue functions and structs moved to queue.rs

// LLM call_claude and call_openrouter moved to extraction.rs

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
    let Some(exit) = child.wait_timeout(timeout).map_err(AppError::Io)? else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(AppError::Validation(format!(
            "preflight probe timed out after {}s",
            start.elapsed().as_secs()
        )));
    };
    let mut stdout = Vec::new();
    if let Some(mut out) = child.stdout.take() {
        std::io::Read::read_to_end(&mut out, &mut stdout).map_err(AppError::Io)?;
    }
    let mut stderr = Vec::new();
    if let Some(mut err) = child.stderr.take() {
        std::io::Read::read_to_end(&mut err, &mut stderr).map_err(AppError::Io)?;
    }
    Ok(std::process::Output {
        status: exit,
        stdout,
        stderr,
    })
}

// Scan functions moved to scan.rs

// Persist functions moved to postprocess.rs

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
    if args.list_dead || args.requeue_dead || args.prune_dead_orphans {
        let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
        let op_label = format!("{:?}", args.operation());
        let paths = AppPaths::resolve(args.db.as_deref())?;
        let queue_path = crate::paths::sidecar_path(&paths.db, ".enrich-queue.sqlite");
        let queue_conn = open_queue_db(&queue_path)?;
        // GAP-SG-66: prune orphan dead rows (memory gone) — needs the main DB to
        // confirm the referenced memory is truly absent before deleting.
        if args.prune_dead_orphans {
            ensure_db_ready(&paths)?;
            let main_conn = open_rw(&paths.db)?;
            let pruned = prune_dead_orphans(&queue_conn, &main_conn, &op_label, &namespace)?;
            let dead_total: i64 = queue_conn
                .query_row(
                    "SELECT COUNT(*) FROM queue WHERE status='dead' \
                     AND (operation = ?1 OR operation IS NULL)",
                    rusqlite::params![op_label],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            emit_json(&DeadSummary {
                summary: true,
                operation: op_label,
                namespace,
                action: "prune-dead-orphans",
                dead_total,
                requeued: 0,
                pruned,
            });
            return Ok(());
        }
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
                pruned: 0,
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
            pruned: 0,
        });
        return Ok(());
    }

    if args.status {
        let paths = AppPaths::resolve(args.db.as_deref())?;
        ensure_db_ready(&paths)?;
        let conn = open_rw(&paths.db)?;
        let namespace = crate::namespace::resolve_namespace(args.namespace.as_deref())?;
        let unbound_backlog = scan_unbound_memories(&conn, &namespace, None, &[])?.len();
        let queue_path = crate::paths::sidecar_path(&paths.db, ".enrich-queue.sqlite");
        let queue_conn = open_queue_db(&queue_path)?;
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
    let mut scan_result = scan_operation(&conn, &namespace, args)?;
    // GAP-SG-69: body-enrich candidates are scanned purely by `LENGTH(body) <
    // min_output_chars`, so a short body whose rewrite the preservation guard
    // keeps rejecting is re-scanned every pass — items_total never reaches 0 and
    // `--until-empty` never converges (the detached worker reported a stuck
    // backlog for 30+ min). Exclude memories already vetoed `status='skipped'`
    // for this operation in the sidecar queue; `cleanup_queue_entry`
    // (remember/edit/forget/purge) clears the veto when the body actually
    // changes, so a genuinely updated memory is reconsidered automatically.
    if matches!(args.operation(), EnrichOperation::BodyEnrich) {
        let q_path = crate::paths::sidecar_path(&paths.db, ".enrich-queue.sqlite");
        if let Ok(q) = open_queue_db(&q_path) {
            if let Ok(vetoed) = skipped_item_keys(&q, &format!("{:?}", args.operation())) {
                scan_result.retain(|k| !vetoed.contains(k));
            }
        }
    }
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

    // Queue setup for resume/retry (GAP-SG-64: sidecar alongside --db)
    let queue_path = crate::paths::sidecar_path(&paths.db, ".enrich-queue.sqlite");
    let queue_conn = open_queue_db(&queue_path)?;

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
            let mut rescan = scan_operation(&conn, &namespace, args)?;
            // GAP-SG-69: drop memories already vetoed `status='skipped'` so the
            // re-scan converges instead of re-enqueuing a non-expandable short
            // body every iteration (body-enrich only; the verdict persists in
            // the sidecar queue and is cleared by cleanup_queue_entry on edit).
            if matches!(args.operation(), EnrichOperation::BodyEnrich) {
                if let Ok(vetoed) = skipped_item_keys(&queue_conn, &op_label) {
                    rescan.retain(|k| !vetoed.contains(k));
                }
            }
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
                    let queue_path = &queue_path;
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
                        let w_queue = match open_queue_db(queue_path) {
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

                            // provider_binary validated upfront (Some for every
                            // LLM-backed op; None only for ReEmbed, which ignores
                            // it). unwrap_or_default yields "" solely in that
                            // unread case, so a broken invariant surfaces as a
                            // recoverable per-item error instead of a panic.
                            let provider_bin = provider_binary.unwrap_or_else(|| std::path::Path::new(""));
                            let call_result = match operation {
                                EnrichOperation::MemoryBindings | EnrichOperation::AugmentBindings => call_memory_bindings(&w_conn, namespace, &item_key, provider_bin, provider_model, provider_timeout, mode),
                                EnrichOperation::EntityDescriptions => call_entity_description(&w_conn, namespace, &item_key, provider_bin, provider_model, provider_timeout, mode),
                                EnrichOperation::BodyEnrich => call_body_enrich(&w_conn, namespace, &item_key, provider_bin, provider_model, provider_timeout, mode, min_oc, max_oc, prompt_tpl, args.preserve_threshold, paths, llm_backend, embedding_backend),
                                EnrichOperation::ReEmbed => call_reembed(&w_conn, namespace, &item_key, paths, llm_backend, embedding_backend),
                                EnrichOperation::WeightCalibrate => call_weight_calibrate(&w_conn, namespace, &item_key, provider_bin, provider_model, provider_timeout, mode),
                                EnrichOperation::RelationReclassify => call_relation_reclassify(&w_conn, namespace, &item_key, provider_bin, provider_model, provider_timeout, mode),
                                EnrichOperation::EntityConnect | EnrichOperation::CrossDomainBridges => call_entity_connect(&w_conn, namespace, &item_key, provider_bin, provider_model, provider_timeout, mode),
                                EnrichOperation::EntityTypeValidate => call_entity_type_validate(&w_conn, namespace, &item_key, provider_bin, provider_model, provider_timeout, mode),
                                EnrichOperation::DescriptionEnrich => call_description_enrich(&w_conn, namespace, &item_key, provider_bin, provider_model, provider_timeout, mode),
                                EnrichOperation::DomainClassify => call_domain_classify(&w_conn, namespace, &item_key, provider_bin, provider_model, provider_timeout, mode),
                                EnrichOperation::GraphAudit => call_graph_audit(&w_conn, namespace, &item_key, provider_bin, provider_model, provider_timeout, mode),
                                EnrichOperation::DeepResearchSynth => call_deep_research_synth(&w_conn, namespace, &item_key, provider_bin, provider_model, provider_timeout, mode),
                                EnrichOperation::BodyExtract => call_body_extract(&w_conn, namespace, &item_key, provider_bin, provider_model, provider_timeout, mode, args.body_extract_graph_only),
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

                // See worker note: provider_binary is Some for every LLM-backed
                // op; "" here only for ReEmbed, which never reads it.
                let provider_bin = provider_binary
                    .as_deref()
                    .unwrap_or_else(|| std::path::Path::new(""));
                let call_result = match args.operation() {
                    EnrichOperation::MemoryBindings | EnrichOperation::AugmentBindings => {
                        call_memory_bindings(
                            &conn,
                            &namespace,
                            &item_key,
                            provider_bin,
                            provider_model,
                            provider_timeout,
                            &args.mode(),
                        )
                    }
                    EnrichOperation::EntityDescriptions => call_entity_description(
                        &conn,
                        &namespace,
                        &item_key,
                        provider_bin,
                        provider_model,
                        provider_timeout,
                        &args.mode(),
                    ),
                    EnrichOperation::BodyEnrich => call_body_enrich(
                        &conn,
                        &namespace,
                        &item_key,
                        provider_bin,
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
                        provider_bin,
                        provider_model,
                        provider_timeout,
                        &args.mode(),
                    ),
                    EnrichOperation::RelationReclassify => call_relation_reclassify(
                        &conn,
                        &namespace,
                        &item_key,
                        provider_bin,
                        provider_model,
                        provider_timeout,
                        &args.mode(),
                    ),
                    EnrichOperation::EntityConnect | EnrichOperation::CrossDomainBridges => {
                        call_entity_connect(
                            &conn,
                            &namespace,
                            &item_key,
                            provider_bin,
                            provider_model,
                            provider_timeout,
                            &args.mode(),
                        )
                    }
                    EnrichOperation::EntityTypeValidate => call_entity_type_validate(
                        &conn,
                        &namespace,
                        &item_key,
                        provider_bin,
                        provider_model,
                        provider_timeout,
                        &args.mode(),
                    ),
                    EnrichOperation::DescriptionEnrich => call_description_enrich(
                        &conn,
                        &namespace,
                        &item_key,
                        provider_bin,
                        provider_model,
                        provider_timeout,
                        &args.mode(),
                    ),
                    EnrichOperation::DomainClassify => call_domain_classify(
                        &conn,
                        &namespace,
                        &item_key,
                        provider_bin,
                        provider_model,
                        provider_timeout,
                        &args.mode(),
                    ),
                    EnrichOperation::GraphAudit => call_graph_audit(
                        &conn,
                        &namespace,
                        &item_key,
                        provider_bin,
                        provider_model,
                        provider_timeout,
                        &args.mode(),
                    ),
                    EnrichOperation::DeepResearchSynth => call_deep_research_synth(
                        &conn,
                        &namespace,
                        &item_key,
                        provider_bin,
                        provider_model,
                        provider_timeout,
                        &args.mode(),
                    ),
                    EnrichOperation::BodyExtract => call_body_extract(
                        &conn,
                        &namespace,
                        &item_key,
                        provider_bin,
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
        // GAP-SG-69: keep the sidecar queue while it still holds `skipped`
        // verdicts. Those rows tell the next scan which short bodies are
        // non-expandable; removing the file would lose the veto and the
        // body-enrich backlog would never converge. cleanup_queue_entry clears
        // a row when its memory is edited/forgotten, so the veto is not permanent.
        let skipped_remaining: i64 = queue_conn
            .query_row(
                "SELECT COUNT(*) FROM queue WHERE status='skipped'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if dead == 0 && skipped_remaining == 0 {
            let _ = std::fs::remove_file(&queue_path);
        }
    }

    Ok(())
}

// EnrichItemResult + call_* functions moved to extraction.rs

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
