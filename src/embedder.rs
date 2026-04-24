use crate::constants::{
    EMBEDDING_DIM, EMBEDDING_MAX_TOKENS, FASTEMBED_BATCH_SIZE, PASSAGE_PREFIX, QUERY_PREFIX,
    REMEMBER_MAX_CONTROLLED_BATCH_CHUNKS, REMEMBER_MAX_CONTROLLED_BATCH_PADDED_TOKENS,
};
use crate::errors::AppError;
use fastembed::{EmbeddingModel, ExecutionProviderDispatch, TextEmbedding, TextInitOptions};
use ort::execution_providers::CPU;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

static EMBEDDER: OnceLock<Mutex<TextEmbedding>> = OnceLock::new();

/// Returns the process-wide singleton embedder, initializing it on first call.
/// Subsequent calls return the cached instance regardless of `models_dir`.
pub fn get_embedder(models_dir: &Path) -> Result<&'static Mutex<TextEmbedding>, AppError> {
    if let Some(m) = EMBEDDER.get() {
        return Ok(m);
    }

    // Desabilita arena allocator da EP CPU para reduzir retenção agressiva de memória
    // entre inferências repetidas com shapes variáveis. O fastembed já desliga
    // memory pattern em alguns cenários, mas não desliga a CPU arena por padrão.
    let cpu_ep: ExecutionProviderDispatch = CPU::default().with_arena_allocator(false).build();

    let model = TextEmbedding::try_new(
        TextInitOptions::new(EmbeddingModel::MultilingualE5Small)
            .with_execution_providers(vec![cpu_ep])
            .with_max_length(EMBEDDING_MAX_TOKENS)
            .with_show_download_progress(true)
            .with_cache_dir(models_dir.to_path_buf()),
    )
    .map_err(|e| AppError::Embedding(e.to_string()))?;
    // If another thread raced and won, discard our instance and return theirs.
    let _ = EMBEDDER.set(Mutex::new(model));
    Ok(EMBEDDER.get().expect("just set above"))
}

pub fn embed_passage(embedder: &Mutex<TextEmbedding>, text: &str) -> Result<Vec<f32>, AppError> {
    let prefixed = format!("{PASSAGE_PREFIX}{text}");
    let results = embedder
        .lock()
        .map_err(|e| AppError::Embedding(format!("lock poisoned: {e}")))?
        .embed(vec![prefixed.as_str()], Some(1))
        .map_err(|e| AppError::Embedding(e.to_string()))?;
    let emb = results
        .into_iter()
        .next()
        .ok_or_else(|| AppError::Embedding("empty embedding result".into()))?;
    assert_eq!(emb.len(), EMBEDDING_DIM, "unexpected embedding dimension");
    Ok(emb)
}

pub fn embed_query(embedder: &Mutex<TextEmbedding>, text: &str) -> Result<Vec<f32>, AppError> {
    let prefixed = format!("{QUERY_PREFIX}{text}");
    let results = embedder
        .lock()
        .map_err(|e| AppError::Embedding(format!("lock poisoned: {e}")))?
        .embed(vec![prefixed.as_str()], Some(1))
        .map_err(|e| AppError::Embedding(e.to_string()))?;
    let emb = results
        .into_iter()
        .next()
        .ok_or_else(|| AppError::Embedding("empty embedding result".into()))?;
    Ok(emb)
}

pub fn embed_passages_batch(
    embedder: &Mutex<TextEmbedding>,
    texts: &[&str],
    batch_size: usize,
) -> Result<Vec<Vec<f32>>, AppError> {
    let prefixed: Vec<String> = texts
        .iter()
        .map(|t| format!("{PASSAGE_PREFIX}{t}"))
        .collect();
    let strs: Vec<&str> = prefixed.iter().map(String::as_str).collect();
    let results = embedder
        .lock()
        .map_err(|e| AppError::Embedding(format!("lock poisoned: {e}")))?
        .embed(strs, Some(batch_size.min(FASTEMBED_BATCH_SIZE)))
        .map_err(|e| AppError::Embedding(e.to_string()))?;
    for emb in &results {
        assert_eq!(emb.len(), EMBEDDING_DIM, "unexpected embedding dimension");
    }
    Ok(results)
}

pub fn controlled_batch_count(token_counts: &[usize]) -> usize {
    plan_controlled_batches(token_counts).len()
}

pub fn embed_passages_controlled(
    embedder: &Mutex<TextEmbedding>,
    texts: &[&str],
    token_counts: &[usize],
) -> Result<Vec<Vec<f32>>, AppError> {
    if texts.len() != token_counts.len() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "texts/token_counts length mismatch in controlled embedding"
        )));
    }

    let mut results = Vec::with_capacity(texts.len());
    for (start, end) in plan_controlled_batches(token_counts) {
        if end - start == 1 {
            results.push(embed_passage(embedder, texts[start])?);
            continue;
        }

        results.extend(embed_passages_batch(
            embedder,
            &texts[start..end],
            end - start,
        )?);
    }

    Ok(results)
}

/// Embed multiple passages serially.
///
/// This path intentionally avoids ONNX batch inference for robustness when
/// real-world Markdown chunks trigger pathological runtime behavior.
pub fn embed_passages_serial<'a, I>(
    embedder: &Mutex<TextEmbedding>,
    texts: I,
) -> Result<Vec<Vec<f32>>, AppError>
where
    I: IntoIterator<Item = &'a str>,
{
    let iter = texts.into_iter();
    let (lower, _) = iter.size_hint();
    let mut results = Vec::with_capacity(lower);
    for text in iter {
        results.push(embed_passage(embedder, text)?);
    }
    Ok(results)
}

fn plan_controlled_batches(token_counts: &[usize]) -> Vec<(usize, usize)> {
    let mut batches = Vec::new();
    let mut start = 0usize;

    while start < token_counts.len() {
        let mut end = start + 1;
        let mut max_tokens = token_counts[start].max(1);

        while end < token_counts.len() && end - start < REMEMBER_MAX_CONTROLLED_BATCH_CHUNKS {
            let candidate_max = max_tokens.max(token_counts[end].max(1));
            let candidate_len = end + 1 - start;
            if candidate_max * candidate_len > REMEMBER_MAX_CONTROLLED_BATCH_PADDED_TOKENS {
                break;
            }
            max_tokens = candidate_max;
            end += 1;
        }

        batches.push((start, end));
        start = end;
    }

    batches
}

/// Convert &[f32] to &[u8] for sqlite-vec storage.
/// # Safety
/// Safe because f32 has no padding and is well-defined bit pattern.
pub fn f32_to_bytes(v: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::constants::{EMBEDDING_DIM, PASSAGE_PREFIX, QUERY_PREFIX};

    // --- testes de f32_to_bytes (função pura, sem modelo) ---

    #[test]
    fn f32_to_bytes_slice_vazio_retorna_vazio() {
        let v: Vec<f32> = vec![];
        assert_eq!(f32_to_bytes(&v), &[] as &[u8]);
    }

    #[test]
    fn f32_to_bytes_um_elemento_retorna_4_bytes() {
        let v = vec![1.0_f32];
        let bytes = f32_to_bytes(&v);
        assert_eq!(bytes.len(), 4);
        // roundtrip: os 4 bytes devem reconstruir o f32 original
        let recovered = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert_eq!(recovered, 1.0_f32);
    }

    #[test]
    fn f32_to_bytes_comprimento_e_4x_elementos() {
        let v = vec![0.0_f32, 1.0, 2.0, 3.0];
        assert_eq!(f32_to_bytes(&v).len(), v.len() * 4);
    }

    #[test]
    fn f32_to_bytes_zero_codificado_como_4_zeros() {
        let v = vec![0.0_f32];
        assert_eq!(f32_to_bytes(&v), &[0u8, 0, 0, 0]);
    }

    #[test]
    fn f32_to_bytes_roundtrip_vetor_embedding_dim() {
        let v: Vec<f32> = (0..EMBEDDING_DIM).map(|i| i as f32 * 0.001).collect();
        let bytes = f32_to_bytes(&v);
        assert_eq!(bytes.len(), EMBEDDING_DIM * 4);
        // reconstrói e compara primeiro e último elemento
        let first = f32::from_le_bytes(bytes[0..4].try_into().unwrap());
        assert!((first - 0.0_f32).abs() < 1e-6);
        let last_start = (EMBEDDING_DIM - 1) * 4;
        let last = f32::from_le_bytes(bytes[last_start..last_start + 4].try_into().unwrap());
        assert!((last - (EMBEDDING_DIM - 1) as f32 * 0.001).abs() < 1e-4);
    }

    // --- verifica prefixos usados pelo embedder (sem modelo) ---

    #[test]
    fn passage_prefix_nao_vazio() {
        assert_eq!(PASSAGE_PREFIX, "passage: ");
    }

    #[test]
    fn query_prefix_nao_vazio() {
        assert_eq!(QUERY_PREFIX, "query: ");
    }

    #[test]
    fn embedding_dim_e_384() {
        assert_eq!(EMBEDDING_DIM, 384);
    }

    // --- testes com modelo real (ignorados no CI normal) ---

    #[test]
    #[ignore = "requer modelo ~600 MB em disco; executar com --include-ignored"]
    fn embed_passage_retorna_vetor_com_dimensao_correta() {
        let dir = tempfile::tempdir().unwrap();
        let embedder = get_embedder(dir.path()).unwrap();
        let result = embed_passage(embedder, "texto de teste").unwrap();
        assert_eq!(result.len(), EMBEDDING_DIM);
    }

    #[test]
    #[ignore = "requer modelo ~600 MB em disco; executar com --include-ignored"]
    fn embed_query_retorna_vetor_com_dimensao_correta() {
        let dir = tempfile::tempdir().unwrap();
        let embedder = get_embedder(dir.path()).unwrap();
        let result = embed_query(embedder, "consulta de teste").unwrap();
        assert_eq!(result.len(), EMBEDDING_DIM);
    }

    #[test]
    #[ignore = "requer modelo ~600 MB em disco; executar com --include-ignored"]
    fn embed_passages_batch_retorna_um_vetor_por_texto() {
        let dir = tempfile::tempdir().unwrap();
        let embedder = get_embedder(dir.path()).unwrap();
        let textos = ["primeiro", "segundo"];
        let results = embed_passages_batch(embedder, &textos, 2).unwrap();
        assert_eq!(results.len(), 2);
        for emb in &results {
            assert_eq!(emb.len(), EMBEDDING_DIM);
        }
    }

    #[test]
    fn controlled_batch_plan_respeita_orcamento() {
        assert_eq!(
            plan_controlled_batches(&[100, 100, 100, 100, 300, 300]),
            vec![(0, 4), (4, 5), (5, 6)]
        );
    }

    #[test]
    fn controlled_batch_count_retorna_um_para_chunk_unico() {
        assert_eq!(controlled_batch_count(&[350]), 1);
    }
}
