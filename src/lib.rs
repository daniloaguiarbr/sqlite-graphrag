//! # neurographrag
//!
//! Local GraphRAG memory for LLMs in a single SQLite file — zero external
//! services required.
//!
//! `neurographrag` is a CLI-first library that persists memories, entities and
//! typed relationships inside a single SQLite database. It combines FTS5
//! full-text search with `sqlite-vec` KNN over locally-generated embeddings to
//! expose a hybrid retrieval ranker tailored for LLM agents.
//!
//! ## CLI usage
//!
//! Install and initialize once, then save and recall memories:
//!
//! ```bash
//! cargo install neurographrag
//! neurographrag init
//! neurographrag remember \
//!     --name onboarding-note \
//!     --type user \
//!     --description "first memory" \
//!     --body "hello graphrag"
//! neurographrag recall "graphrag" --k 5
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

use std::sync::atomic::{AtomicBool, Ordering};

/// Sinaliza que um sinal de encerramento (SIGINT / SIGTERM / SIGHUP) foi recebido.
///
/// Definido em `main` via `ctrlc::set_handler`. Subcomandos de longa duração podem
/// consultar [`shutdown_requested`] para encerrar gracefully antes do timeout.
pub static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Retorna `true` se um sinal de encerramento foi recebido desde o início do processo.
pub fn shutdown_requested() -> bool {
    SHUTDOWN.load(Ordering::SeqCst)
}

/// Token-aware chunking utilities for bodies that exceed the embedding window.
pub mod chunking;

/// `clap` definitions for the top-level `neurographrag` binary.
pub mod cli;

/// Subcommand handlers wired into the `clap` tree from [`cli`].
pub mod commands;

/// Compile-time constants: embedding dimensions, limits and thresholds.
pub mod constants;

/// Local embedding generation backed by `fastembed`.
pub mod embedder;

/// Library-wide error type and the mapping to process exit codes (see [`errors::AppError`]).
pub mod errors;

/// Graph traversal helpers over the entities and relationships tables.
pub mod graph;

/// Semáforo de contagem via lock files para limitar invocações paralelas (veja [`lock::acquire_cli_slot`]).
pub mod lock;

/// Guarda de memória: verifica disponibilidade de RAM antes de carregar o modelo ONNX.
pub mod memory_guard;

/// Namespace resolution with precedence between flag, environment and markers.
pub mod namespace;

/// Centralized stdout/stderr emitters for CLI output formatting.
pub mod output;

/// XDG-aware filesystem paths for the database and cache directories.
pub mod paths;

/// SQLite pragma helpers applied on every connection.
pub mod pragmas;

/// Persistence layer: memories, entities, chunks and version history.
pub mod storage;

mod embedded_migrations {
    use refinery::embed_migrations;
    embed_migrations!("migrations");
}

pub use embedded_migrations::migrations;
