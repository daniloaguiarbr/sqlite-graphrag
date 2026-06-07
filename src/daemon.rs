//! IPC daemon: keeps the embedding model warm across CLI invocations.
//!
//! Manages the background process lifecycle, Unix-socket IPC protocol, and
//! auto-start/backoff logic so embeddings are served without cold-start cost.

use crate::constants::{
    DAEMON_AUTO_START_INITIAL_BACKOFF_MS, DAEMON_AUTO_START_MAX_BACKOFF_MS,
    DAEMON_AUTO_START_MAX_WAIT_MS, DAEMON_IDLE_SHUTDOWN_SECS, DAEMON_PING_TIMEOUT_MS,
    DAEMON_SPAWN_BACKOFF_BASE_MS, DAEMON_SPAWN_LOCK_WAIT_MS, DAEMON_VERSION_RESTART_WAIT_MS,
    SQLITE_GRAPHRAG_VERSION,
};
use crate::errors::AppError;
use crate::{embedder, shutdown_requested};
use fs4::fs_std::FileExt;
use interprocess::local_socket::{
    prelude::LocalSocketStream,
    traits::{Listener as _, Stream as _},
    GenericFilePath, GenericNamespaced, ListenerNonblockingMode, ListenerOptions, ToFsName,
    ToNsName,
};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const VERSION_NOT_CHECKED: u8 = 0;
const VERSION_COMPATIBLE: u8 = 1;
const VERSION_RESTART_ATTEMPTED: u8 = 2;

/// Guards against restart loops: tracks version check state per process lifetime.
static DAEMON_VERSION_STATE: AtomicU8 = AtomicU8::new(VERSION_NOT_CHECKED);

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "snake_case")]
pub enum DaemonRequest {
    Ping,
    Shutdown,
    EmbedPassage {
        text: String,
    },
    EmbedQuery {
        text: String,
    },
    EmbedPassages {
        texts: Vec<String>,
        token_counts: Vec<usize>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DaemonResponse {
    Listening {
        pid: u32,
        socket: String,
        idle_shutdown_secs: u64,
    },
    Ok {
        pid: u32,
        version: String,
        handled_embed_requests: u64,
        model_name: String,
        model_variant: String,
    },
    PassageEmbedding {
        embedding: Vec<f32>,
        handled_embed_requests: u64,
    },
    QueryEmbedding {
        embedding: Vec<f32>,
        handled_embed_requests: u64,
    },
    PassageEmbeddings {
        embeddings: Vec<Vec<f32>>,
        handled_embed_requests: u64,
    },
    ShuttingDown {
        handled_embed_requests: u64,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct DaemonSpawnState {
    consecutive_failures: u32,
    not_before_epoch_ms: u64,
    last_error: Option<String>,
}

pub fn daemon_label(models_dir: &Path) -> String {
    let hash = blake3::hash(models_dir.to_string_lossy().as_bytes())
        .to_hex()
        .to_string();
    format!("sqlite-graphrag-daemon-{}", &hash[..16])
}

pub fn try_ping(models_dir: &Path) -> Result<Option<DaemonResponse>, AppError> {
    request_if_available(models_dir, &DaemonRequest::Ping)
}

pub fn try_shutdown(models_dir: &Path) -> Result<Option<DaemonResponse>, AppError> {
    request_if_available(models_dir, &DaemonRequest::Shutdown)
}

pub fn embed_passage_or_local(models_dir: &Path, text: &str) -> Result<Vec<f32>, AppError> {
    match request_or_autostart(
        models_dir,
        &DaemonRequest::EmbedPassage {
            text: text.to_string(),
        },
        true,
    )? {
        Some(DaemonResponse::PassageEmbedding { embedding, .. }) => Ok(embedding),
        Some(DaemonResponse::Error { message }) => Err(AppError::Embedding(message)),
        Some(other) => Err(AppError::Internal(anyhow::anyhow!(
            "unexpected daemon response for passage embedding: {other:?}"
        ))),
        None => {
            let embedder = embedder::get_embedder(models_dir)?;
            embedder::embed_passage(embedder, text)
        }
    }
}

pub fn embed_query_or_local(
    models_dir: &Path,
    text: &str,
    cli_autostart: bool,
) -> Result<Vec<f32>, AppError> {
    match request_or_autostart(
        models_dir,
        &DaemonRequest::EmbedQuery {
            text: text.to_string(),
        },
        cli_autostart,
    )? {
        Some(DaemonResponse::QueryEmbedding { embedding, .. }) => Ok(embedding),
        Some(DaemonResponse::Error { message }) => Err(AppError::Embedding(message)),
        Some(other) => Err(AppError::Internal(anyhow::anyhow!(
            "unexpected daemon response for query embedding: {other:?}"
        ))),
        None => {
            let embedder = embedder::get_embedder(models_dir)?;
            embedder::embed_query(embedder, text)
        }
    }
}

pub fn embed_passages_controlled_or_local(
    models_dir: &Path,
    texts: &[&str],
    token_counts: &[usize],
) -> Result<Vec<Vec<f32>>, AppError> {
    let request = DaemonRequest::EmbedPassages {
        texts: texts.iter().map(|t| (*t).to_string()).collect(),
        token_counts: token_counts.to_vec(),
    };

    match request_or_autostart(models_dir, &request, true)? {
        Some(DaemonResponse::PassageEmbeddings { embeddings, .. }) => Ok(embeddings),
        Some(DaemonResponse::Error { message }) => Err(AppError::Embedding(message)),
        Some(other) => Err(AppError::Internal(anyhow::anyhow!(
            "unexpected daemon response for passage embedding batch: {other:?}"
        ))),
        None => {
            let embedder = embedder::get_embedder(models_dir)?;
            embedder::embed_passages_controlled(embedder, texts, token_counts)
        }
    }
}

struct DaemonSpawnGuard {
    models_dir: PathBuf,
}

impl DaemonSpawnGuard {
    fn new(models_dir: &Path) -> Self {
        Self {
            models_dir: models_dir.to_path_buf(),
        }
    }
}

impl Drop for DaemonSpawnGuard {
    fn drop(&mut self) {
        let lock_path = spawn_lock_path(&self.models_dir);
        if lock_path.exists() {
            match std::fs::remove_file(&lock_path) {
                Ok(()) => {
                    tracing::debug!(
                        target: "daemon",
                        path = %lock_path.display(),
                        "spawn lock file removed during graceful daemon shutdown"
                    );
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => {
                    tracing::warn!(
                        target: "daemon",
                        error = %err,
                        path = %lock_path.display(),
                        "failed to remove spawn lock file while shutting down daemon"
                    );
                }
            }
        }
        let pid_path = pid_file_path(&self.models_dir);
        let _ = std::fs::remove_file(&pid_path);

        tracing::info!(
            target: "daemon",
            "daemon shut down gracefully; socket will be cleaned up by OS or by the next daemon via try_overwrite"
        );
    }
}

pub fn run(
    models_dir: &Path,
    idle_shutdown_secs: u64,
    shutdown_timeout_secs: u64,
) -> Result<(), AppError> {
    // Scale worker threads to available parallelism so embedding tasks saturate CPU cores.
    // Clamped to [2, 8] to avoid excessive threads on high-core machines.
    let permits = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .clamp(2, 8);
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(permits)
        .thread_name("daemon-worker")
        .enable_all()
        .build()
        .map_err(AppError::Io)?;

    let result = rt.block_on(run_async(models_dir, idle_shutdown_secs, permits));
    rt.shutdown_timeout(std::time::Duration::from_secs(shutdown_timeout_secs));
    result
}

#[tracing::instrument(skip_all, fields(idle_secs = idle_shutdown_secs, permits))]
async fn run_async(
    models_dir: &Path,
    idle_shutdown_secs: u64,
    permits: usize,
) -> Result<(), AppError> {
    let socket = daemon_label(models_dir);
    let name = to_local_socket_name(&socket)?;
    let listener = ListenerOptions::new()
        .name(name)
        .nonblocking(ListenerNonblockingMode::Accept)
        .try_overwrite(true)
        .create_sync()
        .map_err(AppError::Io)?;

    // Guard that cleans up the spawn lock file on graceful shutdown.
    // SIGKILL does not trigger Drop; in that case try_overwrite(true) above is the fallback.
    let _spawn_guard = DaemonSpawnGuard::new(models_dir);

    // Warm the model once per daemon process inside spawn_blocking so the
    // ONNX session initialisation (CPU-bound, may take several seconds) does
    // not block a tokio worker thread.
    let models_dir_warm = models_dir.to_path_buf();
    tokio::task::spawn_blocking(move || embedder::get_embedder(&models_dir_warm).map(|_| ()))
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("model warm-up panicked: {e}")))??;

    let pid_path = pid_file_path(models_dir);
    let _ = std::fs::write(&pid_path, std::process::id().to_string());

    crate::output::emit_json(&DaemonResponse::Listening {
        pid: std::process::id(),
        socket,
        idle_shutdown_secs,
    })?;

    let handled_embed_requests = Arc::new(AtomicU64::new(0));
    let mut last_activity = Instant::now();
    let models_dir = models_dir.to_path_buf();
    // Bound concurrent spawn_blocking tasks to the same thread count as the runtime.
    let permit_pool = Arc::new(tokio::sync::Semaphore::new(permits));

    let token = crate::cancel_token();
    loop {
        if shutdown_requested() || token.is_cancelled() {
            break;
        }

        if !daemon_control_dir(&models_dir).exists() {
            tracing::info!(target: "daemon", "daemon control directory disappeared; shutting down");
            break;
        }

        match listener.accept() {
            Ok(stream) => {
                last_activity = Instant::now();
                let models_dir_clone = models_dir.clone();
                let counter = Arc::clone(&handled_embed_requests);
                let permit =
                    permit_pool.clone().acquire_owned().await.map_err(|e| {
                        AppError::Internal(anyhow::anyhow!("semaphore closed: {e}"))
                    })?;
                let should_exit = tokio::task::spawn_blocking(move || {
                    let _permit = permit; // hold until end of scope
                    handle_client(stream, &models_dir_clone, &counter)
                })
                .await
                .map_err(|e| {
                    AppError::Internal(anyhow::anyhow!("spawn_blocking panicked: {e}"))
                })??;

                if should_exit {
                    break;
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                if last_activity.elapsed() >= Duration::from_secs(idle_shutdown_secs) {
                    tracing::info!(
                        target: "daemon",
                        idle_shutdown_secs,
                        handled_embed_requests = handled_embed_requests.load(Ordering::Relaxed),
                        "daemon idle timeout reached"
                    );
                    break;
                }
                tokio::select! {
                    () = tokio::time::sleep(Duration::from_millis(50)) => {}
                    () = token.cancelled() => { break; }
                }
            }
            Err(err) => return Err(AppError::Io(err)),
        }
    }

    Ok(())
}

fn handle_client(
    stream: LocalSocketStream,
    models_dir: &Path,
    handled_embed_requests: &AtomicU64,
) -> Result<bool, AppError> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(AppError::Io)?;

    if line.trim().is_empty() {
        write_response(
            reader.get_mut(),
            &DaemonResponse::Error {
                message: "empty request to daemon".to_string(),
            },
        )?;
        return Ok(false);
    }

    let request: DaemonRequest = serde_json::from_str(line.trim()).map_err(AppError::Json)?;
    let (response, should_exit) = match request {
        DaemonRequest::Ping => (
            DaemonResponse::Ok {
                pid: std::process::id(),
                version: SQLITE_GRAPHRAG_VERSION.to_string(),
                handled_embed_requests: handled_embed_requests.load(Ordering::Relaxed),
                model_name: crate::constants::FASTEMBED_MODEL_DEFAULT.to_string(),
                model_variant: gliner_variant_from_env(),
            },
            false,
        ),
        DaemonRequest::Shutdown => (
            DaemonResponse::ShuttingDown {
                handled_embed_requests: handled_embed_requests.load(Ordering::Relaxed),
            },
            true,
        ),
        DaemonRequest::EmbedPassage { text } => {
            let embedder = embedder::get_embedder(models_dir)?;
            let embedding = embedder::embed_passage(embedder, &text)?;
            let count = handled_embed_requests.fetch_add(1, Ordering::Relaxed) + 1;
            (
                DaemonResponse::PassageEmbedding {
                    embedding,
                    handled_embed_requests: count,
                },
                false,
            )
        }
        DaemonRequest::EmbedQuery { text } => {
            let embedder = embedder::get_embedder(models_dir)?;
            let embedding = embedder::embed_query(embedder, &text)?;
            let count = handled_embed_requests.fetch_add(1, Ordering::Relaxed) + 1;
            (
                DaemonResponse::QueryEmbedding {
                    embedding,
                    handled_embed_requests: count,
                },
                false,
            )
        }
        DaemonRequest::EmbedPassages {
            texts,
            token_counts,
        } => {
            let embedder = embedder::get_embedder(models_dir)?;
            let text_refs: Vec<&str> = texts.iter().map(String::as_str).collect();
            let embeddings =
                embedder::embed_passages_controlled(embedder, &text_refs, &token_counts)?;
            let count = handled_embed_requests.fetch_add(1, Ordering::Relaxed) + 1;
            (
                DaemonResponse::PassageEmbeddings {
                    embeddings,
                    handled_embed_requests: count,
                },
                false,
            )
        }
    };

    write_response(reader.get_mut(), &response)?;
    Ok(should_exit)
}

fn write_response(
    stream: &mut LocalSocketStream,
    response: &DaemonResponse,
) -> Result<(), AppError> {
    serde_json::to_writer(&mut *stream, response).map_err(AppError::Json)?;
    stream.write_all(b"\n").map_err(AppError::Io)?;
    stream.flush().map_err(AppError::Io)?;
    Ok(())
}

fn request_if_available(
    models_dir: &Path,
    request: &DaemonRequest,
) -> Result<Option<DaemonResponse>, AppError> {
    let socket = daemon_label(models_dir);
    let name = match to_local_socket_name(&socket) {
        Ok(name) => name,
        Err(err) => return Err(AppError::Io(err)),
    };

    let mut stream = match LocalSocketStream::connect(name) {
        Ok(stream) => stream,
        Err(err)
            if matches!(
                err.kind(),
                std::io::ErrorKind::NotFound
                    | std::io::ErrorKind::ConnectionRefused
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::AddrNotAvailable
                    | std::io::ErrorKind::TimedOut
            ) =>
        {
            // v1.0.75: ConnectionReset is what the kernel returns when the socket
            // file exists but the daemon was killed (e.g., by `daemon --stop` or
            // an upgrade auto-restart) between connect and handshake. Treat it as
            // "daemon not available" and fall through to the local embedder.
            return Ok(None);
        }
        Err(err) => return Err(AppError::Io(err)),
    };

    if let Err(err) = serde_json::to_writer(&mut stream, request) {
        // serde_json serialisation errors are deterministic (caller bug), not
        // daemon-gone, so surface them as JSON errors.
        return Err(AppError::Json(err));
    }
    if let Err(err) = stream.write_all(b"\n") {
        if is_daemon_gone(&err) {
            return Ok(None);
        }
        return Err(AppError::Io(err));
    }
    if let Err(err) = stream.flush() {
        if is_daemon_gone(&err) {
            return Ok(None);
        }
        return Err(AppError::Io(err));
    }

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if let Err(err) = reader.read_line(&mut line) {
        if is_daemon_gone(&err) {
            return Ok(None);
        }
        return Err(AppError::Io(err));
    }
    if line.trim().is_empty() {
        return Err(AppError::Embedding(
            "daemon returned an empty response".into(),
        ));
    }

    let response = serde_json::from_str(line.trim()).map_err(AppError::Json)?;
    Ok(Some(response))
}

fn should_autostart(cli_flag: bool) -> bool {
    if !cli_flag {
        return false; // explicit CLI override wins
    }
    !autostart_disabled_by_env()
}

/// Returns true when an I/O error indicates the daemon died mid-request
/// (Connection reset, broken pipe, hung up). Callers should treat these
/// as "daemon not available" and fall back to the local embedder instead
/// of surfacing a hard I/O error to the user.
fn is_daemon_gone(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::ConnectionAborted
    )
}

/// Checks whether a running daemon has a different version from the current CLI binary.
/// If a mismatch is detected, shuts down the stale daemon, waits for it to exit, and
/// re-spawns a fresh one. The `VERSION_RESTART_ATTEMPTED` state prevents infinite loops:
/// this function is a no-op after the first attempt regardless of outcome.
fn maybe_restart_for_version_mismatch(models_dir: &Path) -> Result<(), AppError> {
    // ORDERING: Acquire on success synchronizes-with the Release store at line ~505.
    // Relaxed on failure: no dependent memory is read on the CAS failure path.
    if DAEMON_VERSION_STATE
        .compare_exchange(
            VERSION_NOT_CHECKED,
            VERSION_COMPATIBLE,
            Ordering::Acquire,
            Ordering::Relaxed,
        )
        .is_err()
    {
        // Already checked (compatible) or already attempted a restart — skip.
        return Ok(());
    }

    let response = match try_ping(models_dir)? {
        Some(r) => r,
        None => return Ok(()), // no daemon running, nothing to check
    };

    let daemon_version = match &response {
        DaemonResponse::Ok { version, .. } => version.as_str(),
        _ => return Ok(()), // unexpected response shape, skip
    };

    if daemon_version == SQLITE_GRAPHRAG_VERSION {
        return Ok(()); // versions match, state already set to COMPATIBLE
    }

    // Mismatch detected — mark as restart-attempted so we never loop.
    // ORDERING: Release pairs with the Acquire in compare_exchange and load.
    DAEMON_VERSION_STATE.store(VERSION_RESTART_ATTEMPTED, Ordering::Release);

    tracing::warn!(
        target: "daemon",
        daemon_version = %daemon_version,
        cli_version = SQLITE_GRAPHRAG_VERSION,
        "daemon version mismatch detected; auto-restarting daemon"
    );

    // Send shutdown request.
    try_shutdown(models_dir)?;

    // Wait for the stale daemon to exit.
    wait_for_daemon_exit(models_dir)?;

    // Re-spawn the daemon via the existing mechanism.
    ensure_daemon_running(models_dir)?;

    Ok(())
}

/// Polls until the daemon stops responding to pings, with exponential backoff.
/// Starts at 50 ms, doubles each iteration, caps at 500 ms per sleep.
/// Returns `Ok(())` once the daemon is gone or the timeout is reached.
#[cold]
#[inline(never)]
fn wait_for_daemon_exit(models_dir: &Path) -> Result<(), AppError> {
    let deadline = Instant::now() + Duration::from_millis(DAEMON_VERSION_RESTART_WAIT_MS);
    let mut sleep_ms: u64 = 50;

    while Instant::now() < deadline {
        if try_ping(models_dir)?.is_none() {
            tracing::debug!(target: "daemon", "stale daemon exited after version-mismatch shutdown");
            return Ok(());
        }
        thread::sleep(Duration::from_millis(sleep_ms));
        sleep_ms = (sleep_ms * 2).min(500);
    }

    tracing::warn!(
        target: "daemon",
        timeout_ms = DAEMON_VERSION_RESTART_WAIT_MS,
        "timed out waiting for stale daemon to exit after version-mismatch shutdown"
    );
    Ok(())
}

fn request_or_autostart(
    models_dir: &Path,
    request: &DaemonRequest,
    cli_autostart: bool,
) -> Result<Option<DaemonResponse>, AppError> {
    // ORDERING: Acquire pairs with the Release store in maybe_restart_for_version_mismatch.
    if DAEMON_VERSION_STATE.load(Ordering::Acquire) == VERSION_NOT_CHECKED {
        maybe_restart_for_version_mismatch(models_dir)?;
        // v1.0.75 (G22 follow-up): after a version-mismatch restart, the new daemon
        // is detached and may not be listening yet. Wait for the socket to be live
        // before issuing the first request, otherwise the client races the spawn
        // and gets "Connection reset by peer" (IO error 104) on a freshly-killed
        // socket.
        if DAEMON_VERSION_STATE.load(Ordering::Acquire) == VERSION_RESTART_ATTEMPTED {
            wait_for_daemon_ready(models_dir)?;
        }
    }

    if let Some(response) = request_if_available(models_dir, request)? {
        clear_spawn_backoff_state(models_dir).ok();
        return Ok(Some(response));
    }

    if !should_autostart(cli_autostart) {
        return Ok(None);
    }

    if !ensure_daemon_running(models_dir)? {
        return Ok(None);
    }

    // v1.0.75: ensure_daemon_running may have just spawned a fresh daemon; wait
    // for the new socket to be live before issuing the request.
    if !wait_for_daemon_ready(models_dir)? {
        return Ok(None);
    }

    request_if_available(models_dir, request)
}

fn ensure_daemon_running(models_dir: &Path) -> Result<bool, AppError> {
    if (try_ping(models_dir)?).is_some() {
        clear_spawn_backoff_state(models_dir).ok();
        return Ok(true);
    }

    if spawn_backoff_active(models_dir)? {
        tracing::warn!(target: "daemon", "daemon autostart suppressed by backoff window");
        return Ok(false);
    }

    let spawn_lock = match try_acquire_spawn_lock(models_dir)? {
        Some(lock) => lock,
        None => return wait_for_daemon_ready(models_dir),
    };

    if (try_ping(models_dir)?).is_some() {
        clear_spawn_backoff_state(models_dir).ok();
        drop(spawn_lock);
        return Ok(true);
    }

    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            record_spawn_failure(models_dir, &format!("current_exe failed: {err}"))?;
            drop(spawn_lock);
            return Ok(false);
        }
    };

    let mut child = std::process::Command::new(exe);
    child
        .arg("daemon")
        .arg("--idle-shutdown-secs")
        .arg(DAEMON_IDLE_SHUTDOWN_SECS.to_string())
        .env("SQLITE_GRAPHRAG_DAEMON_CHILD", "1")
        .env_remove("LD_PRELOAD")
        .env_remove("LD_LIBRARY_PATH")
        .env_remove("LD_AUDIT")
        .env_remove("DYLD_INSERT_LIBRARIES")
        .env_remove("DYLD_LIBRARY_PATH")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    match crate::commands::claude_runner::spawn_with_memory_limit(&mut child) {
        Ok(child_handle) => {
            // SAFETY: deliberate orphan daemon detach. The Child handle is intentionally
            // dropped without a corresponding `.wait()` call because the daemon owns its
            // own lifecycle: `Stdio::null()` is set on stdin/stdout/stderr (above) so the
            // child does not inherit terminal handles, the spawn lock file at
            // `<models_dir>/.daemon.spawn.lock` prevents concurrent spawns, and the
            // daemon shuts itself down via `DAEMON_IDLE_SHUTDOWN_SECS` (or an explicit
            // `daemon stop`/SIGTERM). Keeping the handle here would block the parent
            // CLI in the foreground until the daemon exited, defeating the autostart
            // contract that callers expect.
            // See: docs_rules/rules_rust_processos_externos.md section "Child detach justificado"
            //      AND docs/adr/0001-daemon-warmup-exception.md (authorized exception to no-daemon rule)
            let pid = child_handle.id();
            drop(child_handle);
            tracing::debug!(
                target: "daemon",
                pid,
                "daemon detached; lifecycle managed via spawn lock + readiness file"
            );
            let ready = wait_for_daemon_ready(models_dir)?;
            if ready {
                clear_spawn_backoff_state(models_dir).ok();
            } else {
                record_spawn_failure(models_dir, "daemon did not become healthy after autostart")?;
            }
            drop(spawn_lock);
            Ok(ready)
        }
        Err(err) => {
            record_spawn_failure(models_dir, &format!("daemon spawn failed: {err}"))?;
            drop(spawn_lock);
            Ok(false)
        }
    }
}

fn wait_for_daemon_ready(models_dir: &Path) -> Result<bool, AppError> {
    let deadline = Instant::now() + Duration::from_millis(DAEMON_AUTO_START_MAX_WAIT_MS);
    let mut sleep_ms = DAEMON_AUTO_START_INITIAL_BACKOFF_MS.max(DAEMON_PING_TIMEOUT_MS);

    while Instant::now() < deadline {
        if (try_ping(models_dir)?).is_some() {
            return Ok(true);
        }
        thread::sleep(Duration::from_millis(sleep_ms));
        sleep_ms = (sleep_ms * 2).min(DAEMON_AUTO_START_MAX_BACKOFF_MS);
    }

    Ok(false)
}

fn autostart_disabled_by_env() -> bool {
    std::env::var("SQLITE_GRAPHRAG_DAEMON_CHILD").as_deref() == Ok("1")
        || std::env::var("SQLITE_GRAPHRAG_DAEMON_FORCE_AUTOSTART").as_deref() != Ok("1")
            && std::env::var("SQLITE_GRAPHRAG_DAEMON_DISABLE_AUTOSTART").as_deref() == Ok("1")
}

fn daemon_control_dir(models_dir: &Path) -> PathBuf {
    models_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| models_dir.to_path_buf())
}

fn spawn_lock_path(models_dir: &Path) -> PathBuf {
    daemon_control_dir(models_dir).join("daemon-spawn.lock")
}

fn spawn_state_path(models_dir: &Path) -> PathBuf {
    daemon_control_dir(models_dir).join("daemon-spawn-state.json")
}

fn pid_file_path(models_dir: &Path) -> PathBuf {
    daemon_control_dir(models_dir).join("daemon.pid")
}

fn try_acquire_spawn_lock(models_dir: &Path) -> Result<Option<File>, AppError> {
    let path = spawn_lock_path(models_dir);
    std::fs::create_dir_all(crate::paths::parent_or_err(&path)?).map_err(AppError::Io)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(AppError::Io)?;

    let deadline = Instant::now() + Duration::from_millis(DAEMON_SPAWN_LOCK_WAIT_MS);
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(Some(file)),
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Ok(None);
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(err) => return Err(AppError::Io(err)),
        }
    }
}

fn spawn_backoff_active(models_dir: &Path) -> Result<bool, AppError> {
    let state = load_spawn_state(models_dir)?;
    Ok(now_epoch_ms() < state.not_before_epoch_ms)
}

#[cold]
#[inline(never)]
fn record_spawn_failure(models_dir: &Path, message: &str) -> Result<(), AppError> {
    let mut state = load_spawn_state(models_dir)?;
    state.consecutive_failures = state.consecutive_failures.saturating_add(1);
    let exponent = state.consecutive_failures.saturating_sub(1).min(6);
    let base_ms =
        (DAEMON_SPAWN_BACKOFF_BASE_MS * (1_u64 << exponent)).min(DAEMON_AUTO_START_MAX_BACKOFF_MS);
    // v1.0.36 (L2) + v1.0.43 (H7): half-jitter via fastrand (replaces SystemTime nanoseconds
    // which violated rules_rust_retry_com_backoff.md). Effective backoff range: [base/2, base).
    let half = base_ms / 2;
    let jitter = if half == 0 { 0 } else { fastrand::u64(0..half) };
    let backoff_ms = half + jitter;
    state.not_before_epoch_ms = now_epoch_ms() + backoff_ms;
    state.last_error = Some(message.to_string());
    save_spawn_state(models_dir, &state)
}

fn clear_spawn_backoff_state(models_dir: &Path) -> Result<(), AppError> {
    let path = spawn_state_path(models_dir);
    if path.exists() {
        std::fs::remove_file(path).map_err(AppError::Io)?;
    }
    Ok(())
}

fn load_spawn_state(models_dir: &Path) -> Result<DaemonSpawnState, AppError> {
    let path = spawn_state_path(models_dir);
    if !path.exists() {
        return Ok(DaemonSpawnState::default());
    }

    let bytes = std::fs::read(path).map_err(AppError::Io)?;
    serde_json::from_slice(&bytes).map_err(AppError::Json)
}

fn save_spawn_state(models_dir: &Path, state: &DaemonSpawnState) -> Result<(), AppError> {
    let path = spawn_state_path(models_dir);
    std::fs::create_dir_all(crate::paths::parent_or_err(&path)?).map_err(AppError::Io)?;
    let bytes = serde_json::to_vec(state).map_err(AppError::Json)?;
    std::fs::write(path, bytes).map_err(AppError::Io)
}

/// Returns the GLiNER model variant string based on the environment variable
/// `SQLITE_GRAPHRAG_GLINER_VARIANT`, defaulting to `"fp32"`.
fn gliner_variant_from_env() -> String {
    std::env::var("SQLITE_GRAPHRAG_GLINER_VARIANT").unwrap_or_else(|_| "fp32".to_string())
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis() as u64
}

fn to_local_socket_name(name: &str) -> std::io::Result<interprocess::local_socket::Name<'static>> {
    if let Ok(ns_name) = name.to_string().to_ns_name::<GenericNamespaced>() {
        return Ok(ns_name);
    }

    // Fallback when abstract namespaces are unavailable. Honours XDG_RUNTIME_DIR
    // (Linux user-private runtime dir) or SQLITE_GRAPHRAG_HOME (project override)
    // before falling back to /tmp, which can collide when the same name is used
    // by another user/project on a multi-tenant host. Added in v1.0.35.
    let path = if cfg!(unix) {
        let base = std::env::var_os("XDG_RUNTIME_DIR")
            .or_else(|| std::env::var_os("SQLITE_GRAPHRAG_HOME"))
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        base.join(format!("{name}.sock"))
            .to_string_lossy()
            .into_owned()
    } else {
        format!(r"\\.\pipe\{name}")
    };
    path.to_fs_name::<GenericFilePath>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_clear_spawn_backoff_state() {
        let tmp = tempfile::tempdir().unwrap();
        let models_dir = tmp.path().join("cache").join("models");
        std::fs::create_dir_all(&models_dir).unwrap();

        assert!(!spawn_backoff_active(&models_dir).unwrap());

        record_spawn_failure(&models_dir, "spawn failed").unwrap();
        assert!(spawn_backoff_active(&models_dir).unwrap());

        let state = load_spawn_state(&models_dir).unwrap();
        assert_eq!(state.consecutive_failures, 1);
        assert_eq!(state.last_error.as_deref(), Some("spawn failed"));

        clear_spawn_backoff_state(&models_dir).unwrap();
        assert!(!spawn_backoff_active(&models_dir).unwrap());
    }

    #[test]
    fn daemon_control_dir_uses_models_parent() {
        let base = PathBuf::from("/tmp/sqlite-graphrag-cache-test");
        let models_dir = base.join("models");
        assert_eq!(daemon_control_dir(&models_dir), base);
    }

    #[test]
    fn version_state_constants_are_distinct() {
        assert_ne!(VERSION_NOT_CHECKED, VERSION_COMPATIBLE);
        assert_ne!(VERSION_NOT_CHECKED, VERSION_RESTART_ATTEMPTED);
        assert_ne!(VERSION_COMPATIBLE, VERSION_RESTART_ATTEMPTED);
    }

    #[test]
    fn wait_for_daemon_exit_immediate_when_not_running() {
        let tmp = tempfile::tempdir().unwrap();
        let models_dir = tmp.path().join("cache").join("models");
        std::fs::create_dir_all(&models_dir).unwrap();

        let start = Instant::now();
        wait_for_daemon_exit(&models_dir).unwrap();
        // Without a daemon, the first ping returns None and the function exits immediately.
        assert!(start.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn spawn_backoff_exponent_caps_at_six() {
        let tmp = tempfile::tempdir().unwrap();
        let models_dir = tmp.path().join("cache").join("models");
        std::fs::create_dir_all(&models_dir).unwrap();

        // Record 10 consecutive failures to force exponent saturation.
        for i in 0..10 {
            record_spawn_failure(&models_dir, &format!("failure {i}")).unwrap();
        }

        let state = load_spawn_state(&models_dir).unwrap();
        assert_eq!(state.consecutive_failures, 10);

        // Exponent is clamped at 6, so max base_ms is base * 2^6.
        // Effective backoff range is [base/2, base), where base <= base_ms * 64.
        let max_base =
            (DAEMON_SPAWN_BACKOFF_BASE_MS * (1_u64 << 6)).min(DAEMON_AUTO_START_MAX_BACKOFF_MS);
        // The not_before_epoch_ms must not exceed now + max_base (upper bound with jitter < half).
        let now = now_epoch_ms();
        assert!(state.not_before_epoch_ms <= now + max_base);
    }

    #[test]
    fn spawn_backoff_half_jitter_in_range() {
        // Verify the half-jitter formula: result = half + fastrand::u64(0..half)
        // produces values in [half, half + half) == [base/2, base).
        let base_ms: u64 = 100;
        let half = base_ms / 2;
        for _ in 0..100 {
            let jitter = fastrand::u64(0..half);
            let result = half + jitter;
            assert!(result >= half, "result {result} below half {half}");
            assert!(result < base_ms, "result {result} not below base {base_ms}");
        }
    }

    #[test]
    fn to_local_socket_name_produces_valid_result() {
        let result = to_local_socket_name("sqlite-graphrag-test-daemon");
        assert!(result.is_ok(), "expected Ok, got {result:?}");
        // The name string representation must be non-empty.
        let name = result.unwrap();
        let display = format!("{name:?}");
        assert!(!display.is_empty());
    }

    #[test]
    fn version_cas_not_checked_to_compatible() {
        let state = AtomicU8::new(VERSION_NOT_CHECKED);
        let result = state.compare_exchange(
            VERSION_NOT_CHECKED,
            VERSION_COMPATIBLE,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
        assert!(result.is_ok());
        assert_eq!(state.load(Ordering::SeqCst), VERSION_COMPATIBLE);
    }

    #[test]
    fn version_cas_prevents_double_restart() {
        let state = AtomicU8::new(VERSION_NOT_CHECKED);

        // First CAS: NOT_CHECKED → RESTART_ATTEMPTED succeeds.
        let first = state.compare_exchange(
            VERSION_NOT_CHECKED,
            VERSION_RESTART_ATTEMPTED,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
        assert!(first.is_ok());

        // Second CAS from NOT_CHECKED must fail — state is already RESTART_ATTEMPTED.
        let second = state.compare_exchange(
            VERSION_NOT_CHECKED,
            VERSION_RESTART_ATTEMPTED,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
        assert!(second.is_err());
        assert_eq!(state.load(Ordering::SeqCst), VERSION_RESTART_ATTEMPTED);
    }

    #[test]
    fn ping_response_includes_model_fields() {
        let resp = DaemonResponse::Ok {
            pid: 42,
            version: "1.0.0".to_string(),
            handled_embed_requests: 7,
            model_name: "multilingual-e5-small".to_string(),
            model_variant: "fp32".to_string(),
        };
        let json = serde_json::to_value(&resp).expect("serialization failed");
        assert_eq!(json["model_name"], "multilingual-e5-small");
        assert_eq!(json["model_variant"], "fp32");
        assert_eq!(json["status"], "ok");
        assert_eq!(json["handled_embed_requests"], 7u64);
    }

    #[test]
    fn gliner_variant_defaults_to_fp32() {
        // Ensure the default is fp32 when env var is not set.
        std::env::remove_var("SQLITE_GRAPHRAG_GLINER_VARIANT");
        let variant = gliner_variant_from_env();
        assert_eq!(variant, "fp32");
    }

    #[test]
    fn gliner_variant_reads_env_var() {
        std::env::set_var("SQLITE_GRAPHRAG_GLINER_VARIANT", "int8");
        let variant = gliner_variant_from_env();
        std::env::remove_var("SQLITE_GRAPHRAG_GLINER_VARIANT");
        assert_eq!(variant, "int8");
    }

    #[test]
    fn spawn_state_serialization_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let models_dir = tmp.path().join("cache").join("models");
        std::fs::create_dir_all(&models_dir).unwrap();

        let original = DaemonSpawnState {
            consecutive_failures: 3,
            not_before_epoch_ms: 9_999_999_999,
            last_error: Some("test error message".to_string()),
        };
        save_spawn_state(&models_dir, &original).unwrap();

        let loaded = load_spawn_state(&models_dir).unwrap();
        assert_eq!(loaded.consecutive_failures, original.consecutive_failures);
        assert_eq!(loaded.not_before_epoch_ms, original.not_before_epoch_ms);
        assert_eq!(loaded.last_error, original.last_error);
    }
}
