//! fastembed wrapper and per-process embedding cache.
//!
//! v1.0.75 (G23 solution): when the `llm-only` feature is enabled the
//! legacy fastembed pipeline is replaced by stubs that return a clear
//! `AppError::Validation` error so the operator can migrate without silent
//! failures. The original fastembed implementation is compiled unchanged
//! when `llm-only` is NOT enabled (the default build behaviour).

use crate::constants::{
    EMBEDDING_DIM, EMBEDDING_MAX_TOKENS, FASTEMBED_BATCH_SIZE, PASSAGE_PREFIX, QUERY_PREFIX,
    REMEMBER_MAX_CONTROLLED_BATCH_CHUNKS, REMEMBER_MAX_CONTROLLED_BATCH_PADDED_TOKENS,
};
use crate::errors::AppError;
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
use ort::execution_providers::CPUExecutionProvider;
use parking_lot::Mutex;
use std::path::Path;
use std::sync::OnceLock;

/// Process-wide singleton embedding model behind a `Mutex`.
///
/// ONNX Runtime's `Session` is not guaranteed thread-safe for concurrent
/// inference; `Mutex` serialises all embedding calls.  This is correct by
/// design — without the daemon, embedding throughput is intentionally serial.
///
/// For parallel workloads (enrich, ingest) start the daemon first:
/// `sqlite-graphrag daemon` — the model is loaded once and served via UDS,
/// eliminating Mutex contention across CLI invocations.
static EMBEDDER: OnceLock<Mutex<TextEmbedding>> = OnceLock::new();

/// Returns the process-wide singleton embedder, initializing it on first call.
pub fn get_embedder(models_dir: &Path) -> Result<&'static Mutex<TextEmbedding>, AppError> {
    if let Some(embedder) = EMBEDDER.get() {
        return Ok(embedder);
    }

    let model_root = models_dir.to_path_buf();
    let _ = std::fs::create_dir_all(&model_root);

    let init = TextInitOptions::new(EmbeddingModel::MultilingualE5Small)
        .with_cache_dir(model_root)
        .with_execution_providers(vec![CPUExecutionProvider::default().into()])
        .with_max_length(EMBEDDING_MAX_TOKENS);

    let embedder = TextEmbedding::try_new(init).map_err(|e| {
        AppError::Embedding(format!("failed to initialise fastembed TextEmbedding: {e}"))
    })?;
    let _ = EMBEDDER.set(Mutex::new(embedder));
    Ok(EMBEDDER.get().expect("EMBEDDER initialised above"))
}

pub fn embed_passage(embedder: &Mutex<TextEmbedding>, text: &str) -> Result<Vec<f32>, AppError> {
    let mut guard = embedder.lock();
    let prefixed = format!("{PASSAGE_PREFIX}{text}");
    let docs: [&str; 1] = [prefixed.as_str()];
    let embeddings = guard
        .embed(docs, Some(FASTEMBED_BATCH_SIZE))
        .map_err(|e| AppError::Embedding(format!("embed_passage failed: {e}")))?;
    if embeddings.is_empty() {
        return Err(AppError::Embedding(
            "embed_passage returned zero embeddings".to_string(),
        ));
    }
    Ok(normalise_dim(embeddings.into_iter().next().unwrap()))
}

pub fn embed_query(embedder: &Mutex<TextEmbedding>, text: &str) -> Result<Vec<f32>, AppError> {
    let mut guard = embedder.lock();
    let prefixed = format!("{QUERY_PREFIX}{text}");
    let docs: [&str; 1] = [prefixed.as_str()];
    let embeddings = guard
        .embed(docs, Some(FASTEMBED_BATCH_SIZE))
        .map_err(|e| AppError::Embedding(format!("embed_query failed: {e}")))?;
    if embeddings.is_empty() {
        return Err(AppError::Embedding(
            "embed_query returned zero embeddings".to_string(),
        ));
    }
    Ok(normalise_dim(embeddings.into_iter().next().unwrap()))
}

pub fn embed_passages_controlled(
    embedder: &Mutex<TextEmbedding>,
    texts: &[&str],
    token_counts: &[usize],
) -> Result<Vec<Vec<f32>>, AppError> {
    let mut guard = embedder.lock();

    if texts.is_empty() {
        return Ok(Vec::new());
    }

    let mut groups: Vec<(Vec<usize>, Vec<&str>)> = Vec::new();
    let mut current_padded = 0usize;
    let mut current_group: Vec<&str> = Vec::new();
    let mut current_indices: Vec<usize> = Vec::new();

    for (idx, (text, &tokens)) in texts.iter().zip(token_counts.iter()).enumerate() {
        let padded = tokens.saturating_add(8);
        if current_padded + padded > REMEMBER_MAX_CONTROLLED_BATCH_PADDED_TOKENS
            || current_group.len() >= REMEMBER_MAX_CONTROLLED_BATCH_CHUNKS
        {
            if !current_group.is_empty() {
                groups.push((current_indices.clone(), current_group));
            }
            current_group = Vec::new();
            current_indices = Vec::new();
            current_padded = 0;
        }
        current_group.push(text);
        current_indices.push(idx);
        current_padded += padded;
    }
    if !current_group.is_empty() {
        groups.push((current_indices, current_group));
    }

    let mut output = vec![Vec::new(); texts.len()];
    for (indices, group) in groups {
        let prefixed: Vec<String> = group
            .iter()
            .map(|t| format!("{PASSAGE_PREFIX}{t}"))
            .collect();
        let prefixed_refs: Vec<&str> = prefixed.iter().map(String::as_str).collect();
        let embeddings = guard
            .embed(prefixed_refs.iter(), Some(FASTEMBED_BATCH_SIZE))
            .map_err(|e| AppError::Embedding(format!("embed_passages_controlled failed: {e}")))?;
        for (i, embedding) in embeddings.into_iter().enumerate() {
            let original_idx = indices.get(i).copied().unwrap_or(i);
            if original_idx < output.len() {
                output[original_idx] = normalise_dim(embedding);
            }
        }
    }
    Ok(output)
}

pub fn f32_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

fn normalise_dim(mut v: Vec<f32>) -> Vec<f32> {
    if v.len() == EMBEDDING_DIM {
        return v;
    }
    if v.len() > EMBEDDING_DIM {
        v.truncate(EMBEDDING_DIM);
    } else {
        v.resize(EMBEDDING_DIM, 0.0);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f32_to_bytes_roundtrip() {
        let input = vec![0.0_f32, 1.5, -2.25, f32::MIN, f32::MAX];
        let bytes = f32_to_bytes(&input);
        assert_eq!(bytes.len(), input.len() * 4);
        let mut out = Vec::with_capacity(input.len());
        for chunk in bytes.chunks_exact(4) {
            out.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        assert_eq!(out, input);
    }

    #[test]
    fn f32_to_bytes_empty_input() {
        assert!(f32_to_bytes(&[]).is_empty());
    }
}
