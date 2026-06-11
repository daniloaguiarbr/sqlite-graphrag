//! Embedding generation for the GraphRAG memory.
//!
//! v1.0.76: the default build is **LLM-only** — the binary does NOT bundle
//! fastembed / ort / ndarray / tokenizers. All embeddings are produced
//! by a headless invocation of `claude code` or `codex` (OAuth, no MCP,
//! no hooks) and stored as a BLOB in `memory_embeddings(memory_id, embedding,
//! source)`. Vector similarity is computed in pure Rust at query time.
//!
//! # Workload classification (G42/S3, BLOCO 1 — OBRIGATÓRIA)
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

/// Process-wide LLM-embedding client behind a `Mutex`.
///
/// The lock guards configuration cloning only (see module docs); the
/// actual LLM I/O happens on clones, outside the lock.
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
    Ok(RUNTIME.get().expect("RUNTIME initialised above"))
}

/// Initialises the LLM-embedding client on first use and returns it.
pub fn get_embedder(_models_dir: &Path) -> Result<&'static Mutex<LlmEmbedding>, AppError> {
    if let Some(e) = EMBEDDER.get() {
        return Ok(e);
    }
    let backend = LlmEmbedding::detect_available()?;
    let _ = EMBEDDER.set(Mutex::new(backend));
    Ok(EMBEDDER.get().expect("EMBEDDER initialised above"))
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
    let embedder = get_embedder(models_dir)?;
    embed_passage(embedder, text)
}

pub fn embed_query_local(models_dir: &Path, text: &str) -> Result<Vec<f32>, AppError> {
    let embedder = get_embedder(models_dir)?;
    embed_query(embedder, text)
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
            let outcome = tokio::select! {
                res = work(batch) => res,
                _ = token.cancelled() => Err(AppError::Embedding(
                    "embedding cancelled by shutdown signal".to_string(),
                )),
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

    // BLOCO 8: saturation observability — available_permits plus the
    // completed/failed/cancelled counters on the progress stream.
    tracing::info!(
        target: "embedding",
        total_batches,
        completed,
        failed,
        cancelled,
        available_permits = semaphore.available_permits(),
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
                    .map(|(i, _)| (i, dummy_vec(crate::constants::embedding_dim())))
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
                .map(|(i, _)| (i, dummy_vec(crate::constants::embedding_dim())))
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
                .map(|(i, _)| (i, dummy_vec(crate::constants::embedding_dim())))
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
}
