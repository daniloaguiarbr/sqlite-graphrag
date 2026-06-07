//! Embedding generation for the GraphRAG memory.
//!
//! v1.0.76: the default build is **LLM-only** — the binary does NOT bundle
//! fastembed / ort / ndarray / tokenizers. All embeddings are produced
//! by a headless invocation of `claude code` or `codex` (OAuth, no MCP,
//! no hooks) and stored as a BLOB in `memory_embeddings(memory_id, embedding,
//! source)`. Vector similarity is computed in pure Rust at query time.
//!
//! The legacy fastembed pipeline is still available behind the opt-in
//! `embedding-legacy` feature for the transition window. It is removed
//! in v1.1.0. New code MUST use the LLM path (`embed_passage` /
//! `embed_query` here, which always call the LLM).

use crate::constants::EMBEDDING_DIM;
use crate::errors::AppError;
use crate::extract::llm_embedding::LlmEmbedding;
use parking_lot::Mutex;
use std::path::Path;
use std::sync::OnceLock;

/// Process-wide LLM-embedding client behind a `Mutex`.
///
/// The client is a thin wrapper around a single in-flight `claude code` or
/// `codex` subprocess. Each call blocks until the LLM returns the
/// embedding; the daemon was removed in v1.0.76 to make the CLI one-shot.
static EMBEDDER: OnceLock<Mutex<LlmEmbedding>> = OnceLock::new();

#[cfg(feature = "embedding-legacy")]
mod legacy {
    //! Legacy fastembed wrapper, kept behind the opt-in `embedding-legacy`
    //! feature for the v1.0.76 → v1.1.0 transition window.
    use super::*;
    use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
    use ort::execution_providers::CPUExecutionProvider;
    use parking_lot::Mutex;
    use std::path::Path;
    use std::sync::OnceLock;

    static LEGACY: OnceLock<Mutex<TextEmbedding>> = OnceLock::new();

    pub fn get_legacy_embedder(
        models_dir: &Path,
    ) -> Result<&'static Mutex<TextEmbedding>, AppError> {
        if let Some(e) = LEGACY.get() {
            return Ok(e);
        }
        let model_root = models_dir.to_path_buf();
        let _ = std::fs::create_dir_all(&model_root);

        let init = TextInitOptions::new(EmbeddingModel::MultilingualE5Small)
            .with_cache_dir(model_root)
            .with_execution_providers(vec![CPUExecutionProvider::default().into()])
            .with_max_length(crate::constants::EMBEDDING_MAX_TOKENS);

        let embedder = TextEmbedding::try_new(init).map_err(|e| {
            AppError::Embedding(format!("failed to initialise fastembed TextEmbedding: {e}"))
        })?;
        let _ = LEGACY.set(Mutex::new(embedder));
        Ok(LEGACY.get().expect("LEGACY initialised above"))
    }

    pub fn legacy_embed_passage(text: &str) -> Result<Vec<f32>, AppError> {
        let models_dir = crate::paths::AppPaths::resolve(None)
            .map(|p| p.models)
            .map_err(|e| AppError::Embedding(format!("models_dir resolve failed: {e}")))?;
        let embedder = get_legacy_embedder(&models_dir)?;
        let mut guard = embedder.lock();
        let prefixed = format!("{}{}", crate::constants::PASSAGE_PREFIX, text);
        let docs: [&str; 1] = [prefixed.as_str()];
        let mut embeddings = guard
            .embed(docs, Some(crate::constants::FASTEMBED_BATCH_SIZE))
            .map_err(|e| AppError::Embedding(format!("embed_passage failed: {e}")))?;
        if embeddings.is_empty() {
            return Err(AppError::Embedding(
                "embed_passage returned zero embeddings".into(),
            ));
        }
        Ok(normalise_dim(embeddings.remove(0)))
    }
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

/// Embeds a single passage for storage. Delegates to the configured LLM
/// headless (claude code / codex). Returns a 384-dim f32 vector.
pub fn embed_passage(
    embedder: &Mutex<LlmEmbedding>,
    text: &str,
) -> Result<Vec<f32>, AppError> {
    let mut guard = embedder.lock();
    let result = guard.embed_passage(text)?;
    Ok(normalise_dim(result))
}

/// Embeds a single query for similarity search. Same model and dim as
/// `embed_passage`; the only difference is the LLM-side prompt prefix
/// that the headless invocation uses to disambiguate.
pub fn embed_query(
    embedder: &Mutex<LlmEmbedding>,
    text: &str,
) -> Result<Vec<f32>, AppError> {
    let mut guard = embedder.lock();
    let result = guard.embed_query(text)?;
    Ok(normalise_dim(result))
}

/// Embeds a batch of passages with token-count-aware batching. The
/// `token_counts` are still used to keep the LLM invocation under
/// the per-call context budget, but the count is now an approximation
/// (whitespace-split words) since the `tokenizers` crate was removed.
pub fn embed_passages_controlled(
    embedder: &Mutex<LlmEmbedding>,
    texts: &[&str],
    token_counts: &[usize],
) -> Result<Vec<Vec<f32>>, AppError> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let mut output: Vec<Vec<f32>> = Vec::with_capacity(texts.len());
    let mut group: Vec<&str> = Vec::new();
    let mut current_padded = 0usize;
    for (text, &tokens) in texts.iter().zip(token_counts.iter()) {
        let padded = tokens.saturating_add(8);
        if current_padded + padded > crate::constants::REMEMBER_MAX_CONTROLLED_BATCH_PADDED_TOKENS
            || group.len() >= crate::constants::REMEMBER_MAX_CONTROLLED_BATCH_CHUNKS
        {
            if !group.is_empty() {
                flush_group(&mut output, &mut group, embedder)?;
                current_padded = 0;
            }
        }
        group.push(text);
        current_padded += padded;
    }
    if !group.is_empty() {
        flush_group(&mut output, &mut group, embedder)?;
    }
    Ok(output)
}

fn flush_group(
    output: &mut Vec<Vec<f32>>,
    group: &mut Vec<&str>,
    embedder: &Mutex<LlmEmbedding>,
) -> Result<(), AppError> {
    let mut guard = embedder.lock();
    for text in group.iter() {
        let v = guard.embed_passage(text)?;
        output.push(normalise_dim(v));
    }
    group.clear();
    Ok(())
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
    EMBEDDING_DIM
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
        let out = bytes_to_f32(&bytes);
        assert_eq!(out, input);
    }

    #[test]
    fn normalise_dim_truncates_and_pads() {
        let long = vec![0.0; EMBEDDING_DIM + 10];
        assert_eq!(normalise_dim(long.clone()).len(), EMBEDDING_DIM);
        let short = vec![0.0; 10];
        assert_eq!(normalise_dim(short).len(), EMBEDDING_DIM);
        let exact = vec![0.0; EMBEDDING_DIM];
        assert_eq!(normalise_dim(exact.clone()).len(), EMBEDDING_DIM);
    }

    #[test]
    fn embedding_dim_matches_constant() {
        assert_eq!(embedding_dim(), crate::constants::EMBEDDING_DIM);
    }
}
