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
//! permits = min(cpus, available_memory_mb / EMBEDDING_LOAD_EXPECTED_RSS_MB) * 0.5
//! ```
//!
//! where `available_memory_mb` is obtained via `sysinfo::System::available_memory()`
//! converted to MiB. The result is capped at `MAX_CONCURRENT_CLI_INSTANCES`
//! and floored at 1.

/// Embedding vector dimensionality produced by `multilingual-e5-small`.
pub const EMBEDDING_DIM: usize = 384;

/// Default `fastembed` model identifier used by `remember` and `recall`.
pub const FASTEMBED_MODEL_DEFAULT: &str = "multilingual-e5-small";

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

/// Timeout in milliseconds for a single ping probe against the daemon socket.
pub const DAEMON_PING_TIMEOUT_MS: u64 = 10;

/// Idle duration in seconds before the daemon shuts itself down.
pub const DAEMON_IDLE_SHUTDOWN_SECS: u64 = 600;

/// Maximum wait time for the daemon to become healthy after auto-start.
pub const DAEMON_AUTO_START_MAX_WAIT_MS: u64 = 5_000;

/// Initial polling interval to check whether the daemon became healthy.
pub const DAEMON_AUTO_START_INITIAL_BACKOFF_MS: u64 = 50;

/// Ceiling on backoff between automatic daemon spawn attempts.
pub const DAEMON_AUTO_START_MAX_BACKOFF_MS: u64 = 30_000;

/// Base backoff used after daemon spawn/health failures.
pub const DAEMON_SPAWN_BACKOFF_BASE_MS: u64 = 500;

/// Maximum wait time to acquire the daemon spawn lock.
pub const DAEMON_SPAWN_LOCK_WAIT_MS: u64 = 2_000;

/// Prefix prepended to bodies before embedding as required by E5 models.
pub const PASSAGE_PREFIX: &str = "passage: ";

/// Prefix prepended to queries before embedding as required by E5 models.
pub const QUERY_PREFIX: &str = "query: ";

/// Crate version string sourced from `CARGO_PKG_VERSION` at build time.
pub const SQLITE_GRAPHRAG_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Batch size for BERT NER forward passes.
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

/// Default cap on tokens fed to BERT NER per memory body.
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

/// PRD-canonical regex that validates names and namespaces. Allows 1 char `[a-z0-9]`
/// OR a 2-80 char string starting with a letter and ending with a letter/digit,
/// containing only `[a-z0-9-]`. Rejects the `__` prefix (internal reserved).
pub const NAME_SLUG_REGEX: &str = r"^[a-z][a-z0-9-]{0,78}[a-z0-9]$|^[a-z0-9]$";

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
/// Aligned with `DAEMON_MAX_CONCURRENT_CLIENTS` from the PRD. Limits the counting
/// semaphore in [`crate::lock`] to prevent memory overload when multiple parallel
/// invocations attempt to load the ONNX model simultaneously.
pub const MAX_CONCURRENT_CLI_INSTANCES: usize = 4;

/// Minimum available memory in MiB required before starting model loading.
///
/// If `sysinfo::System::available_memory() / 1_048_576` falls below this value,
/// the invocation is aborted with [`crate::errors::AppError::LowMemory`]
/// (exit code [`LOW_MEMORY_EXIT_CODE`]).
pub const MIN_AVAILABLE_MEMORY_MB: u64 = 2_048;

/// Maximum time in seconds an instance waits to acquire a concurrency slot.
///
/// Passed as the default for `--max-wait-secs` in the CLI. After exhausting this limit,
/// the invocation returns [`crate::errors::AppError::AllSlotsFull`] with exit code
/// [`CLI_LOCK_EXIT_CODE`] (75).
pub const CLI_LOCK_DEFAULT_WAIT_SECS: u64 = 300;

/// Expected RSS in MiB for a single instance with the ONNX model loaded via fastembed.
///
/// Used in the formula `min(cpus, available_memory_mb / EMBEDDING_LOAD_EXPECTED_RSS_MB) * 0.5`
/// to compute the dynamic permit count.
///
/// Value calibrated on 2026-04-23 with `/usr/bin/time -v` against `sqlite-graphrag v1.0.3`
/// on the heavy commands `remember`, `recall`, and `hybrid-search`, all peaking near
/// 1.03 GiB RSS per process. The constant below rounds up with a defensive margin.
pub const EMBEDDING_LOAD_EXPECTED_RSS_MB: u64 = 1_100;

/// Process exit code returned when available memory is below [`MIN_AVAILABLE_MEMORY_MB`].
///
/// Value `77` is `EX_NOPERM` in glibc sysexits, reused here to indicate
/// "insufficient system resource to proceed".
pub const LOW_MEMORY_EXIT_CODE: i32 = 77;

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
pub const SCHEMA_USER_VERSION: i64 = 49;

/// Current schema version, equal to the highest migration number in `migrations/Vnnn__*.sql`.
///
/// Added in v1.0.27 as a runtime and test sanity check.
/// Must be bumped in sync with new Refinery migrations; the unit test
/// `schema_version_matches_migrations_count` validates this automatically.
pub const CURRENT_SCHEMA_VERSION: u32 = 9;

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
