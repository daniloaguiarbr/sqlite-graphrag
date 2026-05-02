//! Semantic chunking for embedding inputs (Markdown-aware, 512-token limit).
//!
//! Splits bodies using [`text_splitter::MarkdownSplitter`] with overlap so
//! multi-chunk memories preserve context across chunk boundaries.

// src/chunking.rs
// Token-based chunking for E5 model (512 token limit)

use crate::constants::{CHUNK_OVERLAP_TOKENS, CHUNK_SIZE_TOKENS, EMBEDDING_DIM};
use text_splitter::{ChunkConfig, MarkdownSplitter};
use tokenizers::Tokenizer;

// Conservative heuristic to reduce the risk of underestimating the real token count
// in Markdown, code, and multilingual text. The previous value (4 chars/token) allowed
// chunks that were too large for some real documents.
/// Characters per token heuristic: 2 chars/token reduces the risk of underestimating
/// real token counts in Markdown, code, and multilingual text.
const CHARS_PER_TOKEN: usize = 2;

/// Maximum character length of a single chunk (derived from token limit × chars-per-token).
pub const CHUNK_SIZE_CHARS: usize = CHUNK_SIZE_TOKENS * CHARS_PER_TOKEN;

/// Character overlap between consecutive chunks to preserve cross-boundary context.
pub const CHUNK_OVERLAP_CHARS: usize = CHUNK_OVERLAP_TOKENS * CHARS_PER_TOKEN;

/// A contiguous slice of a body string identified by byte offsets.
#[derive(Debug, Clone)]
pub struct Chunk {
    /// Byte offset of the first character (inclusive).
    pub start_offset: usize,
    /// Byte offset past the last character (exclusive).
    pub end_offset: usize,
    /// Approximate token count for this chunk (chars / `CHARS_PER_TOKEN`).
    pub token_count_approx: usize,
}

/// Returns `true` when `body` exceeds `CHUNK_SIZE_CHARS` and must be split.
pub fn needs_chunking(body: &str) -> bool {
    body.len() > CHUNK_SIZE_CHARS
}

/// Splits `body` into overlapping [`Chunk`]s using a character-based heuristic.
///
/// Short bodies (≤ `CHUNK_SIZE_CHARS`) are returned as a single chunk.
/// Splits prefer paragraph breaks, then sentence-end punctuation, then word boundaries.
///
/// # Errors
/// This function is infallible; it returns a `Vec` directly.
pub fn split_into_chunks(body: &str) -> Vec<Chunk> {
    if !needs_chunking(body) {
        return vec![Chunk {
            token_count_approx: body.chars().count() / CHARS_PER_TOKEN,
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

        let token_count_approx = body[start..end].chars().count() / CHARS_PER_TOKEN;
        chunks.push(Chunk {
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

/// Splits `body` into [`Chunk`]s using pre-computed token byte-offsets.
///
/// Each element of `token_offsets` is a `(start, end)` byte range for one token.
/// Respects `CHUNK_SIZE_TOKENS` and `CHUNK_OVERLAP_TOKENS` constants.
/// Short bodies (≤ `CHUNK_SIZE_TOKENS` tokens) are returned as a single chunk.
pub fn split_into_chunks_by_token_offsets(
    body: &str,
    token_offsets: &[(usize, usize)],
) -> Vec<Chunk> {
    if token_offsets.len() <= CHUNK_SIZE_TOKENS {
        return vec![Chunk {
            token_count_approx: token_offsets.len(),
            start_offset: 0,
            end_offset: body.len(),
        }];
    }

    let mut chunks = Vec::new();
    let mut start_token = 0usize;

    while start_token < token_offsets.len() {
        let end_token = (start_token + CHUNK_SIZE_TOKENS).min(token_offsets.len());

        chunks.push(Chunk {
            start_offset: if start_token == 0 {
                0
            } else {
                token_offsets[start_token].0
            },
            end_offset: if end_token == token_offsets.len() {
                body.len()
            } else {
                token_offsets[end_token - 1].1
            },
            token_count_approx: end_token - start_token,
        });

        if end_token == token_offsets.len() {
            break;
        }

        let next_start = end_token.saturating_sub(CHUNK_OVERLAP_TOKENS);
        start_token = if next_start <= start_token {
            end_token
        } else {
            next_start
        };
    }

    chunks
}

/// Splits body into chunks using MarkdownSplitter with a real tokenizer.
/// Respects Markdown semantic boundaries (H1-H6, paragraphs, blocks).
/// For plain text without Markdown markers, falls back to paragraph and sentence breaks.
pub fn split_into_chunks_hierarchical(body: &str, tokenizer: &Tokenizer) -> Vec<Chunk> {
    if body.is_empty() {
        return Vec::new();
    }

    let config = ChunkConfig::new(CHUNK_SIZE_TOKENS)
        .with_sizer(tokenizer)
        .with_overlap(CHUNK_OVERLAP_TOKENS)
        .expect(
            "compile-time invariant: CHUNK_OVERLAP_TOKENS must be smaller than CHUNK_SIZE_TOKENS",
        );

    let splitter = MarkdownSplitter::new(config);

    let items: Vec<(usize, &str)> = splitter.chunk_indices(body).collect();

    if items.is_empty() {
        return vec![Chunk {
            start_offset: 0,
            end_offset: body.len(),
            token_count_approx: body.chars().count() / CHARS_PER_TOKEN,
        }];
    }

    items
        .into_iter()
        .map(|(start, text)| {
            let end = start + text.len();
            Chunk {
                start_offset: start,
                end_offset: end,
                token_count_approx: text.chars().count() / CHARS_PER_TOKEN,
            }
        })
        .collect()
}

/// Returns the string slice of `body` described by `chunk`'s byte offsets.
pub fn chunk_text<'a>(body: &'a str, chunk: &Chunk) -> &'a str {
    &body[chunk.start_offset..chunk.end_offset]
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

/// Computes the mean of `chunk_embeddings` and L2-normalizes the result.
///
/// Returns a zero-vector of length `EMBEDDING_DIM` when the input is empty.
/// When a single embedding is provided it is returned as-is (no copy).
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
        assert_eq!(chunk_text(body, &chunks[0]), body);
    }

    #[test]
    fn test_long_body_produces_multiple_chunks() {
        let body = "word ".repeat(1000);
        assert!(needs_chunking(&body));
        let chunks = split_into_chunks(&body);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| !chunk_text(&body, c).is_empty()));
    }

    #[test]
    fn split_by_token_offsets_respeita_limite_e_overlap() {
        let body = "ab".repeat(460);
        let offsets: Vec<(usize, usize)> = (0..460)
            .map(|i| {
                let start = i * 2;
                (start, start + 2)
            })
            .collect();

        let chunks = split_into_chunks_by_token_offsets(&body, &offsets);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].token_count_approx, CHUNK_SIZE_TOKENS);
        assert_eq!(chunks[1].token_count_approx, 110);
        assert_eq!(chunks[0].start_offset, 0);
        assert_eq!(
            chunks[1].start_offset,
            offsets[CHUNK_SIZE_TOKENS - CHUNK_OVERLAP_TOKENS].0
        );
    }

    #[test]
    fn split_by_token_offsets_returns_one_chunk_when_fits() {
        let body = "texto curto";
        let offsets = vec![(0, 5), (6, 11)];
        let chunks = split_into_chunks_by_token_offsets(body, &offsets);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].start_offset, 0);
        assert_eq!(chunks[0].end_offset, body.len());
        assert_eq!(chunks[0].token_count_approx, 2);
    }

    #[test]
    fn test_multibyte_body_preserves_progress_and_boundaries() {
        // Multibyte body intentionally includes 2-byte UTF-8 sequences (Latin-1 supplement)
        // expressed as Unicode escapes so this source file remains ASCII-only per the
        // language policy. The original PT-BR phrase "a\u{e7}\u{e3}o \u{fa}til " is preserved
        // since the test exercises UTF-8 char-boundary handling.
        let body = "a\u{e7}\u{e3}o \u{fa}til ".repeat(1000);
        let chunks = split_into_chunks(&body);
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(!chunk_text(&body, chunk).is_empty());
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

    fn split_hier_chars(body: &str, size: usize) -> Vec<Chunk> {
        use text_splitter::{Characters, ChunkConfig, MarkdownSplitter};
        if body.is_empty() {
            return Vec::new();
        }
        let config = ChunkConfig::new(size)
            .with_sizer(Characters)
            .with_overlap(0)
            .expect("overlap must be smaller than size");
        let splitter = MarkdownSplitter::new(config);
        let items: Vec<(usize, &str)> = splitter.chunk_indices(body).collect();
        if items.is_empty() {
            return vec![Chunk {
                start_offset: 0,
                end_offset: body.len(),
                token_count_approx: body.chars().count() / CHARS_PER_TOKEN,
            }];
        }
        items
            .into_iter()
            .map(|(start, text)| {
                let end = start + text.len();
                Chunk {
                    start_offset: start,
                    end_offset: end,
                    token_count_approx: text.chars().count() / CHARS_PER_TOKEN,
                }
            })
            .collect()
    }

    #[test]
    fn test_hierarchical_empty_body_returns_empty() {
        use text_splitter::{Characters, ChunkConfig, MarkdownSplitter};
        let config = ChunkConfig::new(100)
            .with_sizer(Characters)
            .with_overlap(0)
            .expect("overlap < size");
        let splitter = MarkdownSplitter::new(config);
        let result: Vec<_> = splitter.chunk_indices("").collect();
        assert!(result.is_empty());
    }

    #[test]
    fn test_markdown_h1_boundary_yields_two_chunks() {
        let body = "# Title 1\n\nbody1 body1 body1 body1 body1 body1\n\n# Title 2\n\nbody2 body2 body2 body2 body2 body2";
        let chunks = split_hier_chars(body, 30);
        assert!(
            chunks.len() >= 2,
            "expected >=2 chunks, got {}",
            chunks.len()
        );
        for c in &chunks {
            assert!(body.is_char_boundary(c.start_offset));
            assert!(body.is_char_boundary(c.end_offset));
        }
    }

    #[test]
    fn test_markdown_h2_nested_respects_boundaries() {
        let body = "# H1\n\n## H2a\n\nParagraph A with enough text to force a split.\n\n## H2b\n\nParagraph B with enough text to force a split as well.";
        let chunks = split_hier_chars(body, 40);
        assert!(!chunks.is_empty());
        for c in &chunks {
            assert!(body.is_char_boundary(c.start_offset));
            assert!(body.is_char_boundary(c.end_offset));
            assert!(c.end_offset > c.start_offset);
            assert!(c.end_offset <= body.len());
        }
    }

    #[test]
    fn test_markdown_paragraph_soft_boundary() {
        let para = "Plain text sentence used to fill the paragraph. ";
        let body = format!(
            "{}\n\n{}\n\n{}",
            para.repeat(3),
            para.repeat(3),
            para.repeat(3)
        );
        let chunks = split_hier_chars(&body, 80);
        assert!(
            chunks.len() >= 2,
            "expected >=2 chunks with a body of {} chars",
            body.len()
        );
        for c in &chunks {
            assert!(body.is_char_boundary(c.start_offset));
            assert!(body.is_char_boundary(c.end_offset));
        }
    }

    #[test]
    fn test_markdown_60kb_valid_offsets() {
        let block = "# Section\n\nBlock content text. ".repeat(1700);
        assert!(
            block.len() > 50_000,
            "body must be >50KB, has {} bytes",
            block.len()
        );
        let chunks = split_hier_chars(&block, 256);
        assert!(chunks.len() > 1);
        for c in &chunks {
            assert!(block.is_char_boundary(c.start_offset));
            assert!(block.is_char_boundary(c.end_offset));
            assert!(c.end_offset > c.start_offset);
            assert!(!chunk_text(&block, c).is_empty());
        }
    }

    #[test]
    fn test_fallback_plain_text_without_markers() {
        let body = "a ".repeat(1000);
        let chunks = split_hier_chars(&body, 100);
        assert!(!chunks.is_empty());
        for c in &chunks {
            assert!(body.is_char_boundary(c.start_offset));
            assert!(body.is_char_boundary(c.end_offset));
        }
    }
}
