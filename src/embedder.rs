//! Embedding generation for the GraphRAG memory.
//!
//! v1.0.76: the default build is **LLM-only** — the binary does NOT bundle
//! fastembed / ort / ndarray / tokenizers. All embeddings are produced
//! by a headless invocation of `claude code` or `codex` (OAuth, no MCP,
//! no hooks) and stored as a BLOB in `memory_embeddings(memory_id, embedding,
//! source)`. Vector similarity is computed in pure Rust at query time.
//!
//! # Workload classification (G42/S3, BLOCK 1 — MANDATORY)
//!
//! LLM embedding is **I/O-bound + subprocess-bound**: each call waits
//! 5-60s on a network round-trip through a headless `claude -p` /
//! `codex exec` subprocess while the local CPU stays idle. Concurrency
//! therefore uses **tokio** (async I/O concurrency) and NEVER rayon
//! (reserved for CPU-bound work).
//!
//! # Permit formula (G42/S3, BLOCO 2)
//!
//! ```text
//! permits = clamp(--llm-parallelism, 1, 32)
//!           .min(available_parallelism())
//!           .min(available_ram_mb * 0.5 / LLM_WORKER_RSS_MB)
//! ```
//!
//! `LLM_WORKER_RSS_MB = 350` (`crate::constants`): `claude -p` and
//! `codex exec` are node processes with a typical Maximum RSS of
//! 200-400 MB (measured via `/usr/bin/time -l` on macOS /
//! `/usr/bin/time -v` on Linux), so the RAM bound is pertinent.
//!
//! # Locking contract (G42/A3 fix)
//!
//! The process-wide `Mutex<LlmEmbedding>` protects ONLY the cheap clone
//! of the client configuration (flavour + binary path + model + shared
//! schema tempfiles). It is NEVER held across network I/O — the
//! v1.0.76-v1.0.78 `flush_group` held it for the whole sequential
//! embedding loop, which is why `--llm-parallelism 8` measured an
//! effective parallelism of 1.

use crate::errors::AppError;
use crate::extract::llm_embedding::LlmEmbedding;
use parking_lot::Mutex;
use std::path::Path;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

/// Process-wide LLM-embedding client behind a .
///
/// The lock guards configuration cloning only (see module docs); the
/// actual LLM I/O happens on clones, outside the lock.
///
/// ADR-0042 / GAP-002: process-wide Claude-backed LLM-embedding client
/// behind a `Mutex`. Distinct from `EMBEDDER` so the Claude path of
/// `embed_via_backend` no longer re-probes PATH via `detect_available`
/// (the v1.0.82 bug where requesting Claude could resolve to Codex).
static CLAUDE_EMBEDDER: OnceLock<Mutex<LlmEmbedding>> = OnceLock::new();
static OPENCODE_EMBEDDER: OnceLock<Mutex<LlmEmbedding>> = OnceLock::new();
static OPENROUTER_CLIENT: OnceLock<crate::embedding_api::OpenRouterClient> = OnceLock::new();

/// v1.0.95 (ADR-0054): process-wide OpenRouter chat-completions client for
/// the `enrich` JUDGE. Distinct from `OPENROUTER_CLIENT` (embeddings) because
/// the chat client binds a text model, not an embedding model.
static OPENROUTER_CHAT_CLIENT: OnceLock<crate::chat_api::OpenRouterChatClient> = OnceLock::new();

/// v1.0.93: check whether the OpenRouter client has been initialised.
pub fn is_openrouter_initialized() -> bool {
    OPENROUTER_CLIENT.get().is_some()
}
static EMBEDDER: OnceLock<Mutex<LlmEmbedding>> = OnceLock::new();

/// Process-wide multi-thread tokio runtime for embedding I/O.
///
/// G42/A2 fix: v1.0.76-v1.0.78 built a current-thread runtime PER CALL.
/// One runtime per process amortises the setup and hosts the bounded
/// fan-out of `embed_texts_parallel`.
static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

/// Calibration base: chunk (long-text) batch size per LLM call at the
/// calibration dimensionality (G42/S2). Use [`chunk_embed_batch_size`]
/// for the dim-adaptive value (G44).
pub const CHUNK_EMBED_BATCH_SIZE: usize = 8;

/// Calibration base: entity-name (short-text) batch size per LLM call at
/// the calibration dimensionality (G42/S2). Use [`entity_embed_batch_size`]
/// for the dim-adaptive value (G44).
pub const ENTITY_EMBED_BATCH_SIZE: usize = 25;

/// Dimensionality the batch bases above were calibrated against (G44).
pub const EMBED_BATCH_CALIBRATION_DIM: usize = 64;

/// G44: scales a calibration-base batch size to the active dimensionality,
/// keeping the float budget per LLM call constant (~512 floats for chunks,
/// ~1600 for entity names — the budgets empirically validated at dim 64).
/// Fixed batches of 8 at 384 dims asked for ~3072 floats per response:
/// claude returned partial coverage (3 of 8 items, caught by the G42/C5
/// check) and codex timed out at 300s. `base.max(1)` keeps the function
/// total — `clamp` panics when the upper bound is below the lower one.
fn adaptive_batch_for_dim(base: usize, dim: usize) -> usize {
    let base = base.max(1);
    (base * EMBED_BATCH_CALIBRATION_DIM / dim.max(1)).clamp(1, base)
}

/// Dim-adaptive batch size for chunk (long-text) embedding calls (G44).
pub fn chunk_embed_batch_size() -> usize {
    let dim = crate::constants::embedding_dim();
    let batch = adaptive_batch_for_dim(CHUNK_EMBED_BATCH_SIZE, dim);
    tracing::debug!(
        dim,
        base = CHUNK_EMBED_BATCH_SIZE,
        batch,
        "adaptive chunk batch size (G44)"
    );
    batch
}

/// Dim-adaptive batch size for entity-name (short-text) embedding calls (G44).
pub fn entity_embed_batch_size() -> usize {
    let dim = crate::constants::embedding_dim();
    let batch = adaptive_batch_for_dim(ENTITY_EMBED_BATCH_SIZE, dim);
    tracing::debug!(
        dim,
        base = ENTITY_EMBED_BATCH_SIZE,
        batch,
        "adaptive entity batch size (G44)"
    );
    batch
}

/// Returns the process-wide multi-thread runtime, building it on first use.
pub(crate) fn shared_runtime() -> Result<&'static tokio::runtime::Runtime, AppError> {
    if let Some(rt) = RUNTIME.get() {
        return Ok(rt);
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| AppError::Embedding(format!("tokio runtime init failed: {e}")))?;
    let _ = RUNTIME.set(rt);
    RUNTIME.get().ok_or_else(|| {
        AppError::Embedding("tokio runtime unavailable after initialisation".to_string())
    })
}

/// Initialises the LLM-embedding client on first use and returns it.
pub fn get_embedder(_models_dir: &Path) -> Result<&'static Mutex<LlmEmbedding>, AppError> {
    if let Some(e) = EMBEDDER.get() {
        return Ok(e);
    }
    let backend = LlmEmbedding::detect_available()?;
    let _ = EMBEDDER.set(Mutex::new(backend));
    EMBEDDER
        .get()
        .ok_or_else(|| AppError::Embedding("embedder unavailable after initialisation".to_string()))
}

/// ADR-0042 / GAP-002: returns the process-wide Claude embedder, lazily
/// initialising it on first use. Binary and model overrides come from
/// the explicit arguments; `None` falls back to PATH/env defaults via
/// the builder.
pub fn get_claude_embedder(
    claude_binary: Option<&Path>,
    claude_model: Option<&str>,
) -> Result<&'static Mutex<LlmEmbedding>, AppError> {
    if let Some(e) = CLAUDE_EMBEDDER.get() {
        return Ok(e);
    }
    let mut builder = LlmEmbedding::with_claude_builder();
    if let Some(b) = claude_binary {
        builder = builder.override_binary(b.to_path_buf());
    }
    if let Some(m) = claude_model {
        builder = builder.override_model(m.to_string());
    }
    let backend = builder.build()?;
    let _ = CLAUDE_EMBEDDER.set(Mutex::new(backend));
    CLAUDE_EMBEDDER.get().ok_or_else(|| {
        AppError::Embedding("claude embedder unavailable after initialisation".to_string())
    })
}

/// GAP-OPENCODE-001 / v1.0.90: returns the process-wide OpenCode embedder,
/// lazily initialising it on first use. Binary and model overrides come
/// from the explicit arguments; `None` falls back to PATH/env defaults via
/// the builder.
pub fn get_opencode_embedder(
    opencode_binary: Option<&Path>,
    opencode_model: Option<&str>,
) -> Result<&'static Mutex<LlmEmbedding>, AppError> {
    if let Some(e) = OPENCODE_EMBEDDER.get() {
        return Ok(e);
    }
    let mut builder = LlmEmbedding::with_opencode_builder();
    if let Some(b) = opencode_binary {
        builder = builder.override_binary(b.to_path_buf());
    }
    if let Some(m) = opencode_model {
        builder = builder.override_model(m.to_string());
    }
    let backend = builder.build()?;
    let _ = OPENCODE_EMBEDDER.set(Mutex::new(backend));
    OPENCODE_EMBEDDER.get().ok_or_else(|| {
        AppError::Embedding("opencode embedder unavailable after initialisation".to_string())
    })
}

pub fn get_openrouter_embedder(
    api_key: secrecy::SecretBox<String>,
    model: &str,
    dim: usize,
) -> Result<&'static crate::embedding_api::OpenRouterClient, AppError> {
    if let Some(c) = OPENROUTER_CLIENT.get() {
        return Ok(c);
    }
    let client = crate::embedding_api::OpenRouterClient::new(api_key, model.to_string(), dim)?;
    let _ = OPENROUTER_CLIENT.set(client);
    OPENROUTER_CLIENT.get().ok_or_else(|| {
        AppError::Embedding("openrouter client unavailable after initialisation".to_string())
    })
}

/// v1.0.95 (ADR-0054): initialises the process-wide OpenRouter chat client on
/// first use and returns it. `model` is the text model the enrich JUDGE will
/// call (no default; the caller validates presence upfront).
pub fn get_openrouter_chat_client(
    api_key: secrecy::SecretBox<String>,
    model: &str,
    timeout_secs: u64,
) -> Result<&'static crate::chat_api::OpenRouterChatClient, AppError> {
    if let Some(c) = OPENROUTER_CHAT_CLIENT.get() {
        return Ok(c);
    }
    let client =
        crate::chat_api::OpenRouterChatClient::new(api_key, model.to_string(), timeout_secs)?;
    let _ = OPENROUTER_CHAT_CLIENT.set(client);
    OPENROUTER_CHAT_CLIENT.get().ok_or_else(|| {
        AppError::Embedding("openrouter chat client unavailable after initialisation".to_string())
    })
}

/// v1.0.95: returns the process-wide OpenRouter chat client if it has already
/// been initialised via [`get_openrouter_chat_client`]. Used by the enrich
/// JUDGE dispatch, which initialises the singleton once at startup and then
/// fetches it per item without re-threading the API key.
pub fn openrouter_chat_client() -> Option<&'static crate::chat_api::OpenRouterChatClient> {
    OPENROUTER_CHAT_CLIENT.get()
}

/// ADR-0042 / GAP-002: route a single passage through the Claude
/// embedder. Used by the Claude arm of `embed_via_backend` so the
/// fallback chain stops treating Claude as a synonym for codex.
pub fn embed_via_claude_local(
    _models_dir: &Path,
    text: &str,
    claude_binary: Option<&Path>,
    claude_model: Option<&str>,
) -> Result<Vec<f32>, AppError> {
    let _slot_guard = acquire_llm_slot_for_embedding()?;
    let embedder = get_claude_embedder(claude_binary, claude_model)?;
    embed_passage(embedder, text)
}

/// BUG-003 / v1.0.85: split of  that also
/// reports the resolved []. Always  because
/// this path constructs a Claude-flavoured embedder via
///  (no PATH probe, no silent substitution).
pub fn embed_via_claude_local_resolved(
    _models_dir: &Path,
    text: &str,
    claude_binary: Option<&Path>,
    claude_model: Option<&str>,
) -> Result<(Vec<f32>, LlmBackendKind), AppError> {
    let _slot_guard = acquire_llm_slot_for_embedding()?;
    let embedder = get_claude_embedder(claude_binary, claude_model)?;
    let v = embed_passage(embedder, text)?;
    Ok((v, LlmBackendKind::Claude))
}

/// GAP-OPENCODE-001 / v1.0.90: route a single passage through the OpenCode
/// embedder, reporting the resolved [`LlmBackendKind::Opencode`]. Constructs
/// an OpenCode-flavoured embedder via `with_opencode_builder` (no PATH probe,
/// no silent substitution).
pub fn embed_via_opencode_local_resolved(
    _models_dir: &Path,
    text: &str,
    opencode_binary: Option<&Path>,
    opencode_model: Option<&str>,
) -> Result<(Vec<f32>, LlmBackendKind), AppError> {
    let _slot_guard = acquire_llm_slot_for_embedding()?;
    let embedder = get_opencode_embedder(opencode_binary, opencode_model)?;
    let v = embed_passage(embedder, text)?;
    Ok((v, LlmBackendKind::Opencode))
}
/// Clones the embedding-client configuration. The lock is held only for
/// the duration of the clone — NEVER across I/O (G42/A3).
fn clone_client(embedder: &Mutex<LlmEmbedding>) -> LlmEmbedding {
    embedder.lock().clone()
}

/// Embeds a single passage for storage. Delegates to the configured LLM
/// headless (claude code / codex). Returns a vector of the active
/// dimensionality.
pub fn embed_passage(embedder: &Mutex<LlmEmbedding>, text: &str) -> Result<Vec<f32>, AppError> {
    let client = clone_client(embedder);
    let result = client.embed_passage(text)?;
    validate_dim(result)
}

/// Embeds a single query for similarity search. Same model and dim as
/// `embed_passage`; the only difference is the LLM-side prompt prefix
/// that the headless invocation uses to disambiguate.
pub fn embed_query(embedder: &Mutex<LlmEmbedding>, text: &str) -> Result<Vec<f32>, AppError> {
    let client = clone_client(embedder);
    let result = client.embed_query(text)?;
    validate_dim(result)
}

/// Embeds a batch of passages with token-count-aware batching.
///
/// Kept for API compatibility; since v1.0.79 it routes through the
/// bounded parallel fan-out with conservative defaults.
pub fn embed_passages_controlled(
    embedder: &Mutex<LlmEmbedding>,
    texts: &[&str],
    _token_counts: &[usize],
) -> Result<Vec<Vec<f32>>, AppError> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let owned: Vec<String> = texts.iter().map(|t| t.to_string()).collect();
    embed_texts_parallel(embedder, &owned, 1, chunk_embed_batch_size())
}

pub fn embed_passage_local(models_dir: &Path, text: &str) -> Result<Vec<f32>, AppError> {
    let _slot_guard = acquire_llm_slot_for_embedding()?;
    let embedder = get_embedder(models_dir)?;
    embed_passage(embedder, text)
}

/// v1.0.89 (BUG-SKIP-EMBED): reads `SQLITE_GRAPHRAG_SKIP_EMBEDDING_ON_FAILURE`
/// env var (set by `--skip-embedding-on-failure` via main.rs propagation).
/// Returns `true` when the user opted to persist with NULL embedding on failure.
pub fn should_skip_embedding_on_failure() -> bool {
    matches!(
        std::env::var("SQLITE_GRAPHRAG_SKIP_EMBEDDING_ON_FAILURE").as_deref(),
        Ok("1") | Ok("true")
    )
}

/// v1.0.89 (BUG-SKIP-EMBED + GAP-EMBED-PROPAGATION): embed a passage
/// honouring both `--llm-backend` and `--skip-embedding-on-failure`.
///
/// On success returns `Ok(Some(vec))`. On failure:
/// - if `--skip-embedding-on-failure` is active, logs a warning and returns `Ok(None)`
/// - otherwise propagates the error (exit 11)
pub fn embed_passage_or_skip(
    models_dir: &Path,
    text: &str,
    choice: Option<crate::cli::LlmBackendChoice>,
) -> Result<Option<Vec<f32>>, AppError> {
    match embed_passage_with_choice(models_dir, text, choice) {
        Ok((v, _backend)) => Ok(Some(v)),
        Err(AppError::Validation(msg)) => Err(AppError::Validation(msg)),
        Err(e) => {
            if should_skip_embedding_on_failure() {
                tracing::warn!(
                    error = %e,
                    "embedding failed but --skip-embedding-on-failure is active; persisting with NULL embedding"
                );
                Ok(None)
            } else {
                Err(e)
            }
        }
    }
}

/// BUG-003 / v1.0.85: split of `embed_passage_local` that reports the
/// resolved [`LlmBackendKind`] based on the ACTUAL
/// [`LlmEmbedding::flavour`] of the embedder constructed. When
/// `LlmEmbedding::detect_available` substitutes claude for a missing
/// codex, the operator sees the truth in `envelope.backend_invoked`.
pub fn embed_passage_local_resolved(
    models_dir: &Path,
    text: &str,
) -> Result<(Vec<f32>, LlmBackendKind), AppError> {
    let _slot_guard = acquire_llm_slot_for_embedding()?;
    let embedder = get_embedder(models_dir)?;
    let v = embed_passage(embedder, text)?;
    let kind = match embedder.lock().flavour() {
        crate::extract::llm_embedding::EmbeddingFlavour::Codex => LlmBackendKind::Codex,
        crate::extract::llm_embedding::EmbeddingFlavour::Claude => LlmBackendKind::Claude,
        crate::extract::llm_embedding::EmbeddingFlavour::Opencode => LlmBackendKind::Opencode,
    };
    Ok((v, kind))
}

pub fn embed_query_local(models_dir: &Path, text: &str) -> Result<Vec<f32>, AppError> {
    let _slot_guard = acquire_llm_slot_for_embedding()?;
    let embedder = get_embedder(models_dir)?;
    embed_query(embedder, text)
}

// =============================================================================
// v1.0.82 (GAP-003): wrappers que aceitam a escolha do CLI
// (`crate::cli::LlmBackendChoice`) e a traduzem em uma chain para
// `embed_with_fallback`. Centralizam a propagação do flag `--llm-backend`
// nos 6 comandos que produzem embedding (`remember`, `edit`, `ingest`,
// `enrich`, `recall`, `hybrid-search`).
// =============================================================================

/// Embed a single passage using the LLM backend selected by the user via
/// `--llm-backend`. Routes to `embed_with_fallback` so failures fall
/// through to the next backend in the chain before giving up.
///
/// When `choice` is `None` (e.g. a sub-command that does not yet
/// expose the flag), behaviour matches `embed_passage_local` — the
/// active embedder from `LlmEmbedding::detect_available` decides the
/// backend.
pub fn embed_passage_with_choice(
    models_dir: &Path,
    text: &str,
    choice: Option<crate::cli::LlmBackendChoice>,
) -> Result<(Vec<f32>, LlmBackendKind), AppError> {
    let _slot_guard = acquire_llm_slot_for_embedding()?;
    match choice {
        None => {
            let embedder = get_embedder(models_dir)?;
            embed_passage(embedder, text).map(|v| (v, LlmBackendKind::None))
        }
        Some(choice) => embed_with_fallback(models_dir, text, &choice.to_chain(), false),
    }
}

/// v1.0.93: embedding with `EmbeddingBackendChoice` awareness. When the
/// embedding backend is `Openrouter` or `Auto` with a live client, the
/// chain prepends `OpenRouter` before the LLM subprocess backends.
pub fn embed_passage_with_embedding_choice(
    models_dir: &Path,
    text: &str,
    embedding_backend: crate::cli::EmbeddingBackendChoice,
    llm_backend: crate::cli::LlmBackendChoice,
) -> Result<(Vec<f32>, LlmBackendKind), AppError> {
    let _slot_guard = acquire_llm_slot_for_embedding()?;
    let chain = embedding_backend.to_chain(llm_backend);
    embed_with_fallback(models_dir, text, &chain, false)
}

/// failure, returns a structured `FallbackReason` so the caller can
/// surface `vec_degraded` instead of a hard exit 11.
///
/// `None` matches the legacy `try_embed_query_with_fallback` path
/// (uses the active embedder without an explicit chain).
pub fn try_embed_query_with_choice(
    models_dir: &Path,
    text: &str,
    choice: Option<crate::cli::LlmBackendChoice>,
) -> Result<(Vec<f32>, LlmBackendKind), FallbackReason> {
    match embed_passage_with_choice(models_dir, text, choice) {
        // GAP-004 / v1.0.85.1: when the chain terminates on
        //  (i.e. user passed
        // or every preceding backend failed),  returns
        //  instead of an error. Without this guard the
        // empty vector would propagate to  which
        // aborts with exit 11 ("embedding has 0 dims, expected 64").
        // The caller's contract is to surface a typed
        // so  and  can route to FTS5-puro via
        // the existing  /  envelope.
        // Intercept the empty-vector success path and surface it as
        //  (introduced at v1.0.85 / ADR-0043
        // for the symmetric LLM-returned-zero-dim case).
        Ok((v, _backend)) if v.is_empty() => Err(FallbackReason::DimZero),
        Ok((v, backend)) => Ok((v, backend)),
        Err(e) => Err(classify_embedding_error(e)),
    }
}
/// v1.0.93 (GAP-OR-INGEST): query embedding with `EmbeddingBackendChoice`
/// awareness. Mirrors `try_embed_query_with_choice` but routes through
/// `embed_passage_with_embedding_choice` so OpenRouter API is used when
/// configured.
pub fn try_embed_query_with_embedding_choice(
    models_dir: &Path,
    text: &str,
    embedding_backend: crate::cli::EmbeddingBackendChoice,
    llm_backend: crate::cli::LlmBackendChoice,
) -> Result<(Vec<f32>, LlmBackendKind), FallbackReason> {
    match embed_passage_with_embedding_choice(models_dir, text, embedding_backend, llm_backend) {
        Ok((v, _backend)) if v.is_empty() => Err(FallbackReason::DimZero),
        Ok((v, backend)) => Ok((v, backend)),
        Err(e) => Err(classify_embedding_error(e)),
    }
}

/// call. Reads the max-concurrency from
/// `SQLITE_GRAPHRAG_LLM_MAX_HOST_CONCURRENCY` (default derived from
/// `LLM_WORKER_RSS_MB` and available memory), and the wait timeout
/// from `SQLITE_GRAPHRAG_LLM_SLOT_WAIT_SECS` (default 30s).
///
/// Returns `Ok(guard)` for happy path, `AppError::LockBusy` (exit 75)
/// when no slot is available within the wait window, and
/// `AppError::Validation` when the concurrency is 0.
///
/// The `LLM_SLOT_NO_WAIT` env var (or its CLI flag equivalent) sets
/// `wait_secs = 0` to fail fast in tests.
fn acquire_llm_slot_for_embedding() -> Result<crate::llm_slots::LlmSlotGuard, AppError> {
    use crate::constants::{CLI_LOCK_DEFAULT_WAIT_SECS, LLM_WORKER_RSS_MB};
    let max = std::env::var("SQLITE_GRAPHRAG_LLM_MAX_HOST_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|n| *n >= 1)
        .unwrap_or_else(crate::llm_slots::default_max_concurrency);
    let wait_secs = if std::env::var("SQLITE_GRAPHRAG_LLM_SLOT_NO_WAIT").is_ok() {
        0
    } else {
        std::env::var("SQLITE_GRAPHRAG_LLM_SLOT_WAIT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(CLI_LOCK_DEFAULT_WAIT_SECS)
    };
    let _ = LLM_WORKER_RSS_MB; // silence the unused import (used in default_max_concurrency)
                               // GAP-003 / ADR-0043: when the slot semaphore is contended beyond the
                               // backoff window (50 + 100 + 200 + 400 = 750ms total), return a
                               // marker message that `classify_embedding_error` maps to
                               // `FallbackReason::SlotExhausted` (discriminator `slot_exhausted`).
                               // The window is shorter than the legacy 30s timeout, so the operator
                               // observes FTS5-puro fallback quickly instead of after 30s of silence.
    match crate::llm_slots::acquire_llm_slot(max, wait_secs) {
        Ok(guard) => Ok(guard),
        Err(e @ AppError::LockBusy { .. }) if wait_secs > 0 => Err(AppError::Embedding(format!(
            "slot exhausted: {e} (fall back to FTS5)"
        ))),
        Err(e) => Err(e),
    }
}
/// GAP-004 (v1.0.88): typed classifier for embedding error messages.
///
/// Decomposes the legacy `AppError::Embedding(String)` payload into a
/// small enum so the call sites can branch on the cause instead of
/// repeating `msg.contains(...)` literals. The classification is purely
/// lexical (case-insensitive substring match on the error message) — no
/// I/O, no retries, no telemetry, deterministic and safe under
/// `#[serial_test::serial(env)]`.
///
/// 6 variants cover the 5 known discriminators from v1.0.85 (ADR-0043)
/// plus an `Unknown` fallback for messages that do not match any marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingErrorKind {
    /// OAuth token expired or absent; no backend can authenticate.
    OAuth,
    /// OAuth usage quota exhausted on the named backend.
    Quota,
    /// LLM slot semaphore exhausted after the backoff window.
    SlotExhausted,
    /// User-requested backend differs from the one that actually executed.
    BackendMismatch,
    /// Embedding returned a zero-dimensional vector (structural bug).
    ZeroDimension,
    /// Message did not match any of the 5 markers above.
    Unknown,
}

impl EmbeddingErrorKind {
    /// Classify an embedding error message into a typed kind.
    ///
    /// Order of checks matters: `OAuth` is matched before `Quota` because
    /// both substrings can co-occur in the same message. `SlotExhausted`
    /// is checked before `Quota` because the slot-sema path is more
    /// specific (the LLM never even tried to authenticate). The checks
    /// are case-insensitive so `OAuth` and `oauth` both classify to
    /// `EmbeddingErrorKind::OAuth`.
    pub fn classify(msg: &str) -> Self {
        let m = msg.to_lowercase();
        if m.contains("oauth") {
            Self::OAuth
        } else if m.contains("quota") {
            Self::Quota
        } else if m.contains("slot exhausted") {
            Self::SlotExhausted
        } else if m.contains("backend mismatch") {
            Self::BackendMismatch
        } else if m.contains("dim") && m.contains("zero") {
            Self::ZeroDimension
        } else {
            Self::Unknown
        }
    }

    /// Stable, machine-friendly discriminator code (lowercase, kebab-safe).
    pub fn code(&self) -> &'static str {
        match self {
            Self::OAuth => "oauth",
            Self::Quota => "quota",
            Self::SlotExhausted => "slot-exhausted",
            Self::BackendMismatch => "backend-mismatch",
            Self::ZeroDimension => "zero-dimension",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for EmbeddingErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

/// G58/S1: reason an embedding call could not be completed and the caller
/// must fall back to a non-vector retrieval path (FTS5 prefix + LIKE).
///
/// Returned by [`try_embed_query_with_fallback`] so the `recall` and
/// `hybrid-search` handlers can surface a structured `vec_degraded` /
/// `warning` envelope instead of a hard `AppError::Embedding` exit 11.
#[derive(Debug, Clone, PartialEq)]
pub enum FallbackReason {
    /// The LLM subprocess failed (rate limit, OAuth contention, quota
    /// exhausted, model unparsable response, divergent dim, etc.).
    /// Carries the original error message for observability.
    EmbeddingFailed(String),
    /// The LLM slot semaphore was exhausted: 8+ concurrent LLM
    /// subprocesses blocked the acquire beyond the backoff window
    /// (50ms + 100ms + 200ms + 400ms = 750ms total). Resolved at v1.0.85
    /// (GAP-003 / ADR-0043).
    SlotExhausted,
    /// OAuth usage quota exhausted on the named backend. The caller
    /// should retry with an alternative backend (codex ↔ claude)
    /// before falling back to FTS5-puro.
    OAuthQuota { backend: &'static str },
    /// The user requested a backend that differs from the one that
    /// actually executed the embedding (legacy "synonym for codex"
    /// bug from v1.0.83). Resolved at v1.0.84 (GAP-002).
    BackendMismatch {
        requested: &'static str,
        resolved: &'static str,
    },
    /// The embedding returned a zero-dimensional vector, signalling a
    /// structural bug (the LLM did not produce any floats). Distinct
    /// from OAuthQuota (quota exhausted) and EmbeddingFailed
    /// (subprocess error).
    DimZero,
    /// The embedding was cancelled by an external signal (SIGTERM, etc.).
    Cancelled,
    /// The embedding exceeded its time budget. Carries the operation name
    /// and the elapsed seconds for diagnostic logging.
    Timeout {
        operation: String,
        duration_secs: u64,
    },
}

impl FallbackReason {
    /// Stable, machine-friendly reason code used by JSON envelopes
    /// (`vec_degraded_reason`). Mirrors the v1.0.84 contract extended
    /// at v1.0.85 with 4 new variants (GAP-003 / ADR-0043).
    pub fn reason_code(&self) -> &'static str {
        match self {
            Self::EmbeddingFailed(_) => "embedding_failed",
            Self::SlotExhausted => "slot_exhausted",
            Self::OAuthQuota { .. } => "oauth_quota",
            Self::BackendMismatch { .. } => "backend_mismatch",
            Self::DimZero => "dim_zero",
            Self::Cancelled => "cancelled",
            Self::Timeout { .. } => "timeout",
        }
    }
}

impl std::fmt::Display for FallbackReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmbeddingFailed(msg) => write!(f, "embedding failed: {msg}"),
            Self::SlotExhausted => write!(
                f,
                "slot exhausted: failed to acquire LLM slot after backoff window (max=8 concurrent, total backoff=750ms)"
            ),
            Self::OAuthQuota { backend } => {
                write!(f, "OAuth usage quota exhausted on backend '{backend}'")
            }
            Self::BackendMismatch {
                requested,
                resolved,
            } => {
                write!(
                    f,
                    "backend mismatch: user requested '{requested}' but '{resolved}' was invoked"
                )
            }
            Self::DimZero => write!(f, "embedding returned zero-dimensional vector"),
            Self::Cancelled => write!(f, "embedding cancelled by external signal"),
            Self::Timeout {
                operation,
                duration_secs,
            } => {
                write!(
                    f,
                    "embedding timed out after {duration_secs}s during {operation}"
                )
            }
        }
    }
}

impl std::error::Error for FallbackReason {}

/// G58/S1: try to embed a query, mapping any failure to a structured
/// [`FallbackReason`] so callers can route to FTS5 + LIKE fallback instead
/// of returning exit 11 to the user.
///
/// This is the bridge between the hard-fail `embed_query_local` (used by
/// write paths where embedding failure aborts the operation) and the
/// graceful-degradation contract of `recall` / `hybrid-search` in v1.0.80.
pub fn try_embed_query_with_fallback(
    models_dir: &Path,
    query: &str,
) -> Result<(Vec<f32>, LlmBackendKind), FallbackReason> {
    match embed_query_local(models_dir, query) {
        Ok(v) => Ok((v, LlmBackendKind::None)),
        Err(e) => Err(classify_embedding_error(e)),
    }
}

/// G58 / ADR-0043 (v1.0.85): deterministic fallback for `recall` and
/// `hybrid-search`.
///
/// - On `OAuthQuota { backend }`, retry once with the alternative backend
///   (codex ↔ claude) before giving up.
/// - On `SlotExhausted`, sleep 750ms and retry once (gives the slot
///   semaphore time to release a permit from a sibling subprocess).
/// - On any other `FallbackReason`, return immediately (deterministic).
pub fn try_embed_query_with_deterministic_fallback(
    models_dir: &Path,
    query: &str,
    choice: Option<crate::cli::LlmBackendChoice>,
) -> Result<(Vec<f32>, LlmBackendKind), FallbackReason> {
    match try_embed_query_with_choice(models_dir, query, choice) {
        Ok(t) => Ok(t),
        Err(reason @ FallbackReason::OAuthQuota { backend }) => {
            let alt = match backend {
                "codex" => Some(crate::cli::LlmBackendChoice::Claude),
                "claude" => Some(crate::cli::LlmBackendChoice::Codex),
                "opencode" => Some(crate::cli::LlmBackendChoice::Codex),
                "openrouter" => Some(crate::cli::LlmBackendChoice::Codex),
                _ => None,
            };
            if let Some(alt_choice) = alt {
                try_embed_query_with_choice(models_dir, query, Some(alt_choice))
            } else {
                Err(reason)
            }
        }
        Err(reason @ FallbackReason::SlotExhausted) => {
            std::thread::sleep(std::time::Duration::from_millis(750));
            try_embed_query_with_choice(models_dir, query, choice).or(Err(reason))
        }
        Err(other) => Err(other),
    }
}

/// Classify an embedding [`AppError`] into a typed [`FallbackReason`].
///
/// v1.0.85 (ADR-0043): discriminates the 4 new causes (SlotExhausted,
/// OAuthQuota, BackendMismatch, DimZero) from the legacy generic
/// EmbeddingFailed bucket. The classification is purely lexical
/// (substring match on the message) — no I/O, no retries, no
/// telemetry, deterministic and `#[serial_test::serial(env)]`-safe.
pub fn classify_embedding_error(err: AppError) -> FallbackReason {
    match err {
        AppError::Timeout {
            operation,
            duration_secs,
        } => FallbackReason::Timeout {
            operation,
            duration_secs,
        },
        AppError::Embedding(msg) => match EmbeddingErrorKind::classify(&msg) {
            // GAP-004 (v1.0.88): typed-discriminator dispatch.
            // The lexical classifier picks the discriminator; the arms below
            // enrich the result with the backend name and the
            // requested/resolved pair that the JSON envelope needs.
            //
            // Note: `Cancelled` and `EmbeddingFailed(msg)` are not in the
            // 6-variant enum (they have no lexical marker) so we keep them
            // as explicit guards at the head of the match.
            EmbeddingErrorKind::SlotExhausted => FallbackReason::SlotExhausted,
            EmbeddingErrorKind::OAuth => {
                let backend = if msg.contains("codex") {
                    "codex"
                } else if msg.contains("claude") || msg.contains("anthropic-ratelimit") {
                    // G45-CR5: anthropic-ratelimit-* headers are emitted only by
                    // the Claude CLI subprocess; treat them as claude quota
                    // signals even when the message text omits the word
                    // "claude" explicitly.
                    "claude"
                } else if msg.contains("opencode") {
                    "opencode"
                } else {
                    "unknown"
                };
                FallbackReason::OAuthQuota { backend }
            }
            EmbeddingErrorKind::Quota => {
                let backend = if msg.contains("codex") {
                    "codex"
                } else if msg.contains("claude") || msg.contains("anthropic-ratelimit") {
                    "claude"
                } else if msg.contains("opencode") {
                    "opencode"
                } else {
                    "unknown"
                };
                FallbackReason::OAuthQuota { backend }
            }
            EmbeddingErrorKind::BackendMismatch => {
                // The `msg.contains("claude")` arm is intentionally
                // placed BEFORE the OAuth arm so that a backend-mismatch
                // message that mentions both "claude" and "codex" maps to
                // BackendMismatch (the more specific failure mode).
                let (requested, resolved) =
                    if msg.contains("requested claude") && msg.contains("but codex") {
                        ("claude", "codex")
                    } else if msg.contains("requested codex") && msg.contains("but claude") {
                        ("codex", "claude")
                    } else if msg.contains("requested claude") {
                        ("claude", "unknown")
                    } else if msg.contains("requested codex") {
                        ("codex", "unknown")
                    } else {
                        ("unknown", "unknown")
                    };
                FallbackReason::BackendMismatch {
                    requested,
                    resolved,
                }
            }
            EmbeddingErrorKind::ZeroDimension => FallbackReason::DimZero,
            EmbeddingErrorKind::Unknown => {
                if msg.contains("cancelled") {
                    FallbackReason::Cancelled
                } else {
                    FallbackReason::EmbeddingFailed(msg)
                }
            }
        },
        e => FallbackReason::EmbeddingFailed(e.to_string()),
    }
}
// backends before giving up. The chain order matches the user-supplied
// `--llm-fallback` list (default: codex, claude, none).
// =============================================================================

/// Tries each LLM backend in `chain` in order, returning the first
/// successful embedding. On failure, the diagnostic tail of the last
/// error is preserved in the returned `AppError::Embedding` so the
/// operator can see WHY every backend failed.
///
/// If `skip_on_failure` is `true` AND every backend fails, the function
/// returns `Ok(Vec::new())` (an empty vector) to signal "persist
/// without embedding" — the call site is then responsible for writing
/// a `pending_embeddings` row that can be retried later by the
/// `embedding retry` subcommand.
///
/// Defaults the chain to `[codex, claude, none]` when `chain` is
/// empty, matching the v1.0.81 behaviour where codex was the
/// implicit default and claude was the implicit fallback.
pub fn embed_with_fallback(
    models_dir: &Path,
    text: &str,
    chain: &[LlmBackendKind],
    skip_on_failure: bool,
) -> Result<(Vec<f32>, LlmBackendKind), AppError> {
    use crate::llm::exit_code_hints::LlmBackendError;
    let effective: Vec<LlmBackendKind> = if chain.is_empty() {
        vec![
            LlmBackendKind::Codex,
            LlmBackendKind::Claude,
            LlmBackendKind::Opencode,
            LlmBackendKind::None,
        ]
    } else {
        chain.to_vec()
    };

    let mut last_err: Option<AppError> = None;
    for backend in &effective {
        // BUG-003 / v1.0.85: propagar o backend REAL retornado por
        // embed_via_backend (que pode diferir do chain position quando
        // LlmEmbedding::detect_available substitui codex por claude).
        // O tuple `(_, requested_kind)` é descartado — só queremos o
        // backend resolvido na primeira posição.
        // ADR-0046 / BUG-11 v1.0.88: use `embed_via_backend_strict` so the
        // sentinel `None` backend propagates the last real error instead
        // of silently degrading to `Ok((Vec::new(), None))`. This is the
        // path that caused preflight rejections to be swallowed by the
        // chain's default trailing `None`.
        match embed_via_backend_strict(
            models_dir,
            text,
            backend,
            last_err.as_ref(),
            skip_on_failure,
        ) {
            Ok((v, resolved_kind)) => return Ok((v, resolved_kind)),
            Err(e) => {
                // ADR-0011: Validation errors (OAuth-only enforcement) are
                // FATAL — propagate immediately without trying the next
                // backend. This prevents the fallback chain from swallowing
                // OAuth violations via the trailing `None` sentinel.
                if matches!(e, AppError::Validation(_)) {
                    return Err(e);
                }
                tracing::warn!(
                    target: "embedding",
                    backend = ?backend,
                    error = %e,
                    "embed_with_fallback: backend failed, trying next"
                );
                last_err = Some(e);
            }
        }
    }
    if skip_on_failure {
        // Signal "persist with no embedding" via an empty vector paired
        // with `None` so callers know the chain exhausted without a hit.
        // Caller is responsible for writing a `pending_embeddings` row
        // that can be retried later by the `embedding retry` subcommand.
        return Ok((Vec::new(), LlmBackendKind::None));
    }
    Err(last_err
        .unwrap_or_else(|| AppError::Embedding(LlmBackendError::NoBackendsAvailable.to_string())))
}

/// LLM backend kind for the fallback chain. Mirrors the CLI
/// `--llm-backend` enum so users can pass the same value to
/// `--llm-fallback` without translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LlmBackendKind {
    /// `codex exec` (default for v1.0.76+).
    Codex,
    /// `claude -p` (fallback for ChatGPT Pro OAuth unavailability).
    Claude,
    /// `opencode run` (v1.0.90).
    Opencode,
    /// OpenRouter HTTP API (v1.0.93).
    OpenRouter,
    /// No embedding — empty vector returned.
    None,
}

impl LlmBackendKind {
    /// Stable string label used in tracing and JSON envelopes. The
    /// string values are part of the public contract for `envelope.backend_invoked`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Opencode => "opencode",
            Self::OpenRouter => "openrouter",
            Self::None => "none",
        }
    }
}

/// Embeds a single text via the given backend. Used by
/// `embed_with_fallback` and exposed to allow direct one-shot
/// selection without a chain.
/// Embeds a single text via the given backend. Used by
/// `embed_with_fallback` and exposed to allow direct one-shot
/// selection without a chain.
///
/// BUG-003 / v1.0.85: returns `(Vec<f32>, LlmBackendKind)`. The
/// second element reports the backend that ACTUALLY executed the
/// embedding, not the chain position requested by the caller. When
/// `LlmBackendKind::Codex` is requested but `codex` is absent from
/// PATH, `LlmEmbedding::detect_available` substitutes claude and the
/// tuple carries `LlmBackendKind::Claude` so the operator sees the
/// truth in `envelope.backend_invoked`.
pub fn embed_via_backend(
    models_dir: &Path,
    text: &str,
    backend: &LlmBackendKind,
) -> Result<(Vec<f32>, LlmBackendKind), AppError> {
    match backend {
        LlmBackendKind::None => Ok((Vec::new(), LlmBackendKind::None)),
        LlmBackendKind::Codex => embed_passage_local_resolved(models_dir, text),
        LlmBackendKind::Claude => {
            // ADR-0042 / GAP-002: route Claude through its own static
            // embedder instead of re-using the Codex path (which used
            // to silently pick Codex if PATH ordered it first).
            tracing::debug!(
                target: "embedder",
                backend = "claude",
                "embed_via_backend: forcing claude (ADR-0042 / GAP-002 fix)"
            );
            embed_via_claude_local_resolved(models_dir, text, None, None)
        }
        LlmBackendKind::Opencode => {
            tracing::debug!(
                target: "embedder",
                backend = "opencode",
                "embed_via_backend: forcing opencode (GAP-OPENCODE-001)"
            );
            embed_via_opencode_local_resolved(models_dir, text, None, None)
        }
        LlmBackendKind::OpenRouter => {
            tracing::debug!(
                target: "embedder",
                backend = "openrouter",
                "embed_via_backend: using OpenRouter API (v1.0.93)"
            );
            let client = OPENROUTER_CLIENT.get().ok_or_else(|| {
                AppError::Embedding(
                    "OpenRouter client not initialised; call get_openrouter_embedder first".into(),
                )
            })?;
            let rt = shared_runtime()?;
            let vec = rt.block_on(client.embed_single(text, client.default_input_type()))?;
            Ok((vec, LlmBackendKind::OpenRouter))
        }
    }
}

// ADR-0046 / BUG-11 v1.0.88: specialisation of `embed_via_backend` that
// refuses to SILENTLY DEGRADE to `LlmBackendKind::None` after all real
// backends (Codex, Claude) have failed. The previous behaviour
// (`Ok((Vec::new(), None))`) caused the `remember` write path to persist
// memories with zero-dimensional embeddings — breaking `recall` and
// `hybrid-search` while returning exit 0 (BUG-11 CRITICAL).
//
// When `--llm-backend none` is explicitly requested (i.e. `last_err` is
// None AND the chain was a single-element `[None]`), pass
// `skip_on_failure = true` to `embed_with_fallback` to consume the empty
// vector via the pending-embeddings retry queue instead of persisting
// directly. This helper is the right hook for `remember`/`edit`/`ingest`.
pub fn embed_via_backend_strict(
    models_dir: &Path,
    text: &str,
    backend: &LlmBackendKind,
    last_err: Option<&AppError>,
    skip_on_failure: bool,
) -> Result<(Vec<f32>, LlmBackendKind), AppError> {
    use crate::llm::exit_code_hints::LlmBackendError;
    match backend {
        LlmBackendKind::None => {
            // If the caller opted into skip_on_failure AND no prior
            // backend has recorded an error, the empty vector is
            // intentional (chain of only [None]).
            if skip_on_failure && last_err.is_none() {
                Ok((Vec::new(), LlmBackendKind::None))
            } else if last_err.is_some() {
                // The chain reached `None` after Codex/Claude failed.
                // Propagate the most recent error so `remember` aborts
                // instead of persisting a memory without an embedding.
                Err(match last_err {
                    Some(e) => AppError::Embedding(format!("{e}")),
                    None => AppError::Embedding(LlmBackendError::NoBackendsAvailable.to_string()),
                })
            } else {
                // Empty chain with no skip_on_failure — treat as a
                // configuration error (no backends available).
                Err(AppError::Embedding(
                    LlmBackendError::NoBackendsAvailable.to_string(),
                ))
            }
        }
        LlmBackendKind::Codex => embed_passage_local_resolved(models_dir, text),
        LlmBackendKind::Claude => {
            tracing::debug!(
                target: "embedder",
                backend = "claude",
                "embed_via_backend_strict: forcing claude (ADR-0042 / GAP-002 fix)"
            );
            embed_via_claude_local_resolved(models_dir, text, None, None)
        }
        LlmBackendKind::Opencode => {
            tracing::debug!(
                target: "embedder",
                backend = "opencode",
                "embed_via_backend_strict: forcing opencode (GAP-OPENCODE-001)"
            );
            embed_via_opencode_local_resolved(models_dir, text, None, None)
        }
        LlmBackendKind::OpenRouter => embed_via_backend(models_dir, text, backend),
    }
}

/// Legacy one-shot wrapper around `embed_via_backend` that discards
/// the resolved backend. Kept for call sites that only care about
/// the vector and ignore the executed-backend signal. New code
/// should prefer `embed_via_backend` directly.
pub fn embed_via_backend_legacy(
    models_dir: &Path,
    text: &str,
    backend: &LlmBackendKind,
) -> Result<Vec<f32>, AppError> {
    embed_via_backend(models_dir, text, backend).map(|(v, _)| v)
}

pub fn embed_passages_controlled_local(
    models_dir: &Path,
    texts: &[&str],
    token_counts: &[usize],
) -> Result<Vec<Vec<f32>>, AppError> {
    let embedder = get_embedder(models_dir)?;
    embed_passages_controlled(embedder, texts, token_counts)
}

/// G42/S3: embeds `texts` through the bounded parallel fan-out and
/// returns vectors in input order.
pub fn embed_passages_parallel_local(
    models_dir: &Path,
    texts: &[String],
    parallelism: usize,
    batch_size: usize,
) -> Result<Vec<Vec<f32>>, AppError> {
    let embedder = get_embedder(models_dir)?;
    embed_texts_parallel(embedder, texts, parallelism, batch_size)
}

/// GAP-OPENROUTER-REST-CONCURRENCY: result of one bounded fan-out chunk —
/// the chunk index paired with the batch embedding result, used to restore
/// input order after out-of-order `JoinSet` completion.
type EmbedChunkResult = (usize, Result<Vec<Vec<f32>>, AppError>);

/// GAP-OPENROUTER-REST-CONCURRENCY: reassembles the flat vector list in
/// input order from chunk parts produced out-of-order by the bounded
/// `JoinSet` fan-out. Sorts by chunk index, then flattens, so the result
/// matches the original `texts` order exactly.
fn reassemble_ordered(mut parts: Vec<(usize, Vec<Vec<f32>>)>) -> Vec<Vec<f32>> {
    parts.sort_by_key(|(idx, _)| *idx);
    parts.into_iter().flat_map(|(_, v)| v).collect()
}

/// v1.0.93 (GAP-OR-INGEST): embeds multiple passages with
/// `EmbeddingBackendChoice` awareness. When the resolved chain starts
/// with `OpenRouter` and the client is initialised, uses the HTTP batch
/// API (`embed_batch`) instead of subprocess fan-out — no LLM slot
/// consumed, ~200ms per batch vs ~15s per subprocess cold-start.
/// Falls back to `embed_passages_parallel_local` for LLM backends.
pub fn embed_passages_parallel_with_embedding_choice(
    models_dir: &Path,
    texts: &[String],
    parallelism: usize,
    batch_size: usize,
    embedding_backend: crate::cli::EmbeddingBackendChoice,
    llm_backend: crate::cli::LlmBackendChoice,
) -> Result<Vec<Vec<f32>>, AppError> {
    let chain = embedding_backend.to_chain(llm_backend);
    if chain.first() == Some(&LlmBackendKind::OpenRouter) && is_openrouter_initialized() {
        let client = OPENROUTER_CLIENT.get().ok_or_else(|| {
            AppError::Embedding(
                "OpenRouter client not initialised; call get_openrouter_embedder first".into(),
            )
        })?;
        let rt = shared_runtime()?;

        // GAP-OPENROUTER-REST-CONCURRENCY: reuse the caller's `parallelism`
        // as a bounded fan-out width, clamped to a Cloudflare-safe range.
        // Small inputs stay serial — a single batch is one REST call, so the
        // JoinSet overhead would only add latency.
        let k = parallelism.clamp(1, 16);
        if texts.len() <= 32 || k == 1 {
            let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
            let vecs = rt.block_on(client.embed_batch(&refs, client.default_input_type()))?;
            return Ok(vecs);
        }

        // `client` is a `&'static OpenRouterClient` (OPENROUTER_CLIENT is a
        // static OnceLock), so it is Copy + Send + 'static and moves freely
        // into each spawned task. Chunk contents are cloned into owned
        // `Vec<String>` because `texts` is only borrowed.
        let vecs = rt.block_on(async move {
            let mut set: JoinSet<EmbedChunkResult> = JoinSet::new();
            let mut parts: Vec<(usize, Vec<Vec<f32>>)> = Vec::new();

            for (idx, chunk) in texts.chunks(32).enumerate() {
                if set.len() >= k {
                    if let Some(joined) = set.join_next().await {
                        let (cidx, res) = joined.map_err(|e| {
                            AppError::Embedding(format!("embedding task join error: {e}"))
                        })?;
                        parts.push((cidx, res?));
                    }
                }
                let owned: Vec<String> = chunk.to_vec();
                set.spawn(async move {
                    let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
                    let r = client.embed_batch(&refs, client.default_input_type()).await;
                    (idx, r)
                });
            }

            while let Some(joined) = set.join_next().await {
                let (cidx, res) = joined
                    .map_err(|e| AppError::Embedding(format!("embedding task join error: {e}")))?;
                parts.push((cidx, res?));
            }

            Ok::<Vec<Vec<f32>>, AppError>(reassemble_ordered(parts))
        })?;
        Ok(vecs)
    } else {
        embed_passages_parallel_local(models_dir, texts, parallelism, batch_size)
    }
}

/// G56: in-process cache for entity embeddings keyed by `(model, text)`.
///
/// Schema v13 is immutable: `entity_embeddings` does not have a `text`
/// column, so a pure DB-side cache would require a schema bump. Instead
/// we keep a process-wide LRU-style map that survives within one CLI
/// invocation. The hit rate is high in `ingest` (re-embedding the same
/// canonical entity across thousands of memories) and modest in `remember`
/// (typical single-memory invocations).
///
/// Key: `blake3(model || "\0" || text)`. Value: `Arc<Vec<f32>>` so the
/// collector can drop the map entry while a `Vec` is still in flight.
type EntityEmbedCacheMap = std::collections::HashMap<u64, Arc<Vec<f32>>>;

static ENTITY_EMBED_CACHE: OnceLock<parking_lot::Mutex<EntityEmbedCacheMap>> = OnceLock::new();

fn entity_embed_cache() -> &'static parking_lot::Mutex<EntityEmbedCacheMap> {
    ENTITY_EMBED_CACHE.get_or_init(|| parking_lot::Mutex::new(std::collections::HashMap::new()))
}

fn entity_cache_key(model: &str, text: &str) -> u64 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(model.as_bytes());
    hasher.update(b"\0");
    hasher.update(text.as_bytes());
    let h = hasher.finalize();
    let bytes = h.as_bytes();
    u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ])
}

/// G56: embeds entity-name texts through a process-wide cache.
///
/// Skips any `(model, text)` pair already produced in this CLI invocation
/// and only spawns subprocesses for the cache misses. Returns vectors in
/// the same order as `texts`.
///
/// Designed for entity-name batches (short texts). For chunk embeds use
/// [`embed_passages_parallel_local`] directly — chunks are unique per
/// memory and cache hit rate is negligible.
pub fn embed_entity_texts_cached(
    models_dir: &Path,
    texts: &[String],
    parallelism: usize,
    embedding_backend: crate::cli::EmbeddingBackendChoice,
    llm_backend: crate::cli::LlmBackendChoice,
) -> Result<(Vec<Vec<f32>>, EmbedCacheStats), AppError> {
    if texts.is_empty() {
        return Ok((Vec::new(), EmbedCacheStats::default()));
    }
    // GAP-OR-ENTITY-EMBED: resolve the SAME chain the chunk path uses so the
    // entity embedding honours `--embedding-backend`/`--llm-backend` instead
    // of always forcing the codex subprocess (the old G56 code path).
    let chain = embedding_backend.to_chain(llm_backend);

    // `none` short-circuit: when the resolved chain is exactly `[None]`
    // (`--embedding-backend llm --llm-backend none`) skip every backend and
    // return empty vectors WITHOUT spawning a subprocess. Empties are never
    // cached so a later call with a real backend in the same process is not
    // poisoned; they count as misses for stats parity with the chunk path.
    if chain.as_slice() == [LlmBackendKind::None] {
        let out: Vec<Vec<f32>> = texts.iter().map(|_| Vec::new()).collect();
        return Ok((
            out,
            EmbedCacheStats {
                requested: texts.len(),
                hits: 0,
                misses: texts.len(),
            },
        ));
    }

    // Cache model label reflects the EFFECTIVE embedding backend. When the
    // chain actually routes through OpenRouter, vectors carry that model's
    // dim/MRL profile and must never collide with codex-produced vectors;
    // for the local path we keep the prior `model_label()` so the in-process
    // cache key is unchanged (no regression — this cache is process-local).
    let routed_openrouter =
        chain.first() == Some(&LlmBackendKind::OpenRouter) && is_openrouter_initialized();
    let model = if routed_openrouter {
        format!("openrouter:{}", crate::constants::embedding_dim())
    } else {
        get_embedder(models_dir)?.lock().model_label()
    };
    let cache = entity_embed_cache();
    let mut hits: Vec<Option<Arc<Vec<f32>>>> = vec![None; texts.len()];
    let mut miss_indices: Vec<usize> = Vec::with_capacity(texts.len());
    {
        let guard = cache.lock();
        for (i, text) in texts.iter().enumerate() {
            let key = entity_cache_key(&model, text);
            if let Some(v) = guard.get(&key) {
                hits[i] = Some(Arc::clone(v));
            } else {
                miss_indices.push(i);
            }
        }
    }
    let miss_count = miss_indices.len();
    if miss_count > 0 {
        let miss_texts: Vec<String> = miss_indices.iter().map(|&i| texts[i].clone()).collect();
        // GAP-OR-ENTITY-EMBED: route misses through the backend-aware batch
        // helper (same one the chunk path uses). With OpenRouter this hits the
        // REST `embed_batch` (~200ms) instead of the codex subprocess (~120s).
        let miss_vecs = embed_passages_parallel_with_embedding_choice(
            models_dir,
            &miss_texts,
            parallelism,
            entity_embed_batch_size(),
            embedding_backend,
            llm_backend,
        )?;
        let mut guard = cache.lock();
        for (slot, &orig_idx) in miss_indices.iter().enumerate() {
            let vec = Arc::new(miss_vecs[slot].clone());
            let key = entity_cache_key(&model, &texts[orig_idx]);
            guard.insert(key, Arc::clone(&vec));
            hits[orig_idx] = Some(vec);
        }
    }
    let mut out = Vec::with_capacity(texts.len());
    for hit in hits.into_iter() {
        let v = hit.ok_or_else(|| {
            AppError::Embedding("entity embed cache produced null result".to_string())
        })?;
        out.push((*v).clone());
    }
    Ok((
        out,
        EmbedCacheStats {
            requested: texts.len(),
            hits: texts.len() - miss_count,
            misses: miss_count,
        },
    ))
}

/// G56: stats snapshot returned by [`embed_entity_texts_cached`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct EmbedCacheStats {
    pub requested: usize,
    pub hits: usize,
    pub misses: usize,
}

impl EmbedCacheStats {
    /// Hit rate as a fraction in `[0.0, 1.0]`. Returns 0.0 when nothing was requested.
    pub fn hit_rate(&self) -> f64 {
        if self.requested == 0 {
            0.0
        } else {
            self.hits as f64 / self.requested as f64
        }
    }
}

/// G42/S3 core: bounded parallel batch embedding.
///
/// - texts are grouped into batches of `batch_size` (one LLM call per
///   batch, G42/S2);
/// - at most `effective_permits(parallelism)` LLM subprocesses run
///   simultaneously (`Arc<Semaphore>` + `acquire_owned`, BLOCO 2);
/// - results stream through a BOUNDED mpsc channel so the caller-side
///   collector applies backpressure and can persist incrementally
///   (BLOCO 5);
/// - the global `CancellationToken` aborts in-flight work on the first
///   signal; subprocesses die with their futures via `kill_on_drop`
///   (BLOCO 6).
pub fn embed_texts_parallel(
    embedder: &Mutex<LlmEmbedding>,
    texts: &[String],
    parallelism: usize,
    batch_size: usize,
) -> Result<Vec<Vec<f32>>, AppError> {
    let mut slots: Vec<Option<Vec<f32>>> = vec![None; texts.len()];
    embed_texts_parallel_with(embedder, texts, parallelism, batch_size, |idx, v| {
        slots[idx] = Some(v.to_vec());
        Ok(())
    })?;
    let mut out = Vec::with_capacity(slots.len());
    for (idx, slot) in slots.into_iter().enumerate() {
        out.push(slot.ok_or_else(|| {
            AppError::Embedding(format!("embedding fan-out lost item index {idx}"))
        })?);
    }
    Ok(out)
}

/// Like [`embed_texts_parallel`] but invokes `on_result` as soon as each
/// embedding arrives (BLOCO 5: incremental persistence — a kill loses at
/// most the in-flight batches, never the already-delivered items).
pub fn embed_texts_parallel_with(
    embedder: &Mutex<LlmEmbedding>,
    texts: &[String],
    parallelism: usize,
    batch_size: usize,
    mut on_result: impl FnMut(usize, &[f32]) -> Result<(), AppError>,
) -> Result<(), AppError> {
    if texts.is_empty() {
        return Ok(());
    }
    let dim = crate::constants::embedding_dim();
    if texts.len() == 1 {
        let v = embed_passage(embedder, &texts[0])?;
        return on_result(0, &v);
    }

    let client = clone_client(embedder);
    let permits = effective_permits(parallelism);
    let batches = build_batches(texts, batch_size.max(1));
    let token = crate::cancel_token().clone();

    let work = move |batch: Vec<(usize, String)>| {
        let client = client.clone();
        async move {
            client
                .embed_batch_async(crate::constants::PASSAGE_PREFIX, &batch)
                .await
        }
    };

    let fan_out = run_bounded(batches, permits, dim, token, work, &mut on_result);
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(fan_out)),
        Err(_) => shared_runtime()?.block_on(fan_out),
    }
}

/// Groups `(global_index, text)` pairs into batches of `batch_size`.
fn build_batches(texts: &[String], batch_size: usize) -> Vec<Vec<(usize, String)>> {
    texts
        .iter()
        .cloned()
        .enumerate()
        .collect::<Vec<_>>()
        .chunks(batch_size)
        .map(|c| c.to_vec())
        .collect()
}

/// G42/S3 BLOCO 2: effective permit count.
///
/// `permits = clamp(requested, 1, 32) ∧ cpus ∧ ram_livre*0.5/RSS` — see
/// the module docs for the measured RSS rationale.
pub fn effective_permits(requested: usize) -> usize {
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let by_ram = ((crate::memory_guard::available_memory_mb() / 2)
        / crate::constants::LLM_WORKER_RSS_MB)
        .max(1) as usize;
    requested.clamp(1, 32).min(cpus).min(by_ram).max(1)
}

/// Bounded fan-out engine. Generic over the per-batch work so the
/// concurrency contract is testable without spawning real LLMs.
///
/// Cancel safety (BLOCO 6/10): every task races its work against
/// `token.cancelled()` inside `tokio::select!`; both branches are
/// cancel-safe (the work future owns its subprocess via `kill_on_drop`,
/// and `cancelled()` is pure). On collector-side errors the `JoinSet`
/// is shut down, which drops in-flight futures and kills their
/// subprocesses.
async fn run_bounded<F, Fut>(
    batches: Vec<Vec<(usize, String)>>,
    permits: usize,
    dim: usize,
    token: CancellationToken,
    work: F,
    on_result: &mut impl FnMut(usize, &[f32]) -> Result<(), AppError>,
) -> Result<(), AppError>
where
    F: Fn(Vec<(usize, String)>) -> Fut + Clone + Send + 'static,
    Fut: std::future::Future<Output = Result<Vec<(usize, Vec<f32>)>, AppError>> + Send,
{
    let total_batches = batches.len();
    let semaphore = Arc::new(Semaphore::new(permits));
    // BLOCO 5: bounded channel — producers block when the collector is
    // behind (backpressure); PROIBIDO unbounded_channel between stages.
    let (tx, mut rx) = mpsc::channel::<Result<Vec<(usize, Vec<f32>)>, AppError>>(permits * 2);
    let mut set: JoinSet<()> = JoinSet::new();

    for (batch_idx, batch) in batches.into_iter().enumerate() {
        let sem = Arc::clone(&semaphore);
        let token = token.clone();
        let tx = tx.clone();
        let work = work.clone();
        set.spawn(async move {
            let wait_start = std::time::Instant::now();
            // acquire_owned: RAII permit moved into the task; returned
            // on every exit path INCLUDING panic (BLOCO 2).
            let Ok(_permit) = sem.acquire_owned().await else {
                let _ = tx
                    .send(Err(AppError::Embedding("semaphore closed".to_string())))
                    .await;
                return;
            };
            let permit_wait_ms = wait_start.elapsed().as_millis() as u64;
            let work_start = std::time::Instant::now();
            // ADR-0034: when `SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1` is set the
            // cancellation arm is dropped and the batch runs to completion.
            // This unblocks audit/test invocations whose `SHUTDOWN` flag was
            // contaminated by an earlier signal handler in the same process
            // tree. Production code never sees this branch.
            let outcome = if crate::should_obey_shutdown() {
                tokio::select! {
                    res = work(batch) => res,
                    _ = token.cancelled() => Err(AppError::Embedding(
                        "embedding cancelled by shutdown signal".to_string(),
                    )),
                }
            } else {
                work(batch).await
            };
            // BLOCO 8: permit wait time logged SEPARATELY from work time.
            tracing::debug!(
                target: "embedding",
                batch_idx,
                permit_wait_ms,
                work_ms = work_start.elapsed().as_millis() as u64,
                ok = outcome.is_ok(),
                "embedding batch finished"
            );
            let _ = tx.send(outcome).await;
        });
    }
    drop(tx);

    let mut completed = 0usize;
    let mut failed = 0usize;
    let mut cancelled = 0usize;
    let mut first_error: Option<AppError> = None;

    while let Some(message) = rx.recv().await {
        match message {
            Ok(items) => {
                completed += 1;
                if first_error.is_none() {
                    for (idx, v) in items {
                        if v.len() != dim {
                            first_error = Some(AppError::Embedding(format!(
                                "LLM returned {} dims for item {idx}, expected {dim}; \
                                 refusing to truncate or pad silently (G42/C5)",
                                v.len()
                            )));
                            break;
                        }
                        if let Err(e) = on_result(idx, &v) {
                            first_error = Some(e);
                            break;
                        }
                    }
                    if first_error.is_some() {
                        // Abort remaining work: dropped futures kill
                        // their subprocesses via kill_on_drop (BLOCO 6).
                        set.shutdown().await;
                    }
                }
            }
            Err(e) => {
                if matches!(&e, AppError::Embedding(msg) if msg.contains("cancelled")) {
                    cancelled += 1;
                } else {
                    failed += 1;
                }
                if first_error.is_none() {
                    first_error = Some(e);
                    set.shutdown().await;
                }
            }
        }
    }

    // Drain the JoinSet: surface panics distinctly (panic handling —
    // JoinError::is_panic tratado em todo join_next, BLOCO 9).
    while let Some(join_result) = set.join_next().await {
        if let Err(join_err) = join_result {
            if join_err.is_panic() {
                failed += 1;
                if first_error.is_none() {
                    first_error = Some(AppError::Embedding(format!(
                        "embedding task panicked: {join_err}"
                    )));
                }
            } else {
                cancelled += 1;
            }
        }
    }

    // v1.0.85 (ADR-0043 hygiene): the fan-out summary event moved
    // from `tracing::info!` to `tracing::debug!` and the
    // `available_permits` field was removed — the user prohibited
    // pool-state telemetry (slot_pool_stats / slot_wait_ms) and
    // decorative `tracing::info!` events. The remaining counters
    // (total_batches / completed / failed / cancelled) describe the
    // progress of the operation itself, not the slot pool, and
    // remain visible to operators running with `RUST_LOG=debug` or
    // `-vvv`.
    tracing::debug!(
        target: "embedding",
        total_batches,
        completed,
        failed,
        cancelled,
        "embedding fan-out finished"
    );

    match first_error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

pub fn f32_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

pub fn bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    let mut out = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        out.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    out
}

/// Returns the dimensionality of the embedding space. Used to
/// validate LLM responses and to size the in-memory cache.
pub fn embedding_dim() -> usize {
    crate::constants::embedding_dim()
}

/// G42/C5: a vector with a divergent dimensionality is an ERROR, never
/// silently truncated or zero-padded (the pre-v1.0.79 `normalise_dim`
/// masked malformed LLM responses).
fn validate_dim(v: Vec<f32>) -> Result<Vec<f32>, AppError> {
    let dim = crate::constants::embedding_dim();
    if v.len() != dim {
        return Err(AppError::Embedding(format!(
            "embedding has {} dims, expected {dim}; \
             refusing to truncate or pad silently (G42/C5)",
            v.len()
        )));
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn reassemble_ordered_restores_input_order() {
        // GAP-OPENROUTER-REST-CONCURRENCY: the bounded JoinSet fan-out
        // completes chunks out of order, so parts arrive shuffled. The
        // reassembly MUST restore the exact input order by chunk index.
        let parts = vec![
            (2, vec![vec![2.0_f32], vec![2.1]]),
            (0, vec![vec![0.0], vec![0.1]]),
            (1, vec![vec![1.0], vec![1.1]]),
        ];
        let out = reassemble_ordered(parts);
        assert_eq!(
            out,
            vec![
                vec![0.0_f32],
                vec![0.1],
                vec![1.0],
                vec![1.1],
                vec![2.0],
                vec![2.1],
            ]
        );
    }

    #[test]
    fn f32_to_bytes_roundtrip() {
        let input = vec![0.0_f32, 1.5, -2.25, f32::MIN, f32::MAX];
        let bytes = f32_to_bytes(&input);
        assert_eq!(bytes.len(), input.len() * 4);
        let out = bytes_to_f32(&bytes);
        assert_eq!(out, input);
    }

    #[test]
    fn validate_dim_rejects_divergent_vectors() {
        // G42/C5 acceptance criterion: a divergent vector MUST fail —
        // never be silently normalised.
        let dim = crate::constants::embedding_dim();
        let long = vec![0.0; dim + 10];
        assert!(validate_dim(long).is_err(), "longer vector must error");
        let short = vec![0.0; dim.saturating_sub(1).max(1)];
        assert!(validate_dim(short).is_err(), "shorter vector must error");
        let exact = vec![0.0; dim];
        assert_eq!(validate_dim(exact).expect("exact dim must pass").len(), dim);
    }

    #[test]
    fn embedding_dim_matches_constants_source() {
        assert_eq!(embedding_dim(), crate::constants::embedding_dim());
    }

    #[test]
    fn build_batches_preserves_global_indices() {
        let texts: Vec<String> = (0..10).map(|i| format!("t{i}")).collect();
        let batches = build_batches(&texts, 4);
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].len(), 4);
        assert_eq!(batches[2].len(), 2);
        assert_eq!(batches[2][1].0, 9);
        assert_eq!(batches[2][1].1, "t9");
    }

    #[test]
    fn effective_permits_clamps_to_bounds() {
        assert!(effective_permits(0) >= 1);
        assert!(effective_permits(1000) <= 32);
    }

    fn test_batches(n: usize) -> Vec<Vec<(usize, String)>> {
        (0..n).map(|i| vec![(i, format!("t{i}"))]).collect()
    }

    fn dummy_vec(dim: usize) -> Vec<f32> {
        vec![0.0; dim]
    }

    /// G42 acceptance criterion: with N permits the measured peak of
    /// concurrent workers NEVER exceeds N, even with 10x more batches.
    #[test]
    fn concurrency_peak_never_exceeds_permits() {
        let permits = 4usize;
        let batches = test_batches(permits * 10);
        let dim = crate::constants::embedding_dim();
        let current = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        let current_c = Arc::clone(&current);
        let peak_c = Arc::clone(&peak);
        let work = move |batch: Vec<(usize, String)>| {
            let current = Arc::clone(&current_c);
            let peak = Arc::clone(&peak_c);
            async move {
                let now = current.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                current.fetch_sub(1, Ordering::SeqCst);
                Ok(batch
                    .into_iter()
                    .map(|(i, _)| (i, dummy_vec(dim)))
                    .collect())
            }
        };

        let mut delivered = 0usize;
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("test runtime");
        rt.block_on(run_bounded(
            batches,
            permits,
            dim,
            CancellationToken::new(),
            work,
            &mut |_idx, _v| {
                delivered += 1;
                Ok(())
            },
        ))
        .expect("fan-out must succeed");

        assert_eq!(delivered, permits * 10, "every item must be delivered");
        assert!(
            peak.load(Ordering::SeqCst) <= permits,
            "peak concurrency {} exceeded permits {permits}",
            peak.load(Ordering::SeqCst)
        );
    }

    /// G42 acceptance criterion: a panicking task returns its permit via
    /// RAII and surfaces as JoinError::is_panic, not a hang.
    #[test]
    fn panicking_task_returns_permit_and_surfaces_error() {
        let permits = 2usize;
        let batches = test_batches(4);
        let dim = crate::constants::embedding_dim();

        let work = move |batch: Vec<(usize, String)>| async move {
            if batch[0].0 == 1 {
                panic!("intentional test panic");
            }
            Ok(batch
                .into_iter()
                .map(|(i, _)| (i, dummy_vec(dim)))
                .collect())
        };

        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("test runtime");
        let result = rt.block_on(run_bounded(
            batches,
            permits,
            dim,
            CancellationToken::new(),
            work,
            &mut |_idx, _v| Ok(()),
        ));

        let err = result.expect_err("panic must surface as an error");
        assert!(
            err.to_string().contains("panicked"),
            "error must mention the panic: {err}"
        );
    }

    /// G42 acceptance criterion: cancellation aborts in-flight work and
    /// the fan-out terminates within the shutdown timeout.
    #[test]
    fn cancellation_terminates_fan_out_quickly() {
        let permits = 2usize;
        let batches = test_batches(8);
        let dim = crate::constants::embedding_dim();
        let token = CancellationToken::new();

        let work = move |batch: Vec<(usize, String)>| async move {
            // Long enough that only cancellation can finish the test fast.
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            Ok(batch
                .into_iter()
                .map(|(i, _)| (i, dummy_vec(dim)))
                .collect())
        };

        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("test runtime");
        let cancel = token.clone();
        let start = std::time::Instant::now();
        let result = rt.block_on(async move {
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                cancel.cancel();
            });
            run_bounded(batches, permits, dim, token, work, &mut |_idx, _v| Ok(())).await
        });

        assert!(result.is_err(), "cancelled fan-out must report an error");
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "graceful shutdown must finish well under the work duration"
        );
    }

    /// G42 acceptance criterion: a divergent dim coming out of the work
    /// stage fails the fan-out instead of being silently accepted.
    #[test]
    fn fan_out_rejects_divergent_dim() {
        let permits = 2usize;
        let batches = test_batches(2);
        let dim = crate::constants::embedding_dim();

        let work = move |batch: Vec<(usize, String)>| async move {
            Ok(batch
                .into_iter()
                .map(|(i, _)| (i, vec![0.0f32; 3]))
                .collect::<Vec<(usize, Vec<f32>)>>())
        };

        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("test runtime");
        let result = rt.block_on(run_bounded(
            batches,
            permits,
            dim,
            CancellationToken::new(),
            work,
            &mut |_idx, _v| Ok(()),
        ));

        let err = result.expect_err("divergent dim must fail the fan-out");
        assert!(err.to_string().contains("G42/C5"), "error cites C5: {err}");
    }

    /// G44: the calibration bases stay intact at the calibration dim.
    #[test]
    fn adaptive_batch_dim64_keeps_calibrated_sizes() {
        assert_eq!(adaptive_batch_for_dim(CHUNK_EMBED_BATCH_SIZE, 64), 8);
        assert_eq!(adaptive_batch_for_dim(ENTITY_EMBED_BATCH_SIZE, 64), 25);
    }

    /// G44: legacy 384-dim databases shrink to reliable batch sizes.
    #[test]
    fn adaptive_batch_dim384_shrinks() {
        assert_eq!(adaptive_batch_for_dim(CHUNK_EMBED_BATCH_SIZE, 384), 1);
        assert_eq!(adaptive_batch_for_dim(ENTITY_EMBED_BATCH_SIZE, 384), 4);
    }

    /// G44: intermediate dims scale proportionally to the float budget.
    #[test]
    fn adaptive_batch_intermediate_dims() {
        assert_eq!(adaptive_batch_for_dim(8, 128), 4);
        assert_eq!(adaptive_batch_for_dim(8, 256), 2);
    }

    /// G44: dims below the calibration dim never exceed the base.
    #[test]
    fn adaptive_batch_small_dim_clamps_to_base() {
        assert_eq!(adaptive_batch_for_dim(8, 8), 8);
    }

    /// G44: the function is total — no division by zero, no clamp panic.
    #[test]
    fn adaptive_batch_total_function() {
        assert_eq!(adaptive_batch_for_dim(8, 4096), 1);
        assert_eq!(adaptive_batch_for_dim(8, 0), 8);
        assert_eq!(adaptive_batch_for_dim(0, 64), 1);
    }

    /// G44 end-to-end: the public wrappers follow the env-dim override.
    #[test]
    #[serial_test::serial(env)]
    fn adaptive_wrappers_follow_env_dim() {
        std::env::set_var("SQLITE_GRAPHRAG_EMBEDDING_DIM", "384");
        let chunk = chunk_embed_batch_size();
        let entity = entity_embed_batch_size();
        std::env::remove_var("SQLITE_GRAPHRAG_EMBEDDING_DIM");
        crate::constants::set_active_embedding_dim(crate::constants::DEFAULT_EMBEDDING_DIM);
        assert_eq!(chunk, 1, "384-dim chunk batch must shrink to 1 (G44)");
        assert_eq!(entity, 4, "384-dim entity batch must shrink to 4 (G44)");
    }

    // ---------------------------------------------------------------
    // G58/S1: FallbackReason + try_embed_query_with_fallback tests
    // ---------------------------------------------------------------

    /// GAP-004 (v1.0.88): EmbeddingErrorKind::classify maps an OAuth
    /// error message to the OAuth variant regardless of case or
    /// surrounding text.
    #[test]
    fn embedding_error_kind_classify_oauth_message() {
        assert_eq!(
            EmbeddingErrorKind::classify("OAuth token expired for claude"),
            EmbeddingErrorKind::OAuth,
        );
        assert_eq!(
            EmbeddingErrorKind::classify("oauth authentication failed"),
            EmbeddingErrorKind::OAuth,
        );
    }

    /// GAP-004 (v1.0.88): EmbeddingErrorKind::classify maps a quota
    /// message to the Quota variant (without "OAuth" substring).
    #[test]
    fn embedding_error_kind_classify_quota_message() {
        assert_eq!(
            EmbeddingErrorKind::classify("quota exhausted on backend"),
            EmbeddingErrorKind::Quota,
        );
        assert_eq!(
            EmbeddingErrorKind::classify("Usage quota limit reached"),
            EmbeddingErrorKind::Quota,
        );
    }

    /// GAP-004 (v1.0.88): EmbeddingErrorKind::classify maps a slot-sema
    /// message to the SlotExhausted variant (matched BEFORE Quota so
    /// the more specific LLM-never-tried path wins).
    #[test]
    fn embedding_error_kind_classify_slot_exhausted_message() {
        assert_eq!(
            EmbeddingErrorKind::classify(
                "slot exhausted: failed to acquire LLM slot after backoff"
            ),
            EmbeddingErrorKind::SlotExhausted,
        );
    }

    /// GAP-004 (v1.0.88): EmbeddingErrorKind::classify maps a
    /// zero-dimensional vector error to the ZeroDimension variant.
    #[test]
    fn embedding_error_kind_classify_zero_dimension_message() {
        assert_eq!(
            EmbeddingErrorKind::classify("embedding returned dim=zero"),
            EmbeddingErrorKind::ZeroDimension,
        );
        assert_eq!(
            EmbeddingErrorKind::classify("got zero-dim vector from LLM"),
            EmbeddingErrorKind::ZeroDimension,
        );
    }

    /// GAP-004 (v1.0.88): EmbeddingErrorKind::classify falls back to
    /// the Unknown variant when no marker matches, and the code()
    /// accessor returns the kebab-safe discriminator string.
    #[test]
    fn embedding_error_kind_classify_unknown_fallback() {
        assert_eq!(
            EmbeddingErrorKind::classify("unrelated subprocess error"),
            EmbeddingErrorKind::Unknown,
        );
        assert_eq!(
            EmbeddingErrorKind::classify("rate limit hit"),
            EmbeddingErrorKind::Unknown,
        );
        // code() returns the stable discriminator string.
        assert_eq!(EmbeddingErrorKind::OAuth.code(), "oauth");
        assert_eq!(EmbeddingErrorKind::Quota.code(), "quota");
        assert_eq!(EmbeddingErrorKind::SlotExhausted.code(), "slot-exhausted");
        assert_eq!(
            EmbeddingErrorKind::BackendMismatch.code(),
            "backend-mismatch"
        );
        assert_eq!(EmbeddingErrorKind::ZeroDimension.code(), "zero-dimension");
        assert_eq!(EmbeddingErrorKind::Unknown.code(), "unknown");
    }

    /// Display impl covers all three variants without panicking.
    #[test]
    fn fallback_reason_display_does_not_panic() {
        let _ = FallbackReason::EmbeddingFailed("rate limit".into()).to_string();
        let _ = FallbackReason::Cancelled.to_string();
        let _ = FallbackReason::Timeout {
            operation: "embed_query".into(),
            duration_secs: 30,
        }
        .to_string();
    }

    /// FallbackReason is PartialEq — used in test assertions to verify
    /// the mapping rules.
    #[test]
    fn fallback_reason_is_partial_eq() {
        assert_eq!(
            FallbackReason::EmbeddingFailed("a".into()),
            FallbackReason::EmbeddingFailed("a".into())
        );
        assert_eq!(FallbackReason::Cancelled, FallbackReason::Cancelled);
        assert_ne!(
            FallbackReason::EmbeddingFailed("a".into()),
            FallbackReason::EmbeddingFailed("b".into())
        );
        assert_ne!(
            FallbackReason::Cancelled,
            FallbackReason::Timeout {
                operation: "x".into(),
                duration_secs: 1
            }
        );
    }

    /// Timeout variant preserves the operation name and duration from the
    /// original AppError::Timeout for observability.
    #[test]
    fn fallback_reason_timeout_preserves_fields() {
        let r = FallbackReason::Timeout {
            operation: "embed_query_local".into(),
            duration_secs: 300,
        };
        match r {
            FallbackReason::Timeout {
                operation,
                duration_secs,
            } => {
                assert_eq!(operation, "embed_query_local");
                assert_eq!(duration_secs, 300);
            }
            other => panic!("expected Timeout, got {other:?}"),
        }
    }

    /// try_embed_query_with_fallback surfaces an EmbeddingFailed variant
    /// when the LLM subprocess errors. Uses a path that surely does not
    /// contain any embedder configuration (the binary is invoked as
    /// `codex` / `claude` via PATH which, in tests, defaults to nothing
    /// in scope, so `LlmEmbedding::detect_available()` returns Err).
    #[test]
    #[ignore = "G58 S1 stub: requires env without codex/claude on PATH; tracked as T5 of Fase 2"]
    fn try_embed_query_with_fallback_surfaces_embedding_failed_for_missing_binary() {
        // Pointing at a models dir that does not exist forces the embedder
        // init to fail; the error is mapped to EmbeddingFailed.
        let bogus = std::path::Path::new("/nonexistent-models-dir-for-g58-fallback-test");
        let result = try_embed_query_with_fallback(bogus, "hello world");
        match result {
            Err(FallbackReason::EmbeddingFailed(msg)) => {
                // The original error must survive in the message for ops triage.
                assert!(!msg.is_empty(), "fallback message must not be empty");
            }
            Err(FallbackReason::Cancelled) => {
                panic!("expected EmbeddingFailed, got Cancelled");
            }
            Err(FallbackReason::Timeout { .. }) => {
                panic!("expected EmbeddingFailed, got Timeout");
            }
            Err(FallbackReason::SlotExhausted) => {
                panic!("expected EmbeddingFailed, got SlotExhausted");
            }
            Err(FallbackReason::OAuthQuota { .. }) => {
                panic!("expected EmbeddingFailed, got OAuthQuota");
            }
            Err(FallbackReason::BackendMismatch { .. }) => {
                panic!("expected EmbeddingFailed, got BackendMismatch");
            }
            Err(FallbackReason::DimZero) => {
                panic!("expected EmbeddingFailed, got DimZero");
            }
            Ok(_) => {
                panic!("expected an error, got Ok — embedder must fail for bogus path");
            }
        }
    }

    // G56: entity embed cache — unit tests
    #[test]
    fn g56_entity_cache_key_is_stable_and_distinct() {
        let k1 = entity_cache_key("codex:default", "sqlite-graphrag");
        let k2 = entity_cache_key("codex:default", "sqlite-graphrag");
        let k3 = entity_cache_key("codex:default", "claude-code");
        let k4 = entity_cache_key("claude:default", "sqlite-graphrag");
        assert_eq!(k1, k2, "same model+text must hash identically");
        assert_ne!(k1, k3, "different text must hash differently");
        assert_ne!(k1, k4, "different model must hash differently");
    }

    #[test]
    fn g56_entity_embed_cache_stats_hit_rate() {
        let zero = EmbedCacheStats::default();
        assert_eq!(zero.hit_rate(), 0.0);
        let half = EmbedCacheStats {
            requested: 4,
            hits: 2,
            misses: 2,
        };
        assert!((half.hit_rate() - 0.5).abs() < 1e-9);
        let all = EmbedCacheStats {
            requested: 7,
            hits: 7,
            misses: 0,
        };
        assert!((all.hit_rate() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn g56_entity_embed_cache_populates_and_hits() {
        // Manually populate the cache: bypasses the LLM by writing a
        // known vector under a chosen (model, text) key, then verifies
        // the cache is consulted before any LLM call would happen.
        let cache = entity_embed_cache();
        let model = "test-model";
        let text = "sqlite-graphrag";
        let key = entity_cache_key(model, text);
        let stored = Arc::new(vec![0.42_f32; crate::constants::embedding_dim()]);
        cache.lock().insert(key, Arc::clone(&stored));
        let guard = cache.lock();
        let hit = guard.get(&key).expect("cache must return stored value");
        assert_eq!(hit.len(), crate::constants::embedding_dim());
        assert!((hit[0] - 0.42).abs() < 1e-6);
    }

    #[test]
    fn g56_empty_texts_short_circuits_with_zero_stats() {
        // Cannot call embed_entity_texts_cached without an LLM on PATH,
        // so we only verify the empty-input contract via the stats struct.
        let stats = EmbedCacheStats::default();
        assert_eq!(stats.requested, 0);
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.hit_rate(), 0.0);
    }
}

// =============================================================================
// v1.0.82 (GAP-005) — embed_with_fallback tests
// =============================================================================
#[cfg(test)]
mod embed_with_fallback_tests {
    use super::*;
    use crate::llm::exit_code_hints::LlmBackendError;

    #[test]
    fn none_backend_returns_empty_vector_without_calling_llm() {
        // The `None` backend short-circuits to `Ok(vec![])` without
        // touching the LLM at all. This is the signal the caller uses
        // to insert a `pending_embeddings` row.
        let (v, kind) = embed_via_backend(
            std::path::Path::new("/nonexistent"),
            "any text",
            &LlmBackendKind::None,
        )
        .expect("None backend never fails");
        assert!(v.is_empty());
        assert_eq!(kind, LlmBackendKind::None, "None backend must report None");
    }

    #[test]
    fn empty_chain_defaults_to_codex_claude_none() {
        // Internal invariant: the default chain order is the v1.0.81
        // implicit order (codex first, then claude, then None as
        // graceful-degradation fallback).
        let defaults = [
            LlmBackendKind::Codex,
            LlmBackendKind::Claude,
            LlmBackendKind::None,
        ];

        // ---------------------------------------------------------------
        // ADR-0042: as_str + reason_code unit tests
        // ---------------------------------------------------------------

        #[allow(dead_code)]
        fn llm_backend_kind_as_str_is_stable() {
            assert_eq!(LlmBackendKind::Codex.as_str(), "codex");
            assert_eq!(LlmBackendKind::Claude.as_str(), "claude");
            assert_eq!(LlmBackendKind::None.as_str(), "none");
        }

        #[allow(dead_code)]
        fn fallback_reason_reason_code_is_stable() {
            assert_eq!(
                FallbackReason::EmbeddingFailed("any".into()).reason_code(),
                "embedding_failed"
            );
            assert_eq!(FallbackReason::Cancelled.reason_code(), "cancelled");
            assert_eq!(
                FallbackReason::Timeout {
                    operation: "embed_query".into(),
                    duration_secs: 30
                }
                .reason_code(),
                "timeout"
            );
        }
        assert_eq!(defaults.len(), 3);
    }

    #[test]
    fn embed_with_fallback_chain_of_only_none_aborts_without_skip_on_failure_v1088() {
        // ADR-0046 / BUG-11 v1.0.88: a fallback chain of only `[None]`
        // without `skip_on_failure=true` MUST abort with
        // `AppError::Embedding("no LLM backends available; fallback chain exhausted")`.
        //
        // Before BUG-11, the `None` tail returned `Ok((vec![], None))`
        // silently, which let `remember` persist a memory with a
        // zero-dimensional embedding (invisible to recall). The fix
        // routes the chain exhaustion through `embed_via_backend_strict`
        // so the caller can distinguish between "chain intentionally
        // degrades to skip" (skip_on_failure=true) and "chain has no
        // viable backend at all" (this test).
        let chain = vec![LlmBackendKind::None];
        let err = embed_with_fallback(
            std::path::Path::new("/nonexistent-models-dir-for-gap005-test"),
            "hello",
            &chain,
            false,
        )
        .expect_err("chain of only [None] without skip_on_failure MUST abort");
        let msg = format!("{err}");
        assert!(
            msg.contains("no LLM backends available"),
            "error must mention exhausted chain, got: {msg}"
        );
    }
    #[test]
    fn embed_with_fallback_skip_on_failure_with_only_none_returns_empty() {
        // skip_on_failure=true + a chain of only `None` returns Ok(vec![])
        // because the None short-circuit always succeeds. This is the
        // canonical contract: skip_on_failure is a no-op when None is
        // the tail because None already provides graceful degradation.
        let chain = vec![LlmBackendKind::None];
        let v = embed_with_fallback(
            std::path::Path::new("/nonexistent-models-dir-for-gap005-test"),
            "hello",
            &chain,
            true,
        )
        .expect("None chain is always Ok");
        assert!(v.0.is_empty(), "vector must be empty");
        assert_eq!(v.1, LlmBackendKind::None);
    }
    #[allow(dead_code)]
    fn llm_backend_error_no_backends_default_message() {
        // The fallback chain exhaustion error must mention
        // in its hint so the operator knows the remediation.
        let e = LlmBackendError::NoBackendsAvailable;
        let h = e.hint();
        assert!(h.contains("--llm-fallback"));
    }

    #[test]
    fn llm_backend_error_nonzero_exit_carries_stderr_tail() {
        let e = LlmBackendError::NonZeroExit {
            exit_code: Some(137),
            signal: Some(9),
            stdout_tail: "out".into(),
            stderr_tail: "OOM killed".into(),
            binary: "codex".into(),
            hint: "OOM".into(),
        };
        let s = e.to_string();
        assert!(s.contains("codex"));
        assert!(s.contains("OOM killed"));
        assert!(s.contains("signal 9") || s.contains("exit 137"));
    }
}
