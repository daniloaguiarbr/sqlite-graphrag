use crate::constants::SQLITE_GRAPHRAG_VERSION;
use crate::errors::AppError;
use crate::{embedder, shutdown_requested};
use interprocess::local_socket::{
    prelude::LocalSocketStream,
    traits::{Listener as _, Stream as _},
    GenericFilePath, GenericNamespaced, ListenerNonblockingMode, ListenerOptions, ToFsName,
    ToNsName,
};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

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
    match request_if_available(
        models_dir,
        &DaemonRequest::EmbedPassage {
            text: text.to_string(),
        },
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

pub fn embed_query_or_local(models_dir: &Path, text: &str) -> Result<Vec<f32>, AppError> {
    match request_if_available(
        models_dir,
        &DaemonRequest::EmbedQuery {
            text: text.to_string(),
        },
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

    match request_if_available(models_dir, &request)? {
        Some(DaemonResponse::PassageEmbeddings { embeddings, .. }) => Ok(embeddings),
        Some(DaemonResponse::Error { message }) => Err(AppError::Embedding(message)),
        Some(other) => Err(AppError::Internal(anyhow::anyhow!(
            "unexpected daemon response for batch passage embeddings: {other:?}"
        ))),
        None => {
            let embedder = embedder::get_embedder(models_dir)?;
            embedder::embed_passages_controlled(embedder, texts, token_counts)
        }
    }
}

pub fn run(models_dir: &Path, idle_shutdown_secs: u64) -> Result<(), AppError> {
    let socket = daemon_label(models_dir);
    let name = to_local_socket_name(&socket)?;
    let listener = ListenerOptions::new()
        .name(name)
        .nonblocking(ListenerNonblockingMode::Accept)
        .try_overwrite(true)
        .create_sync()
        .map_err(AppError::Io)?;

    // Warm the model once per daemon process.
    let _ = embedder::get_embedder(models_dir)?;

    crate::output::emit_json(&DaemonResponse::Listening {
        pid: std::process::id(),
        socket,
        idle_shutdown_secs,
    })?;

    let mut handled_embed_requests = 0_u64;
    let mut last_activity = Instant::now();

    loop {
        if shutdown_requested() {
            break;
        }

        match listener.accept() {
            Ok(stream) => {
                last_activity = Instant::now();
                let should_exit = handle_client(stream, models_dir, &mut handled_embed_requests)?;
                if should_exit {
                    break;
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                if last_activity.elapsed() >= Duration::from_secs(idle_shutdown_secs) {
                    tracing::info!(
                        idle_shutdown_secs,
                        handled_embed_requests,
                        "daemon idle timeout reached"
                    );
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(err) => return Err(AppError::Io(err)),
        }
    }

    Ok(())
}

fn handle_client(
    stream: LocalSocketStream,
    models_dir: &Path,
    handled_embed_requests: &mut u64,
) -> Result<bool, AppError> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(AppError::Io)?;

    if line.trim().is_empty() {
        write_response(
            reader.get_mut(),
            &DaemonResponse::Error {
                message: "empty daemon request".to_string(),
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
                handled_embed_requests: *handled_embed_requests,
            },
            false,
        ),
        DaemonRequest::Shutdown => (
            DaemonResponse::ShuttingDown {
                handled_embed_requests: *handled_embed_requests,
            },
            true,
        ),
        DaemonRequest::EmbedPassage { text } => {
            let embedder = embedder::get_embedder(models_dir)?;
            let embedding = embedder::embed_passage(embedder, &text)?;
            *handled_embed_requests += 1;
            (
                DaemonResponse::PassageEmbedding {
                    embedding,
                    handled_embed_requests: *handled_embed_requests,
                },
                false,
            )
        }
        DaemonRequest::EmbedQuery { text } => {
            let embedder = embedder::get_embedder(models_dir)?;
            let embedding = embedder::embed_query(embedder, &text)?;
            *handled_embed_requests += 1;
            (
                DaemonResponse::QueryEmbedding {
                    embedding,
                    handled_embed_requests: *handled_embed_requests,
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
            *handled_embed_requests += 1;
            (
                DaemonResponse::PassageEmbeddings {
                    embeddings,
                    handled_embed_requests: *handled_embed_requests,
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
        return Err(AppError::Embedding("daemon returned empty response".into()));
    }

    let response = serde_json::from_str(line.trim()).map_err(AppError::Json)?;
    Ok(Some(response))
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
