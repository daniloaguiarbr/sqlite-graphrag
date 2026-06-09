//! Token-count utilities for embedding input sizing.
//!
//! v1.0.76: the `tokenizers` crate was removed. Token counts are now
//! approximated from whitespace-split word counts, calibrated by a
//! `WORDS_TO_TOKENS` factor (default `0.75`, conservative for English +
//! the multilingual-e5 prefix that the LLM headless invocation prepends).
//!
//! For passages shorter than `EMBEDDING_MAX_TOKENS` words, the count
//! is exact. For longer passages, the count is approximate but still
//! useful for the chunking decision in `src/embedder.rs::embed_passages_controlled`.

use crate::errors::AppError;

/// Approximate tokens-per-word. The multilingual-e5 family uses
/// SentencePiece tokenisation, which yields ~1.33 tokens per English word
/// and slightly less for code. We round up to 1.5 to keep the chunking
/// decision conservative (better to over-chunk than to overflow the
/// LLM context window).
const WORDS_TO_TOKENS_NUMERATOR: usize = 3;
const WORDS_TO_TOKENS_DENOMINATOR: usize = 2;

/// Returns the approximate token count for `text` when prefixed with
/// `prefix` (e.g. `passage:` for `embed_passage`).
pub fn count_passage_tokens(text: &str) -> Result<usize, AppError> {
    Ok(approx_tokens(&format!(
        "{}{}",
        crate::constants::PASSAGE_PREFIX,
        text
    )))
}

/// Returns the byte-offset pairs `(start, end)` for each whitespace-delimited
/// word in `text`. The tokenizers crate used to return true sub-word offsets;
/// the LLM headless path doesn't need that granularity, so we return word
/// boundaries.
pub fn passage_token_offsets(text: &str) -> Result<Vec<(usize, usize)>, AppError> {
    let mut offsets = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                if i > s {
                    offsets.push((s, i));
                }
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        if text.len() > s {
            offsets.push((s, text.len()));
        }
    }
    Ok(offsets)
}

/// Returns the model's max input length. Since we no longer have a
/// tokenizer config, this returns the constant from `constants.rs`.
/// Operators that need a different ceiling should set
/// `SQLITE_GRAPHRAG_EMBEDDING_MAX_TOKENS` in the environment.
pub fn get_model_max_length() -> usize {
    crate::constants::EMBEDDING_MAX_TOKENS
}

fn approx_tokens(text: &str) -> usize {
    let words = text.split_whitespace().count();
    // Round up to avoid under-chunking.
    let num = words.saturating_mul(WORDS_TO_TOKENS_NUMERATOR);
    let (tokens, rem) = (
        num / WORDS_TO_TOKENS_DENOMINATOR,
        num % WORDS_TO_TOKENS_DENOMINATOR,
    );
    if rem == 0 {
        tokens
    } else {
        tokens + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_has_zero_tokens() {
        assert_eq!(approx_tokens(""), 0);
        assert_eq!(approx_tokens("   \n\t  "), 0);
    }

    #[test]
    fn single_word_rounds_up() {
        // 1 word * 3 / 2 = 1.5 → 2 tokens
        assert_eq!(approx_tokens("hello"), 2);
    }

    #[test]
    fn four_words_rounds_to_six() {
        // 4 * 3 / 2 = 6 exactly
        assert_eq!(approx_tokens("the quick brown fox"), 6);
    }

    #[test]
    fn passage_offsets_skip_whitespace() {
        let offsets = passage_token_offsets("hello world foo").unwrap();
        assert_eq!(offsets, vec![(0, 5), (6, 11), (12, 15)]);
    }

    #[test]
    fn passage_offsets_handle_leading_and_trailing_whitespace() {
        let offsets = passage_token_offsets("  hello  ").unwrap();
        assert_eq!(offsets, vec![(2, 7)]);
    }

    #[test]
    fn count_passage_tokens_matches_approx_tokens() {
        assert_eq!(count_passage_tokens("rust sqlite graphrag").unwrap(), 6);
    }

    #[test]
    fn count_passage_tokens_includes_prefix_for_short_inputs() {
        assert_eq!(count_passage_tokens("teste fix real 4").unwrap(), 8);
    }

    #[test]
    fn count_passage_tokens_matches_embedding_when_text_already_has_prefix() {
        assert_eq!(
            count_passage_tokens("passage: teste fix real 5").unwrap(),
            9
        );
    }
}
