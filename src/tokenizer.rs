//! Token-count utilities for embedding input sizing.
//!
//! Provides fast approximate token counting used to decide whether a body
//! fits in a single chunk or requires the multi-chunk splitter.

use crate::constants::PASSAGE_PREFIX;
use crate::errors::AppError;
use fastembed::{EmbeddingModel, TextEmbedding};
use huggingface_hub::api::sync::ApiBuilder;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tokenizers::Tokenizer;

struct TokenizerRuntime {
    tokenizer: Tokenizer,
    model_max_length: usize,
}

static TOKENIZER_RUNTIME: OnceLock<TokenizerRuntime> = OnceLock::new();

pub fn get_tokenizer(models_dir: &Path) -> Result<&'static Tokenizer, AppError> {
    Ok(&get_runtime(models_dir)?.tokenizer)
}

pub fn get_model_max_length(models_dir: &Path) -> Result<usize, AppError> {
    Ok(get_runtime(models_dir)?.model_max_length)
}

pub fn count_passage_tokens(tokenizer: &Tokenizer, text: &str) -> Result<usize, AppError> {
    let prefixed = format!("{PASSAGE_PREFIX}{text}");
    count_tokens(tokenizer, &prefixed)
}

pub fn passage_token_offsets(
    tokenizer: &Tokenizer,
    text: &str,
) -> Result<Vec<(usize, usize)>, AppError> {
    let prefixed = format!("{PASSAGE_PREFIX}{text}");
    let prefix_len = PASSAGE_PREFIX.len();
    let encoding = tokenizer
        .encode(prefixed, true)
        .map_err(|e| AppError::Embedding(e.to_string()))?;

    let mut offsets = Vec::new();
    for &(start, end) in encoding.get_offsets() {
        if end <= start || end <= prefix_len {
            continue;
        }

        let adjusted_start = start.saturating_sub(prefix_len).min(text.len());
        let adjusted_end = end.saturating_sub(prefix_len).min(text.len());

        if adjusted_end > adjusted_start
            && text.is_char_boundary(adjusted_start)
            && text.is_char_boundary(adjusted_end)
        {
            offsets.push((adjusted_start, adjusted_end));
        }
    }

    if offsets.is_empty() && !text.is_empty() {
        offsets.push((0, text.len()));
    }

    Ok(offsets)
}

fn count_tokens(tokenizer: &Tokenizer, text: &str) -> Result<usize, AppError> {
    let encoding = tokenizer
        .encode(text, true)
        .map_err(|e| AppError::Embedding(e.to_string()))?;
    Ok(encoding.len())
}

fn get_runtime(models_dir: &Path) -> Result<&'static TokenizerRuntime, AppError> {
    if let Some(runtime) = TOKENIZER_RUNTIME.get() {
        return Ok(runtime);
    }

    let runtime = load_runtime(models_dir)?;
    let _ = TOKENIZER_RUNTIME.set(runtime);
    Ok(TOKENIZER_RUNTIME
        .get()
        .expect("tokenizer runtime just initialized"))
}

fn load_runtime(models_dir: &Path) -> Result<TokenizerRuntime, AppError> {
    let model_info = TextEmbedding::get_model_info(&EmbeddingModel::MultilingualE5Small)
        .map_err(|e| AppError::Embedding(e.to_string()))?;

    let cache_dir = std::env::var("HF_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| models_dir.to_path_buf());
    let endpoint =
        std::env::var("HF_ENDPOINT").unwrap_or_else(|_| "https://huggingface.co".to_string());

    let api = ApiBuilder::new()
        .with_cache_dir(cache_dir)
        .with_endpoint(endpoint)
        .with_progress(false)
        .build()
        .map_err(|e| AppError::Embedding(e.to_string()))?;
    let repo = api.model(model_info.model_code.clone());

    let tokenizer_bytes =
        std::fs::read(repo.get("tokenizer.json").map_err(map_hf_err)?).map_err(AppError::Io)?;
    let tokenizer_config_bytes =
        std::fs::read(repo.get("tokenizer_config.json").map_err(map_hf_err)?)
            .map_err(AppError::Io)?;

    let tokenizer =
        Tokenizer::from_bytes(tokenizer_bytes).map_err(|e| AppError::Embedding(e.to_string()))?;
    let tokenizer_config: serde_json::Value =
        serde_json::from_slice(&tokenizer_config_bytes).map_err(AppError::Json)?;
    let model_max_length = tokenizer_config["model_max_length"]
        .as_u64()
        .map(|n| n as usize)
        .or_else(|| {
            tokenizer_config["model_max_length"]
                .as_f64()
                .map(|n| n as usize)
        })
        .ok_or_else(|| AppError::Embedding("tokenizer_config.json sem model_max_length".into()))?;

    Ok(TokenizerRuntime {
        tokenizer,
        model_max_length,
    })
}

fn map_hf_err(err: huggingface_hub::api::sync::ApiError) -> AppError {
    AppError::Embedding(err.to_string())
}
