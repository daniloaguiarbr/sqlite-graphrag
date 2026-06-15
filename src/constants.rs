//! Compile-time constants shared across the crate.
//!
//! Grouped into embedding configuration, length and size limits, SQLite
//! pragmas and retrieval tuning knobs. Values are taken from the PRD and
//! must stay in sync with the migrations under `migrations/`.
//!
//! ## Dynamic concurrency permit calculation
//!
//! The maximum number of simultaneous instances can be adjusted at runtime
//! using the formula:
//!
//! ```text
//! permits = min(cpus, available_memory_mb / LLM_WORKER_RSS_MB) * 0.5
//! ```
//!
//! where `available_memory_mb` is obtained via `sysinfo::System::available_memory()`
//! converted to MiB. The result is capped at `MAX_CONCURRENT_CLI_INSTANCES`
//! and floored at 1.

/// Default embedding vector dimensionality (v1.0.79, G42/S1).
///
/// Lowered from 384 to 64: with the LLM-only backend (v1.0.76+) each float
/// costs ~8 autoregressive output tokens, so 384 dims ≈ 3072 tokens per
/// vector at 50-100 tokens/s (30-60s per vector). 64 dims retain 90%+
/// retrieval quality for corpora under 100k memories (Matryoshka
/// Representation Learning, arXiv 2205.13147) while cutting generation
/// time ~6x. The historical 384 value matched `multilingual-e5-small`.
pub const DEFAULT_EMBEDDING_DIM: usize = 64;

/// Active embedding dimensionality for this process. `0` means unresolved.
static ACTIVE_EMBEDDING_DIM: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Resolves the active embedding dimensionality (single source of truth).
///
/// Precedence:
/// 1. `SQLITE_GRAPHRAG_EMBEDDING_DIM` env var (also set by the global
///    `--embedding-dim` flag before dispatch);
/// 2. the value recorded via [`set_active_embedding_dim`] — populated from
///    the `dim` key of `schema_meta` when the database is opened, so
///    existing 384-dim databases keep working unchanged;
/// 3. [`DEFAULT_EMBEDDING_DIM`].
pub fn embedding_dim() -> usize {
    if let Some(env_dim) = embedding_dim_from_env() {
        return env_dim;
    }
    let active = ACTIVE_EMBEDDING_DIM.load(std::sync::atomic::Ordering::Acquire);
    if active != 0 {
        return active;
    }
    DEFAULT_EMBEDDING_DIM
}

/// Reads and validates the env-var override. Values outside [8, 4096]
/// are rejected (returns `None`) so a typo cannot produce degenerate
/// vectors or multi-MB embedding rows.
pub fn embedding_dim_from_env() -> Option<usize> {
    let raw = std::env::var("SQLITE_GRAPHRAG_EMBEDDING_DIM").ok()?;
    match raw.parse::<usize>() {
        Ok(n) if (8..=4096).contains(&n) => Some(n),
        // G49: an invalid value silently fell back to the default (64),
        // letting a typo permanently stamp a new database with the wrong
        // dimensionality. Warn loudly instead of discarding in silence.
        _ => {
            tracing::warn!(
                value = %raw,
                "SQLITE_GRAPHRAG_EMBEDDING_DIM is invalid (expected an integer in [8, 4096]); ignoring and using the database/default dimensionality"
            );
            None
        }
    }
}

/// Records the dimensionality found in the opened database
/// (`schema_meta.dim`). Out-of-range values are ignored. The env var,
/// when set, always wins over this value (see [`embedding_dim`]).
pub fn set_active_embedding_dim(dim: usize) {
    if (8..=4096).contains(&dim) {
        ACTIVE_EMBEDDING_DIM.store(dim, std::sync::atomic::Ordering::Release);
    }
}

// G46: FASTEMBED_MODEL_DEFAULT removed — the fastembed model was deleted in
// v1.0.76 (LLM-only build); `schema_meta.model` now records the CLI version.

/// Batch size for `fastembed` encoding calls.
pub const FASTEMBED_BATCH_SIZE: usize = 32;

/// Maximum byte length for a memory `name` field in kebab-case.
pub const MAX_MEMORY_NAME_LEN: usize = 80;

/// Maximum byte length for an `ingest`-derived kebab-case name.
///
/// Stricter than `MAX_MEMORY_NAME_LEN` (80) to leave headroom for collision
/// suffixes (`-2`, `-10`, ...) when multiple files derive to the same base.
/// Used exclusively by `src/commands/ingest.rs`.
pub const DERIVED_NAME_MAX_LEN: usize = 60;

/// Maximum character length for a memory `description` field.
pub const MAX_MEMORY_DESCRIPTION_LEN: usize = 500;

/// Hard upper bound on memory `body` length in bytes.
pub const MAX_MEMORY_BODY_LEN: usize = 512_000;

/// Body character count above which the body is split into chunks.
pub const MAX_BODY_CHARS_BEFORE_CHUNK: usize = 8_000;

/// Maximum attempts when a statement returns `SQLITE_BUSY`.
pub const MAX_SQLITE_BUSY_RETRIES: u32 = 5;

/// Base delay in milliseconds for the first SQLITE_BUSY retry.
///
/// Each subsequent attempt doubles the delay (exponential backoff):
/// 300 ms → 600 ms → 1200 ms → 2400 ms → 4800 ms (≈ 9.3 s total).
pub const SQLITE_BUSY_BASE_DELAY_MS: u64 = 300;

/// Query timeout applied to statements in milliseconds.
pub const QUERY_TIMEOUT_MILLIS: u64 = 5_000;

/// Jaccard threshold above which two memories are considered fuzzy duplicates.
pub const DEDUP_FUZZY_THRESHOLD: f64 = 0.8;

/// Cosine distance threshold below which two memories are semantic duplicates.
pub const DEDUP_SEMANTIC_THRESHOLD: f32 = 0.1;

/// Maximum number of hops allowed in graph traversals.
pub const MAX_GRAPH_HOPS: u32 = 2;

/// Minimum relationship weight required for traversal inclusion.
pub const MIN_RELATION_WEIGHT: f64 = 0.3;

/// Default traversal depth for `related` when `--hops` is omitted.
pub const DEFAULT_MAX_HOPS: u32 = 2;

/// Default minimum weight filter applied during graph traversal.
pub const DEFAULT_MIN_WEIGHT: f64 = 0.3;

/// Default weight assigned to newly created relationships.
pub const DEFAULT_RELATION_WEIGHT: f64 = 0.5;

/// Default `k` used by `recall` when the caller omits `--k`.
pub const DEFAULT_K_RECALL: usize = 10;

/// Default `k` for memory KNN searches when the caller omits `--k`.
pub const K_MEMORIES_DEFAULT: usize = 10;

/// Default `k` for entity KNN searches during graph expansion.
pub const K_ENTITIES_SEARCH: usize = 5;

/// Default upper bound on distinct entities persisted per memory.
///
/// Bumped from 30 → 50 in v1.0.43 to reduce semantic loss on rich documents.
/// Configurable at runtime via `SQLITE_GRAPHRAG_MAX_ENTITIES_PER_MEMORY`.
pub const MAX_ENTITIES_PER_MEMORY: usize = 50;

/// Resolves the per-memory entity cap, honouring the env-var override.
///
/// v1.0.43: makes the cap (default 50) configurable via `SQLITE_GRAPHRAG_MAX_ENTITIES_PER_MEMORY`.
/// Stress tests showed inputs with 33-46 candidates being truncated at the old cap of 30.
/// Values outside [1, 1000] fall back to the default.
pub fn max_entities_per_memory() -> usize {
    std::env::var("SQLITE_GRAPHRAG_MAX_ENTITIES_PER_MEMORY")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| (1..=1_000).contains(&n))
        .unwrap_or(MAX_ENTITIES_PER_MEMORY)
}

/// Upper bound on distinct relationships persisted per memory.
pub const MAX_RELATIONSHIPS_PER_MEMORY: usize = 50;

/// Resolves the per-memory relationship cap, honouring the env-var override.
///
/// v1.0.22: makes the cap (default 50) configurable via `SQLITE_GRAPHRAG_MAX_RELATIONS_PER_MEMORY`.
/// Audit found that rich documents silently hit the cap; users with dense technical corpora
/// can raise it via env. Values outside [1, 10000] fall back to the default.
pub fn max_relationships_per_memory() -> usize {
    std::env::var("SQLITE_GRAPHRAG_MAX_RELATIONS_PER_MEMORY")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| (1..=10_000).contains(&n))
        .unwrap_or(MAX_RELATIONSHIPS_PER_MEMORY)
}

/// Character length of the description preview shown in `list` output.
pub const TEXT_DESCRIPTION_PREVIEW_LEN: usize = 100;

/// `PRAGMA busy_timeout` value applied on every connection.
pub const BUSY_TIMEOUT_MILLIS: i32 = 5_000;

/// `PRAGMA cache_size` value in kibibytes (negative means KiB).
pub const CACHE_SIZE_KB: i32 = -64_000;

/// `PRAGMA mmap_size` value in bytes applied to each connection.
pub const MMAP_SIZE_BYTES: i64 = 268_435_456;

/// `PRAGMA wal_autocheckpoint` threshold in pages.
pub const WAL_AUTOCHECKPOINT_PAGES: i32 = 1_000;

/// Default `k` constant used by Reciprocal Rank Fusion in `hybrid-search`.
pub const RRF_K_DEFAULT: u32 = 60;

/// Chunk size expressed in tokens for body splitting.
pub const CHUNK_SIZE_TOKENS: usize = 400;

/// Token overlap between consecutive chunks.
pub const CHUNK_OVERLAP_TOKENS: usize = 50;

/// Explicit operational guard for multi-chunk documents in `remember`.
///
/// The multi-chunk path uses serial embeddings to avoid ONNX memory amplification.
/// This limit preserves a clear operational ceiling for agents and scripts.
pub const REMEMBER_MAX_SAFE_MULTI_CHUNKS: usize = 512;

/// Ceiling on chunks per controlled micro-batch in `remember`.
///
/// The `fastembed` runtime uses `BatchLongest` padding, so oversized batches amplify
/// the cost of the longest chunk. This ceiling keeps batches small even when chunks are short.
pub const REMEMBER_MAX_CONTROLLED_BATCH_CHUNKS: usize = 4;

/// Maximum padded-token budget per controlled micro-batch in `remember`.
///
/// The budget uses `max_tokens_no_batch * batch_size`, approximating the real cost of
/// `BatchLongest` padding. Values exceeding this fall back to smaller batches or serialisation.
pub const REMEMBER_MAX_CONTROLLED_BATCH_PADDED_TOKENS: usize = 512;

/// Prefix prepended to bodies before embedding as required by E5 models.
pub const PASSAGE_PREFIX: &str = "passage: ";

/// Prefix prepended to queries before embedding as required by E5 models.
pub const QUERY_PREFIX: &str = "query: ";

/// Crate version string sourced from `CARGO_PKG_VERSION` at build time.
pub const SQLITE_GRAPHRAG_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Batch size for GLiNER NER forward passes.
///
/// Larger values amortise fixed forward-pass overhead but increase peak RAM.
/// Memory guide (CPU only, max 512-token windows):
///   N=4  → ~54 MiB peak
///   N=8  → ~108 MiB peak  ← default
///   N=16 → ~216 MiB peak
///   N=32 → ~432 MiB peak  (not recommended without 16+ GiB RAM)
///
/// Override via `GRAPHRAG_NER_BATCH_SIZE` env var. Values outside [1, 32] are
/// clamped silently.
pub fn ner_batch_size() -> usize {
    std::env::var("GRAPHRAG_NER_BATCH_SIZE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(8)
        .clamp(1, 32)
}

/// Default cap on tokens fed to GLiNER NER per memory body.
///
/// v1.0.31: large markdown documents (>50 KB) tokenise into thousands of
/// 512-token windows, each requiring a CPU forward pass that takes hundreds
/// of milliseconds. A 68 KB document was observed taking 5+ minutes.
/// Truncating the input before sliding-window construction caps the worst-case
/// latency while preserving extraction quality for the leading body region.
///
/// Regex prefilter still runs on the full body, so URLs, emails, UUIDs,
/// all-caps identifiers and CamelCase brand names are extracted regardless.
pub const EXTRACTION_MAX_TOKENS_DEFAULT: usize = 5_000;

/// Resolves the per-body NER token cap, honouring the env-var override.
///
/// Override via `SQLITE_GRAPHRAG_EXTRACTION_MAX_TOKENS` env var. Values outside
/// [512, 100_000] fall back to [`EXTRACTION_MAX_TOKENS_DEFAULT`].
pub fn extraction_max_tokens() -> usize {
    std::env::var("SQLITE_GRAPHRAG_EXTRACTION_MAX_TOKENS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| (512..=100_000).contains(&n))
        .unwrap_or(EXTRACTION_MAX_TOKENS_DEFAULT)
}

/// GLiNER confidence threshold for span scoring.
///
/// Override via `SQLITE_GRAPHRAG_GLINER_THRESHOLD` env var. Values outside
/// `[0.0, 1.0]` are ignored and the default `0.5` is used.
pub fn gliner_confidence_threshold() -> f32 {
    std::env::var("SQLITE_GRAPHRAG_GLINER_THRESHOLD")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|&v| (0.0..=1.0).contains(&v))
        .unwrap_or(0.5)
}

/// HuggingFace repository for the GLiNER ONNX model.
///
/// Override via `SQLITE_GRAPHRAG_GLINER_MODEL` env var.
pub fn gliner_model_repo() -> String {
    std::env::var("SQLITE_GRAPHRAG_GLINER_MODEL")
        .unwrap_or_else(|_| "onnx-community/gliner_multi-v2.1".to_string())
}

/// PRD-canonical regex that validates names and namespaces. Allows 1 char `[a-z0-9]`
/// OR a 2-80 char string starting with a letter and ending with a letter/digit,
/// containing only `[a-z0-9-]`. Rejects the `__` prefix (internal reserved).
pub const NAME_SLUG_REGEX: &str = r"^[a-z][a-z0-9-]{0,78}[a-z0-9]$|^[a-z0-9]$";

static NAME_SLUG_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();

/// Returns a reference to the compiled [`NAME_SLUG_REGEX`] pattern.
/// Compiled once on first call, cached via `OnceLock`.
pub fn name_slug_regex() -> &'static regex::Regex {
    NAME_SLUG_RE.get_or_init(|| {
        regex::Regex::new(NAME_SLUG_REGEX).expect("NAME_SLUG_REGEX is a valid pattern")
    })
}

/// Default retention period (days) used by `purge` when `--retention-days` is omitted.
pub const PURGE_RETENTION_DAYS_DEFAULT: u32 = 90;

/// Maximum number of simultaneously active namespaces (deleted_at IS NULL). Exit 5 when exceeded.
pub const MAX_NAMESPACES_ACTIVE: u32 = 100;

/// Maximum tokens accepted by an embedding input before chunking.
pub const EMBEDDING_MAX_TOKENS: usize = 512;

/// Maximum result count from the recursive graph CTE in `recall`.
pub const K_GRAPH_MATCHES_LIMIT: usize = 20;

/// Default `--limit` for `list` when omitted.
pub const K_LIST_DEFAULT_LIMIT: usize = 100;

/// Default `--limit` for `graph entities` when omitted.
pub const K_GRAPH_ENTITIES_DEFAULT_LIMIT: usize = 50;

/// Default `--limit` for `related` when omitted.
pub const K_RELATED_DEFAULT_LIMIT: usize = 10;

/// Default `--limit` for `history` when omitted.
pub const K_HISTORY_DEFAULT_LIMIT: usize = 20;

/// Default weight for the vector contribution in the `hybrid-search` RRF formula.
pub const WEIGHT_VEC_DEFAULT: f64 = 1.0;

/// Default weight for the BM25 text contribution in the `hybrid-search` RRF formula.
pub const WEIGHT_FTS_DEFAULT: f64 = 1.0;

/// Character size of the body preview emitted in text/markdown formats.
pub const TEXT_BODY_PREVIEW_LEN: usize = 200;

/// Default value injected into ORT_NUM_THREADS when not set by the user.
pub const ORT_NUM_THREADS_DEFAULT: &str = "1";

/// Default value injected into ORT_INTRA_OP_NUM_THREADS when not set.
pub const ORT_INTRA_OP_NUM_THREADS_DEFAULT: &str = "1";

/// Default value injected into OMP_NUM_THREADS when not set by the user.
pub const OMP_NUM_THREADS_DEFAULT: &str = "1";

/// Exit code for partial batch failure (PRD line 1822). Conflicts with DbBusy in v1.x;
/// in v2.0.0 DbBusy migrates to 15 and this code takes 13 per PRD.
pub const BATCH_PARTIAL_FAILURE_EXIT_CODE: i32 = 13;

/// Exit code for DbBusy in v2.0.0 (migrated from 13 to free 13 for batch failure).
pub const DB_BUSY_EXIT_CODE: i32 = 15;

/// Filename used for the advisory exclusive lock that prevents parallel invocations.
pub const CLI_LOCK_FILE: &str = "cli.lock";

/// Polling interval in milliseconds used by `--wait-lock` between `try_lock_exclusive` attempts.
pub const CLI_LOCK_POLL_INTERVAL_MS: u64 = 500;

/// Process exit code returned when the lock is busy and no wait was requested (EX_TEMPFAIL).
pub const CLI_LOCK_EXIT_CODE: i32 = 75;

/// Maximum number of CLI instances running simultaneously.
///
/// Limits the counting
/// semaphore in [`crate::lock`] to prevent memory overload when multiple parallel
/// v1.0.75 (G18 solution): removed the rigid 4-slot ceiling. The adaptive
/// `calculate_safe_concurrency` function in [`crate::lock`]` now reports
/// the dynamic limit. This constant is preserved as a *legacy fallback*
/// when the dynamic calculation cannot be performed (e.g. when `sysinfo`
/// cannot read `/proc/meminfo`).
///
/// Operators should prefer passing `--max-concurrency` explicitly OR
/// letting the runtime compute the limit. The default ceiling is intentionally
/// higher (16) so the legacy 4-slot hard cap does not silently reappear.
pub const MAX_CONCURRENT_CLI_INSTANCES: usize = 16;

/// G28-B (v1.0.68): polling interval in milliseconds used by
/// `acquire_job_singleton` between retry attempts when another invocation
/// already holds the singleton for `(job_type, namespace)`.
pub const JOB_SINGLETON_POLL_INTERVAL_MS: u64 = 1000;

/// Minimum available memory in MiB required before starting model loading.
///
/// If `sysinfo::System::available_memory() / 1_048_576` falls below this value,
/// the invocation is aborted with [`crate::errors::AppError::LowMemory`]
/// (exit code [`LOW_MEMORY_EXIT_CODE`]).
pub const MIN_AVAILABLE_MEMORY_MB: u64 = 2_048;

/// Maximum process RSS in MiB before aborting embedding operations.
/// Users can override via `--max-rss-mb`. Set to 8 GiB by default.
pub const DEFAULT_MAX_RSS_MB: u64 = 8_192;

/// Maximum time in seconds an instance waits to acquire a concurrency slot.
///
/// Passed as the default for `--max-wait-secs` in the CLI. After exhausting this limit,
/// the invocation returns [`crate::errors::AppError::AllSlotsFull`] with exit code
/// [`CLI_LOCK_EXIT_CODE`] (75).
pub const CLI_LOCK_DEFAULT_WAIT_SECS: u64 = 300;

/// v1.0.75 (G18 + G23): expected RSS in MiB for an LLM-only worker that
/// spawns a `claude -p` or `codex exec` subprocess. Much lower than the
/// embedding cost because the ONNX model is not loaded per-worker.
pub const LLM_WORKER_RSS_MB: u64 = 350;

/// Process exit code returned when available memory is below [`MIN_AVAILABLE_MEMORY_MB`].
///
/// Value `77` is `EX_NOPERM` in glibc sysexits, reused here to indicate
/// "insufficient system resource to proceed".
pub const LOW_MEMORY_EXIT_CODE: i32 = 77;

/// Process exit code returned when a duplicate memory or entity is detected (exit 9).
///
/// Moved from `2` to `9` in v1.0.52 to free exit code `2` for future use and align
/// with the PRD exit code contract. Shell callers and LLM agents must use `9` from
/// this version onwards.
pub const DUPLICATE_EXIT_CODE: i32 = 9;

/// Process exit code returned when shutdown is requested via SIGINT/SIGTERM/SIGHUP
/// (v1.0.82, GAP-002 final).
///
/// The shell sees this code INSTEAD of the legacy `128 + signal` (130/143/129) so
/// that LLM agents and orchestrators can branch on a single deterministic value
/// when the operation was cancelled by the user. The signal name is preserved in
/// the JSON envelope emitted before exit (`{"code":19,"signal":"SIGINT",...}`).
pub const SHUTDOWN_EXIT_CODE: i32 = 19;

/// Canonical value of `PRAGMA user_version` written after migrations.
///
/// **Why 49 instead of `CURRENT_SCHEMA_VERSION` (9)?**
/// `user_version` is a 32-bit integer that SQLite reserves for application use.
/// We deliberately set it to a project-specific marker (49 = decimal) so external
/// inspection tools (`sqlite3 db.sqlite "PRAGMA user_version"`, the `file` command,
/// SQLite browser GUIs) can distinguish a sqlite-graphrag database from a generic
/// SQLite file at a glance. The application-level schema version (9, matching
/// `CURRENT_SCHEMA_VERSION`) is stored in the `schema_meta` table and exposed via
/// `health --json`/`stats --json`. Bumping migrations does NOT change this constant.
/// Refinery uses its own `refinery_schema_history` table for migration bookkeeping.
pub const SCHEMA_USER_VERSION: i64 = 50;

/// Current schema version, equal to the highest migration number in `migrations/Vnnn__*.sql`.
///
/// Added in v1.0.27 as a runtime and test sanity check.
/// Must be bumped in sync with new Refinery migrations; the unit test
/// `schema_version_matches_migrations_count` validates this automatically.
pub const CURRENT_SCHEMA_VERSION: u32 = 15;

#[cfg(test)]
mod tests_schema_version {
    use super::CURRENT_SCHEMA_VERSION;

    #[test]
    fn schema_version_matches_migrations_count() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let migrations_dir = std::path::Path::new(manifest_dir).join("migrations");
        let count = std::fs::read_dir(&migrations_dir)
            .expect("migrations directory must exist")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().starts_with('V'))
            .count() as u32;
        assert_eq!(
            CURRENT_SCHEMA_VERSION, count,
            "CURRENT_SCHEMA_VERSION ({CURRENT_SCHEMA_VERSION}) must equal the number of V*.sql migrations ({count})"
        );
    }
}
