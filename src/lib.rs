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

use std::sync::atomic::{AtomicBool, Ordering};

/// Sinaliza que um sinal de encerramento (SIGINT / SIGTERM / SIGHUP) foi recebido.
///
/// Definido em `main` via `ctrlc::set_handler`. Subcomandos de longa duração podem
/// consultar [`shutdown_requested`] para encerrar gracefully antes do timeout.
pub static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Retorna `true` se um sinal de encerramento foi recebido desde o início do processo.
///
/// O valor reflete o estado de [`SHUTDOWN`]. Sem chamada a `ctrlc::set_handler`,
/// o estado inicial é sempre `false`.
///
/// # Examples
///
/// ```
/// use sqlite_graphrag::shutdown_requested;
///
/// // Em condições normais de inicialização o sinal não foi recebido.
/// assert!(!shutdown_requested());
/// ```
///
/// ```
/// use std::sync::atomic::Ordering;
/// use sqlite_graphrag::{SHUTDOWN, shutdown_requested};
///
/// // Simula recebimento de sinal e verifica que a função reflete o estado.
/// SHUTDOWN.store(true, Ordering::SeqCst);
/// assert!(shutdown_requested());
/// // Restaura para não contaminar outros testes.
/// SHUTDOWN.store(false, Ordering::SeqCst);
/// ```
pub fn shutdown_requested() -> bool {
    SHUTDOWN.load(Ordering::SeqCst)
}

/// Token-aware chunking utilities for bodies that exceed the embedding window.
pub mod chunking;

/// Hybrid entity extraction: regex pre-filter + candle BERT NER (graceful degradation).
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

/// Library-wide error type and the mapping to process exit codes (see [`errors::AppError`]).
pub mod errors;

/// Graph traversal helpers over the entities and relationships tables.
pub mod graph;

/// Bilingual message layer for human-facing stderr progress (`--lang en|pt`, `SQLITE_GRAPHRAG_LANG`).
pub mod i18n;

/// Semáforo de contagem via lock files para limitar invocações paralelas (veja [`lock::acquire_cli_slot`]).
pub mod lock;

/// Guarda de memória: verifica disponibilidade de RAM antes de carregar o modelo ONNX.
pub mod memory_guard;

/// Namespace resolution with precedence between flag, environment and markers.
pub mod namespace;

/// Centralized stdout/stderr emitters for CLI output formatting.
pub mod output;

/// Parser de argumentos dual-format: aceita Unix epoch e RFC 3339.
pub mod parsers;

/// Filesystem paths for the project-local database and app support directories.
pub mod paths;

/// SQLite pragma helpers applied on every connection.
pub mod pragmas;

/// Persistence layer: memories, entities, chunks and version history.
pub mod storage;

/// Fuso horário de exibição para campos `*_iso` (flag `--tz`, env `SQLITE_GRAPHRAG_DISPLAY_TZ`, fallback UTC).
pub mod tz;

/// Tokenizer real do modelo de embeddings para contagem e chunking por tokens reais.
pub mod tokenizer;

mod embedded_migrations {
    use refinery::embed_migrations;
    embed_migrations!("migrations");
}

pub use embedded_migrations::migrations;
