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

/// Token-aware chunking utilities for bodies that exceed the embedding window.
pub mod chunking;

/// Hybrid entity extraction: regex pre-filter + GLiNER zero-shot NER (graceful degradation).
pub mod extraction;

/// `clap` definitions for the top-level `sqlite-graphrag` binary.
pub mod cli;

/// Subcommand handlers wired into the `clap` tree from [`cli`].
pub mod commands;

/// Compile-time constants: embedding dimensions, limits and thresholds.
pub mod constants;

/// Daemon IPC for persistent embedding model reuse across CLI invocations.
pub mod daemon;

/// Local embedding generation backed by `fastembed`.
pub mod embedder;

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

/// Counting semaphore via lock files to limit parallel invocations (see [`lock::acquire_cli_slot`]).
pub mod lock;

/// Memory guard: checks RAM availability before loading the ONNX model.
pub mod memory_guard;

/// Namespace resolution with precedence between flag, environment and markers.
pub mod namespace;

/// Centralized stdout/stderr emitters for CLI output formatting.
pub mod output;

/// Dual-format argument parser: accepts Unix epoch and RFC 3339.
pub mod parsers;

/// Filesystem paths for the project-local database and app support directories.
pub mod paths;

/// SQLite pragma helpers applied on every connection.
pub mod pragmas;

/// Cross-platform signal handling: SIGINT, SIGTERM, SIGHUP.
pub mod signals;

/// Centralized retry infrastructure with exponential backoff and half-jitter.
pub mod retry;

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
