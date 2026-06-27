//! # sqlite-graphrag
//!
//! Local GraphRAG memory for LLMs in a single SQLite file — zero external
//! services required.
//!
//! `sqlite-graphrag` is a CLI-first library that persists memories, entities and
//! typed relationships inside a single SQLite database. It combines FTS5
//! full-text search with `sqlite-vec` KNN over locally-generated embeddings to
//! expose a hybrid retrieval ranker tailored for LLM agents.
//!
//! ## CLI usage
//!
//! Install and initialize once, then save and recall memories:
//!
//! ```bash
//! cargo install sqlite-graphrag
//! sqlite-graphrag init
//! sqlite-graphrag remember \
//!     --name onboarding-note \
//!     --type user \
//!     --description "first memory" \
//!     --body "hello graphrag"
//! sqlite-graphrag recall "graphrag" --k 5
//! ```
//!
//! ## Crate layout
//!
//! The public modules group the CLI, the SQLite storage layer and the
//! supporting primitives (embedder, chunking, graph, namespace detection,
//! output, paths and pragmas). The CLI binary wires them together through the
//! commands in [`commands`].
//!
//! ## Exit codes
//!
//! Errors returned from [`errors::AppError`] map to deterministic exit codes
//! suitable for orchestration by shell scripts and LLM agents. Consult the
//! README for the full contract.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::OnceLock;
use tokio_util::sync::CancellationToken;

/// Signals that a shutdown signal (SIGINT / SIGTERM / SIGHUP) has been received.
///
/// Set in `main` via `ctrlc::set_handler`. Long-running subcommands can
/// poll [`shutdown_requested`] to shut down gracefully before timeout.
/// Async code should prefer [`cancel_token`] with `tokio::select!`.
pub static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Counter of shutdown signals received. 0=none, 1=graceful, 2+=forced exit.
pub static SIGNAL_COUNT: AtomicU8 = AtomicU8::new(0);

/// Signal number that triggered shutdown (2=SIGINT, 15=SIGTERM). 0=none.
static SIGNAL_NUMBER: AtomicU8 = AtomicU8::new(0);

static CANCEL: OnceLock<CancellationToken> = OnceLock::new();

/// Returns the process-wide cancellation token for async graceful shutdown.
///
/// The token is cancelled by the signal handler alongside [`SHUTDOWN`].
/// Async loops should use `token.cancelled().await` inside `tokio::select!`
/// for instant wake-up instead of polling [`shutdown_requested`].
pub fn cancel_token() -> &'static CancellationToken {
    CANCEL.get_or_init(CancellationToken::new)
}

/// Returns `true` if a shutdown signal has been received since the process started.
///
/// The value reflects the state of [`SHUTDOWN`]. Without a `ctrlc::set_handler` call,
/// the initial state is always `false`.
///
/// # Examples
///
/// ```
/// use sqlite_graphrag::shutdown_requested;
///
/// // Under normal startup conditions the signal has not been received.
/// assert!(!shutdown_requested());
/// ```
///
/// ```
/// use std::sync::atomic::Ordering;
/// use sqlite_graphrag::{SHUTDOWN, shutdown_requested};
///
/// // Simulate receiving a signal and verify that the function reflects the state.
/// SHUTDOWN.store(true, Ordering::Release);
/// assert!(shutdown_requested());
/// // Restore to avoid contaminating other tests.
/// SHUTDOWN.store(false, Ordering::Release);
/// ```
pub fn shutdown_requested() -> bool {
    // ORDERING: Acquire pairs with the Release store in the signal handler (main.rs).
    SHUTDOWN.load(Ordering::Acquire)
}

/// Returns the signal number that triggered shutdown (0 if none received).
///
/// Typically 2 (SIGINT) for Ctrl+C. Used to compute Unix-conventional exit
/// code 128+N in the main function.
pub fn shutdown_signal() -> u8 {
    SIGNAL_NUMBER.load(Ordering::Acquire)
}

/// Resets the global shutdown flag to `false` and zeroes the signal counters.
///
/// Returns `true` if the flag was previously set, `false` if it was already
/// cleared. Intended for tests and audit invocations where the SHUTDOWN flag
/// was contaminated by an earlier signal handler in the same process tree.
/// Production code must NOT call this — the only legitimate callers are
/// integration tests, audit scripts, and the `--ignore-shutdown` CLI flag.
///
/// Note: this only resets the `SHUTDOWN` flag. The global [`CancellationToken`]
/// remains in its previous cancelled state because `tokio_util::sync::CancellationToken`
/// is one-shot. Callers that need a resettable token must use a per-invocation
/// token (see [`should_obey_shutdown`]) instead of relying on the global one.
///
/// # Examples
///
/// ```
/// use std::sync::atomic::Ordering;
/// use sqlite_graphrag::{SHUTDOWN, try_reset_shutdown};
///
/// SHUTDOWN.store(true, Ordering::Release);
/// assert!(try_reset_shutdown());
/// assert!(!SHUTDOWN.load(Ordering::Acquire));
/// ```
pub fn try_reset_shutdown() -> bool {
    // AcqRel pairs with the Release store in the signal handler and Acquire
    // loads in [`shutdown_requested`]. The swap is intentional: we want to
    // observe-and-reset atomically so a concurrent signal does not slip
    // between the load and the store.
    SHUTDOWN.swap(false, Ordering::AcqRel) | {
        SIGNAL_COUNT.store(0, Ordering::Release);
        SIGNAL_NUMBER.store(0, Ordering::Release);
        // Suppress "unused" warning on the chained block; the `|` is just a
        // sequence point and the final expression is the swap result.
        false
    }
}

/// Returns `true` when audit/test mode is active and long-running subcommands
/// should ignore the cancellation token. The flag is honoured by the embedder
/// loop in [`crate::embedder`] and by every call site that consults
/// [`shutdown_requested`]. Production invocations always return `true` here.
///
/// The flag is read from the `SQLITE_GRAPHRAG_IGNORE_SHUTDOWN` environment
/// variable. Accepted values: `1`, `true`, `yes`, `on` (case-insensitive).
/// Anything else (including unset) means obey the cancellation token.
pub fn should_obey_shutdown() -> bool {
    !is_ignore_shutdown_set()
}

fn is_ignore_shutdown_set() -> bool {
    // PROC: read once per call; this is not on a hot path. Tests set the env
    // var in a `serial(env)` block so concurrent invocations cannot race.
    std::env::var("SQLITE_GRAPHRAG_IGNORE_SHUTDOWN")
        .ok()
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            v == "1" || v == "true" || v == "yes" || v == "on"
        })
        .unwrap_or(false)
}

/// Token-aware chunking utilities for bodies that exceed the embedding window.
pub mod chunking;

/// Hybrid entity extraction: regex pre-filter + GLiNER zero-shot NER (graceful degradation).
pub mod extraction;

/// v1.0.75 (G21 solution): extraction backend abstraction with
/// LLM/Embedding/None/Composite implementations.
pub mod extract;

/// `clap` definitions for the top-level `sqlite-graphrag` binary.
pub mod cli;

/// XDG-based API key management for OpenRouter and other providers.
pub mod config;

/// Subcommand handlers wired into the `clap` tree from [`cli`].
pub mod commands;

/// Compile-time constants: embedding dimensions, limits and thresholds.
pub mod constants;

/// Local embedding generation (LLM-only, one-shot per invocation).
pub mod embedder;

/// HTTP client for the OpenRouter chat-completions API (direct HTTP, no CLI subprocess).
pub mod chat_api;

/// HTTP client for the OpenRouter embeddings API (direct HTTP, no CLI subprocess).
pub mod embedding_api;

/// Canonical entity type taxonomy: 13 variants, ValueEnum + serde + rusqlite impls.
pub mod entity_type;

/// Library-wide error type and the mapping to process exit codes (see [`errors::AppError`]).
pub mod errors;

/// Graph traversal helpers over the entities and relationships tables.
pub mod graph;

/// Type aliases for AHash-backed collections in hot paths.
pub mod hash;

/// Bilingual message layer for human-facing stderr progress (`--lang en|pt`, `SQLITE_GRAPHRAG_LANG`).
pub mod i18n;

/// Counting semaphore via lock files to limit parallel invocations.
/// Provides `acquire_cli_slot` (counting semaphore) and the G28-B
/// per-namespace heavy-job singleton `acquire_job_singleton` for
/// `enrich`, `ingest --mode claude-code`, `ingest --mode codex`.
pub mod lock;

/// GAP-004 (v1.0.82): Cross-process slot semaphore for LLM subprocesses.
/// `acquire_llm_slot` limits concurrent `codex`/`claude` spawns per host
/// to prevent OAuth rate limit saturation when N+ sessions run in parallel.
pub mod llm_slots;

/// GAP-005 (v1.0.82): Exit code diagnostics for LLM subprocess crashes.
pub mod llm {
    pub mod exit_code_hints;
}

/// v1.0.75 (G22 solution): spawn subsystem abstraction with
/// `VersionAdapter` trait for codex/claude/opencode executors.
pub mod spawn;

/// Memory guard: checks RAM availability before loading the ONNX model.
pub mod memory_guard;

/// Type-safe enumeration of the five `memories.source` CHECK constraint values.
/// Replaces the footgun `pub source: String` to prevent G29-style regressions.
#[allow(rustdoc::broken_intra_doc_links)]
pub mod memory_source;

/// Namespace resolution with precedence between flag, environment and markers.
pub mod namespace;

/// Centralized stdout/stderr emitters for CLI output formatting.
pub mod output;

/// Dual-format argument parser: accepts Unix epoch and RFC 3339.
pub mod parsers;

/// G29 Passo 4: preservation checks (Jaccard trigram) for LLM-enriched bodies.
pub mod preservation;

/// Filesystem paths for the project-local database and app support directories.
pub mod paths;

/// SQLite pragma helpers applied on every connection.
pub mod pragmas;

/// v1.0.76: in-process vector similarity helpers. Replaces the
/// `sqlite-vec` KNN API with pure-Rust cosine over the BLOB-backed
/// `memory_embeddings` / `entity_embeddings` tables.
pub mod similarity;

/// Cross-platform signal handling: SIGINT, SIGTERM, SIGHUP.
pub mod signals;

/// Centralized retry infrastructure with exponential backoff and half-jitter.
pub mod retry;

/// G28: orphan-process reaper that runs at CLI startup.
#[allow(rustdoc::broken_intra_doc_links)]
pub mod reaper;

/// G28-D: system load average observation (pre-spawn saturation check).
pub mod system_load;

/// Persistence layer: memories, entities, chunks and version history.
pub mod storage;

/// Centralized tracing subscriber initialization with panic hook and log bridge.
pub mod telemetry;

/// Cross-platform terminal initialization: UTF-8 console, ANSI colors, NO_COLOR.
pub mod terminal;

/// Display time zone for `*_iso` fields (flag `--tz`, env `SQLITE_GRAPHRAG_DISPLAY_TZ`, fallback UTC).
pub mod tz;

/// Stdin reader with configurable timeout to prevent indefinite blocking.
pub mod stdin_helper;

/// Real tokenizer of the embedding model for accurate token counting and chunking.
pub mod tokenizer;

mod embedded_migrations {
    use refinery::embed_migrations;
    embed_migrations!("migrations");
}

pub use embedded_migrations::migrations;
