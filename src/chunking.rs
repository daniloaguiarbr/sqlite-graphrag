// src/chunking.rs
// Token-based chunking for E5 model (512 token limit)

use crate::constants::{CHUNK_OVERLAP_TOKENS, CHUNK_SIZE_TOKENS, EMBEDDING_DIM};

// Heurística conservadora para reduzir o risco de subestimar o número real de tokens
// em Markdown, código e texto multilíngue. Valor anterior 4 chars/token permitia
// chunks grandes demais para alguns documentos reais.
const CHARS_PER_TOKEN: usize = 2;
pub const CHUNK_SIZE_CHARS: usize = CHUNK_SIZE_TOKENS * CHARS_PER_TOKEN;
pub const CHUNK_OVERLAP_CHARS: usize = CHUNK_OVERLAP_TOKENS * CHARS_PER_TOKEN;

#[derive(Debug, Clone)]
pub struct Chunk {
    pub text: String,
    pub start_offset: usize,
    pub end_offset: usize,
    pub token_count_approx: usize,
}

pub fn needs_chunking(body: &str) -> bool {
    body.len() > CHUNK_SIZE_CHARS
}

pub fn split_into_chunks(body: &str) -> Vec<Chunk> {
    if !needs_chunking(body) {
        return vec![Chunk {
            token_count_approx: body.chars().count() / CHARS_PER_TOKEN,
            text: body.to_string(),
            start_offset: 0,
            end_offset: body.len(),
        }];
    }

    let mut chunks = Vec::new();
    let mut start = 0usize;

    while start < body.len() {
        start = next_char_boundary(body, start);
        let desired_end = previous_char_boundary(body, (start + CHUNK_SIZE_CHARS).min(body.len()));
        let end = if desired_end < body.len() {
            find_split_boundary(body, start, desired_end)
        } else {
            desired_end
        };

        let end = if end <= start {
            let fallback = previous_char_boundary(body, (start + CHUNK_SIZE_CHARS).min(body.len()));
            if fallback > start {
                fallback
            } else {
                body.len()
            }
        } else {
            end
        };

        let text = body[start..end].to_string();
        let token_count_approx = text.chars().count() / CHARS_PER_TOKEN;
        chunks.push(Chunk {
            text,
            start_offset: start,
            end_offset: end,
            token_count_approx,
        });

        if end >= body.len() {
            break;
        }

        let next_start = next_char_boundary(body, end.saturating_sub(CHUNK_OVERLAP_CHARS));
        start = if next_start >= end { end } else { next_start };
    }

    chunks
}

fn find_split_boundary(body: &str, start: usize, desired_end: usize) -> usize {
    let slice = &body[start..desired_end];
    if let Some(pos) = slice.rfind("\n\n") {
        return start + pos + 2;
    }
    if let Some(pos) = slice.rfind(". ") {
        return start + pos + 2;
    }
    if let Some(pos) = slice.rfind(' ') {
        return start + pos + 1;
    }
    desired_end
}

fn previous_char_boundary(body: &str, mut idx: usize) -> usize {
    idx = idx.min(body.len());
    while idx > 0 && !body.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn next_char_boundary(body: &str, mut idx: usize) -> usize {
    idx = idx.min(body.len());
    while idx < body.len() && !body.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

pub fn aggregate_embeddings(chunk_embeddings: &[Vec<f32>]) -> Vec<f32> {
    if chunk_embeddings.is_empty() {
        return vec![0.0f32; EMBEDDING_DIM];
    }
    if chunk_embeddings.len() == 1 {
        return chunk_embeddings[0].clone();
    }

    let dim = chunk_embeddings[0].len();
    let mut mean = vec![0.0f32; dim];
    for emb in chunk_embeddings {
        for (i, v) in emb.iter().enumerate() {
            mean[i] += v;
        }
    }
    let n = chunk_embeddings.len() as f32;
    for v in &mut mean {
        *v /= n;
    }

    let norm: f32 = mean.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-9 {
        for v in &mut mean {
            *v /= norm;
        }
    }
    mean
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_body_no_chunking() {
        let body = "short text";
        assert!(!needs_chunking(body));
        let chunks = split_into_chunks(body);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, body);
    }

    #[test]
    fn test_long_body_produces_multiple_chunks() {
        let body = "word ".repeat(1000);
        assert!(needs_chunking(&body));
        let chunks = split_into_chunks(&body);
        assert!(chunks.len() > 1);
    }

    #[test]
    fn test_multibyte_body_preserves_progress_and_boundaries() {
        let body = "ação útil ".repeat(1000);
        let chunks = split_into_chunks(&body);
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(!chunk.text.is_empty());
            assert!(body.is_char_boundary(chunk.start_offset));
            assert!(body.is_char_boundary(chunk.end_offset));
            assert!(chunk.end_offset > chunk.start_offset);
        }
        for pair in chunks.windows(2) {
            assert!(pair[1].start_offset >= pair[0].start_offset);
            assert!(pair[1].end_offset > pair[0].start_offset);
        }
    }

    #[test]
    fn test_aggregate_embeddings_normalizes() {
        let embs = vec![vec![1.0f32, 0.0], vec![0.0f32, 1.0]];
        let agg = aggregate_embeddings(&embs);
        let norm: f32 = agg.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }
}
