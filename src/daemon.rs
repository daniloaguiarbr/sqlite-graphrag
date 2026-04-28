use crate::constants::{
    DAEMON_AUTO_START_INITIAL_BACKOFF_MS, DAEMON_AUTO_START_MAX_BACKOFF_MS,
    DAEMON_AUTO_START_MAX_WAIT_MS, DAEMON_IDLE_SHUTDOWN_SECS, DAEMON_PING_TIMEOUT_MS,
    DAEMON_SPAWN_BACKOFF_BASE_MS, DAEMON_SPAWN_LOCK_WAIT_MS, SQLITE_GRAPHRAG_VERSION,
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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    )? {
        Some(DaemonResponse::PassageEmbedding { embedding, .. }) => Ok(embedding),
        Some(DaemonResponse::Error { message }) => Err(AppError::Embedding(message)),
        Some(other) => Err(AppError::Internal(anyhow::anyhow!(
            "resposta inesperada do daemon para embedding de passage: {other:?}"
        ))),
        None => {
            let embedder = embedder::get_embedder(models_dir)?;
            embedder::embed_passage(embedder, text)
        }
    }
}

pub fn embed_query_or_local(models_dir: &Path, text: &str) -> Result<Vec<f32>, AppError> {
    match request_or_autostart(
        models_dir,
        &DaemonRequest::EmbedQuery {
            text: text.to_string(),
        },
    )? {
        Some(DaemonResponse::QueryEmbedding { embedding, .. }) => Ok(embedding),
        Some(DaemonResponse::Error { message }) => Err(AppError::Embedding(message)),
        Some(other) => Err(AppError::Internal(anyhow::anyhow!(
            "resposta inesperada do daemon para embedding de query: {other:?}"
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

    match request_or_autostart(models_dir, &request)? {
        Some(DaemonResponse::PassageEmbeddings { embeddings, .. }) => Ok(embeddings),
        Some(DaemonResponse::Error { message }) => Err(AppError::Embedding(message)),
        Some(other) => Err(AppError::Internal(anyhow::anyhow!(
            "resposta inesperada do daemon para batch de embeddings de passage: {other:?}"
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
                        path = %lock_path.display(),
                        "lock file de spawn removido ao encerrar daemon graciosamente"
                    );
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        path = %lock_path.display(),
                        "falha ao remover lock file de spawn ao encerrar daemon"
                    );
                }
            }
        }
        tracing::info!(
            "daemon encerrado graciosamente; socket será limpo pelo OS ou pelo próximo daemon via try_overwrite"
        );
    }
}

pub fn run(models_dir: &Path, idle_shutdown_secs: u64) -> Result<(), AppError> {
    // Tokio runtime com 2 worker threads para reduzir threads ociosas do daemon.
    // O loop de accept permanece síncrono; cada conexão é despachada para spawn_blocking
    // de forma que embeddings pesados não bloqueiem os workers tokio.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("daemon-worker")
        .enable_all()
        .build()
        .map_err(AppError::Io)?;

    rt.block_on(run_async(models_dir, idle_shutdown_secs))
}

async fn run_async(models_dir: &Path, idle_shutdown_secs: u64) -> Result<(), AppError> {
    let socket = daemon_label(models_dir);
    let name = to_local_socket_name(&socket)?;
    let listener = ListenerOptions::new()
        .name(name)
        .nonblocking(ListenerNonblockingMode::Accept)
        .try_overwrite(true)
        .create_sync()
        .map_err(AppError::Io)?;

    // Guard que limpa o lock file de spawn em encerramento gracioso.
    // SIGKILL não dispara Drop; nesse caso try_overwrite(true) acima é o fallback.
    let _spawn_guard = DaemonSpawnGuard::new(models_dir);

    // Warm the model once per daemon process inside spawn_blocking so the
    // ONNX session initialisation (CPU-bound, may take several seconds) does
    // not block a tokio worker thread.
    let models_dir_warm = models_dir.to_path_buf();
    tokio::task::spawn_blocking(move || embedder::get_embedder(&models_dir_warm).map(|_| ()))
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("model warm-up panicked: {e}")))??;

    crate::output::emit_json(&DaemonResponse::Listening {
        pid: std::process::id(),
        socket,
        idle_shutdown_secs,
    })?;

    let handled_embed_requests = Arc::new(AtomicU64::new(0));
    let mut last_activity = Instant::now();
    let models_dir = models_dir.to_path_buf();

    loop {
        if shutdown_requested() {
            break;
        }

        if !daemon_control_dir(&models_dir).exists() {
            tracing::info!("daemon control directory disappeared; shutting down");
            break;
        }

        match listener.accept() {
            Ok(stream) => {
                last_activity = Instant::now();
                let models_dir_clone = models_dir.clone();
                let counter = Arc::clone(&handled_embed_requests);
                let should_exit = tokio::task::spawn_blocking(move || {
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
                        idle_shutdown_secs,
                        handled_embed_requests = handled_embed_requests.load(Ordering::Relaxed),
                        "daemon idle timeout reached"
                    );
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
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
                message: "requisição vazia ao daemon".to_string(),
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
                    | std::io::ErrorKind::AddrNotAvailable
                    | std::io::ErrorKind::TimedOut
            ) =>
        {
            return Ok(None);
        }
        Err(err) => return Err(AppError::Io(err)),
    };

    serde_json::to_writer(&mut stream, request).map_err(AppError::Json)?;
    stream.write_all(b"\n").map_err(AppError::Io)?;
    stream.flush().map_err(AppError::Io)?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(AppError::Io)?;
    if line.trim().is_empty() {
        return Err(AppError::Embedding("daemon retornou resposta vazia".into()));
    }

    let response = serde_json::from_str(line.trim()).map_err(AppError::Json)?;
    Ok(Some(response))
}

fn request_or_autostart(
    models_dir: &Path,
    request: &DaemonRequest,
) -> Result<Option<DaemonResponse>, AppError> {
    if let Some(response) = request_if_available(models_dir, request)? {
        clear_spawn_backoff_state(models_dir).ok();
        return Ok(Some(response));
    }

    if autostart_disabled() {
        return Ok(None);
    }

    if !ensure_daemon_running(models_dir)? {
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
        tracing::warn!("daemon autostart suppressed by backoff window");
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
            record_spawn_failure(models_dir, format!("current_exe failed: {err}"))?;
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
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    match child.spawn() {
        Ok(_) => {
            let ready = wait_for_daemon_ready(models_dir)?;
            if ready {
                clear_spawn_backoff_state(models_dir).ok();
            } else {
                record_spawn_failure(
                    models_dir,
                    "daemon did not become healthy after autostart".to_string(),
                )?;
            }
            drop(spawn_lock);
            Ok(ready)
        }
        Err(err) => {
            record_spawn_failure(models_dir, format!("daemon spawn failed: {err}"))?;
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

fn autostart_disabled() -> bool {
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

fn record_spawn_failure(models_dir: &Path, message: String) -> Result<(), AppError> {
    let mut state = load_spawn_state(models_dir)?;
    state.consecutive_failures = state.consecutive_failures.saturating_add(1);
    let exponent = state.consecutive_failures.saturating_sub(1).min(6);
    let backoff_ms =
        (DAEMON_SPAWN_BACKOFF_BASE_MS * (1_u64 << exponent)).min(DAEMON_AUTO_START_MAX_BACKOFF_MS);
    state.not_before_epoch_ms = now_epoch_ms() + backoff_ms;
    state.last_error = Some(message);
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

    let path = if cfg!(unix) {
        format!("/tmp/{name}.sock")
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

        record_spawn_failure(&models_dir, "spawn failed".to_string()).unwrap();
        assert!(spawn_backoff_active(&models_dir).unwrap());

        let state = load_spawn_state(&models_dir).unwrap();
        assert_eq!(state.consecutive_failures, 1);
        assert_eq!(state.last_error.as_deref(), Some("spawn failed"));

        clear_spawn_backoff_state(&models_dir).unwrap();
        assert!(!spawn_backoff_active(&models_dir).unwrap());
    }

    #[test]
    fn daemon_control_dir_usa_pai_de_models() {
        let base = PathBuf::from("/tmp/sqlite-graphrag-cache-test");
        let models_dir = base.join("models");
        assert_eq!(daemon_control_dir(&models_dir), base);
    }
}
