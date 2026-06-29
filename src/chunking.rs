//! Semantic chunking for embedding inputs (Markdown-aware, 512-token limit).
//!
//! Splits bodies using [`text_splitter::MarkdownSplitter`] with overlap so
//! multi-chunk memories preserve context across chunk boundaries.

// src/chunking.rs
// Token-based chunking for E5 model (512 token limit)

use crate::constants::{CHUNK_OVERLAP_TOKENS, CHUNK_SIZE_TOKENS};
use text_splitter::{ChunkConfig, MarkdownSplitter};

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

    let mut chunks = Vec::with_capacity(body.len() / CHUNK_SIZE_CHARS + 1);
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

    let mut chunks = Vec::with_capacity(token_offsets.len() / CHUNK_SIZE_TOKENS + 1);
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
///
/// v1.0.76: the `tokenizer` parameter was removed. The chunker now uses the
/// char-based heuristic (`CHARS_PER_TOKEN = 2`) which is the same heuristic
/// the rest of the codebase uses for `Chunk::token_count_approx`.
// expect_used (audited v1.0.97): overlap < chunk size is a const-derived
// compile-time invariant (CHUNK_OVERLAP_TOKENS < CHUNK_SIZE_TOKENS).
#[allow(clippy::expect_used)]
pub fn split_into_chunks_hierarchical(body: &str) -> Vec<Chunk> {
    if body.is_empty() {
        return Vec::new();
    }

    // v1.0.76: text_splitter 0.30.1's `ChunkConfig::new` defaults to a
    // char-count sizer when no explicit sizer is set. The default chunk
    // size is in characters, not tokens, so we scale `CHUNK_SIZE_TOKENS`
    // by `CHARS_PER_TOKEN` to keep the chunk size roughly equivalent.
    let char_chunk_size = CHUNK_SIZE_TOKENS * CHARS_PER_TOKEN;
    let char_overlap = CHUNK_OVERLAP_TOKENS * CHARS_PER_TOKEN;
    let config = ChunkConfig::new(char_chunk_size)
        .with_overlap(char_overlap)
        .expect("compile-time invariant: CHUNK_OVERLAP must be smaller than chunk size");

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
/// Returns a zero-vector of the active embedding dimensionality when the
/// input is empty. When a single embedding is provided it is returned
/// as-is (no copy).
pub fn aggregate_embeddings(chunk_embeddings: &[Vec<f32>]) -> Vec<f32> {
    if chunk_embeddings.is_empty() {
        return vec![0.0f32; crate::constants::embedding_dim()];
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

/// Budget assessment of a body for the auto-split and dry-run paths
/// (GAP-SG-05/06).
#[derive(Debug, Clone, Copy)]
pub struct BodyBudget {
    /// Total body length in bytes.
    pub bytes: usize,
    /// Conservative cl100k token count of the whole body.
    pub approx_tokens: usize,
    /// Number of embedding chunks the body splits into.
    pub chunk_count: usize,
    /// Number of sub-memories auto-split would produce (1 when the body fits).
    pub partition_count: usize,
    /// True when the body exceeds at least one single-memory budget and would
    /// be auto-split.
    pub exceeds_limits: bool,
}

/// Returns the number of embedding chunks `body` splits into, using the same
/// hierarchical splitter the persistence path uses (GAP-SG-05).
pub fn estimate_chunk_count(body: &str) -> usize {
    split_into_chunks_hierarchical(body).len()
}

/// Returns `true` when `body` fits a single memory without auto-split: under
/// the partition byte budget, the safe chunk ceiling, and the embedding request
/// token ceiling.
fn fits_single_partition(body: &str) -> bool {
    body.len() <= crate::constants::AUTOSPLIT_PARTITION_MAX_BYTES
        && estimate_chunk_count(body) <= crate::constants::REMEMBER_MAX_SAFE_MULTI_CHUNKS
        && crate::tokenizer::count_tokens(body) <= crate::constants::EMBEDDING_REQUEST_MAX_TOKENS
}

/// Assesses `body` against the single-memory budgets (GAP-SG-05/06).
///
/// Used by `ingest --dry-run` to report chunk and token counts and how many
/// sub-memories an auto-split would create.
pub fn assess_body_budget(body: &str) -> BodyBudget {
    let partition_count = split_body_by_sections(body).len();
    BodyBudget {
        bytes: body.len(),
        approx_tokens: crate::tokenizer::count_tokens(body),
        chunk_count: estimate_chunk_count(body),
        partition_count,
        exceeds_limits: partition_count > 1,
    }
}

/// Splits a large `body` into sub-memory partitions at Markdown section
/// boundaries (ATX headers), keeping each partition below the byte, chunk and
/// token budgets (GAP-SG-04/07).
///
/// A body that already fits a single memory is returned unchanged as a single
/// element. Otherwise the body is cut at ATX header lines, sections are greedily
/// packed into partitions under [`crate::constants::AUTOSPLIT_PARTITION_MAX_BYTES`],
/// and any partition still over budget (e.g. one huge headerless section) is
/// hard-sliced on char boundaries. Concatenating the returned partitions
/// reproduces `body` exactly (lossless).
pub fn split_body_by_sections(body: &str) -> Vec<String> {
    if fits_single_partition(body) {
        return vec![body.to_string()];
    }

    let max_bytes = crate::constants::AUTOSPLIT_PARTITION_MAX_BYTES;
    let sections = split_markdown_sections(body);

    let mut packed: Vec<String> = Vec::new();
    let mut current = String::new();
    for section in sections {
        if !current.is_empty() && current.len() + section.len() > max_bytes {
            packed.push(std::mem::take(&mut current));
        }
        current.push_str(&section);
    }
    if !current.is_empty() {
        packed.push(current);
    }

    let mut result = Vec::with_capacity(packed.len());
    for partition in packed {
        if fits_single_partition(&partition) {
            result.push(partition);
        } else {
            result.extend(hard_slice_to_budget(&partition));
        }
    }

    if result.is_empty() {
        vec![body.to_string()]
    } else {
        result
    }
}

/// Cuts `body` into sections, each starting at an ATX Markdown header line and
/// running until the next header. Leading content before the first header is the
/// first section. Sections retain their original text (trailing newlines
/// included) so concatenation is lossless.
fn split_markdown_sections(body: &str) -> Vec<String> {
    let mut sections: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in body.split_inclusive('\n') {
        if is_atx_header(line) && !current.is_empty() {
            sections.push(std::mem::take(&mut current));
        }
        current.push_str(line);
    }
    if !current.is_empty() {
        sections.push(current);
    }
    if sections.is_empty() {
        sections.push(body.to_string());
    }
    sections
}

/// Returns `true` when `line` is an ATX Markdown header: 1..=6 leading `#`
/// followed by a space, tab or line end. Up to leading spaces are tolerated.
fn is_atx_header(line: &str) -> bool {
    let trimmed = line.trim_start_matches(' ');
    let hashes = trimmed.chars().take_while(|&c| c == '#').count();
    if hashes == 0 || hashes > 6 {
        return false;
    }
    let after = &trimmed[hashes..];
    after.is_empty() || after.starts_with(' ') || after.starts_with('\n') || after.starts_with('\t')
}

/// Hard-slices `body` on char boundaries into pieces no larger than the
/// partition byte budget. The fallback for a single Markdown section that alone
/// exceeds the budget; an 80 KiB piece is always under the chunk and token
/// ceilings (see [`crate::constants::AUTOSPLIT_PARTITION_MAX_BYTES`]).
fn hard_slice_to_budget(body: &str) -> Vec<String> {
    let max_bytes = crate::constants::AUTOSPLIT_PARTITION_MAX_BYTES;
    let mut pieces: Vec<String> = Vec::new();
    let mut start = 0usize;
    while start < body.len() {
        let mut end = previous_char_boundary(body, (start + max_bytes).min(body.len()));
        if end <= start {
            end = next_char_boundary(body, start + 1);
        }
        pieces.push(body[start..end].to_string());
        start = end;
    }
    pieces
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

    // ── GAP-SG-04/05/06/07: budget assessment and section auto-split ──

    fn assert_partition_within_limits(p: &str) {
        assert!(
            p.len() <= crate::constants::AUTOSPLIT_PARTITION_MAX_BYTES,
            "partition {} bytes exceeds byte budget",
            p.len()
        );
        assert!(
            estimate_chunk_count(p) <= crate::constants::REMEMBER_MAX_SAFE_MULTI_CHUNKS,
            "partition exceeds chunk budget"
        );
        assert!(
            crate::tokenizer::count_tokens(p) <= crate::constants::EMBEDDING_REQUEST_MAX_TOKENS,
            "partition exceeds token budget"
        );
    }

    #[test]
    fn assess_body_budget_small_body_fits() {
        let budget = assess_body_budget("# Title\n\nshort body");
        assert_eq!(budget.partition_count, 1);
        assert!(!budget.exceeds_limits);
        assert!(budget.chunk_count >= 1);
        assert!(budget.approx_tokens >= 1);
    }

    #[test]
    fn split_body_by_sections_returns_single_for_small_body() {
        let body = "# H\n\nsmall";
        let parts = split_body_by_sections(body);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0], body);
    }

    #[test]
    fn split_body_by_sections_partitions_large_markdown_below_limits() {
        // ~400 sections of ~520 bytes each => ~208 KB, above the 80 KiB budget.
        let mut body = String::new();
        for i in 0..400 {
            body.push_str(&format!("# Section {i}\n\n{}\n\n", "body text ".repeat(50)));
        }
        assert!(body.len() > crate::constants::AUTOSPLIT_PARTITION_MAX_BYTES);

        let parts = split_body_by_sections(&body);
        assert!(
            parts.len() > 1,
            "expected multiple partitions, got {}",
            parts.len()
        );
        for p in &parts {
            assert_partition_within_limits(p);
        }
        // Lossless: concatenation reproduces the original body exactly.
        assert_eq!(parts.concat(), body);
    }

    #[test]
    fn split_body_by_sections_hard_slices_headerless_body() {
        // No ATX headers and far above the byte budget => hard-slice fallback.
        let body = "x".repeat(crate::constants::AUTOSPLIT_PARTITION_MAX_BYTES * 3 + 17);
        let parts = split_body_by_sections(&body);
        assert!(parts.len() > 1);
        for p in &parts {
            assert_partition_within_limits(p);
        }
        assert_eq!(parts.concat(), body);
    }

    #[test]
    fn is_atx_header_recognizes_headers() {
        assert!(is_atx_header("# Title"));
        assert!(is_atx_header("### Sub\n"));
        assert!(is_atx_header("  ## Indented"));
        assert!(!is_atx_header("####### too many"));
        assert!(!is_atx_header("#nospace"));
        assert!(!is_atx_header("plain text"));
    }
}
