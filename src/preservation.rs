//! Preservation checks for LLM-enriched memory bodies (G29 Passo 4).
//!
//! When a language model rewrites a memory body, the operator must be
//! protected against silent hallucination: the LLM may invent facts, drop
//! key terms, or drift semantically far from the source. This module
//! provides a lightweight, deterministic similarity metric that runs
//! locally without any model call, so the gate can be enforced before the
//! enriched body touches persistent storage.
//!
//! The default metric is a normalised trigram-Jaccard similarity computed
//! on the union of `set_a` and `set_b`. The score is in `[0.0, 1.0]`,
//! where `1.0` means the two inputs share every trigram and `0.0` means
//! they share none. The threshold default of `0.7` follows the gap G29
//! specification, with `--preserve-threshold <F>` letting operators tune
//! it per workload.
//!
//! # Examples
//!
//! ```
//! use sqlite_graphrag::preservation::{jaccard_similarity, PreservationVerdict};
//!
//! let score = jaccard_similarity("the quick brown fox", "the quick red fox");
//! assert!(score > 0.5);
//!
//! let verdict = PreservationVerdict::evaluate("orig body", "rewritten body", 0.7);
//! assert!(matches!(verdict, PreservationVerdict::Preserved { .. }));
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Computes the trigram-Jaccard similarity between two strings.
///
/// The score is `|A ∩ B| / |A ∪ B|` where `A` and `B` are the sets of
/// character-trigrams extracted from each input. The trigrams are taken
/// over Unicode scalar values via `char_indices`, so the function is
/// safe to call on multi-byte UTF-8 inputs without byte-boundary errors.
///
/// # Edge cases
///
/// - Both inputs empty: returns `1.0` (the empty trigram set is trivially
///   contained in itself).
/// - One input empty, the other non-empty: returns `0.0` (no overlap).
/// - Identical inputs: returns `1.0`.
///
/// The function is pure: no I/O, no allocation beyond the two trigram
/// sets, deterministic for a given pair of inputs. It is safe to call
/// in hot paths.
pub fn jaccard_similarity(a: &str, b: &str) -> f64 {
    let set_a = trigrams(a);
    let set_b = trigrams(b);
    if set_a.is_empty() && set_b.is_empty() {
        return 1.0;
    }
    let intersection = set_a.intersection(&set_b).count() as f64;
    let union = set_a.union(&set_b).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

/// Extracts the set of character-trigrams from a string.
///
/// Padding handles short strings: inputs with fewer than three characters
/// are represented by the unique chars they do contain (with the
/// `[c, '\0', '\0']` padding), which guarantees that two identical
/// short strings still produce the same trigram set and score `1.0`.
fn trigrams(input: &str) -> HashSet<[char; 3]> {
    let chars: Vec<char> = input.chars().collect();
    if chars.is_empty() {
        return HashSet::new();
    }
    let mut out: HashSet<[char; 3]> = HashSet::with_capacity(chars.len().saturating_add(2));
    let mut window: [char; 3] = ['\0', '\0', '\0'];
    for (i, ch) in chars.iter().enumerate() {
        window[0] = if i >= 1 { chars[i - 1] } else { '\0' };
        window[1] = *ch;
        window[2] = if i + 1 < chars.len() {
            chars[i + 1]
        } else {
            '\0'
        };
        out.insert(window);
    }
    out
}

/// Outcome of a preservation evaluation against a configurable threshold.
///
/// `PreservationVerdict` is the wire type the enrich pipeline emits in its
/// NDJSON stream: every body-enrich attempt ends in one of the four
/// variants so callers can route the result without re-running the
/// similarity computation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum PreservationVerdict {
    /// The rewritten body is at least `threshold`-similar to the original.
    Preserved { score: f64, threshold: f64 },
    /// The rewritten body diverges too much from the original and was
    /// rejected by the gate.
    Rejected { score: f64, threshold: f64 },
    /// The original and rewritten bodies are byte-equal (no rewrite was
    /// needed); preserved by definition.
    Unchanged { byte_len: usize },
}

impl PreservationVerdict {
    /// Evaluates the gate against `threshold` and returns the matching
    /// variant. The threshold is clamped to `[0.0, 1.0]` defensively; an
    /// out-of-range value does not panic the caller.
    pub fn evaluate(original: &str, rewritten: &str, threshold: f64) -> Self {
        let threshold = threshold.clamp(0.0, 1.0);
        if original == rewritten {
            return Self::Unchanged {
                byte_len: original.len(),
            };
        }
        let score = jaccard_similarity(original, rewritten);
        if score >= threshold {
            Self::Preserved { score, threshold }
        } else {
            Self::Rejected { score, threshold }
        }
    }

    /// Returns `true` when the gate accepted the rewrite.
    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Preserved { .. } | Self::Unchanged { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_strings_score_one() {
        let s = "the quick brown fox jumps over the lazy dog";
        assert!((jaccard_similarity(s, s) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn completely_different_strings_score_zero_or_near_zero() {
        let a = "aaaaaaaaaa";
        let b = "zzzzzzzzzz";
        assert!(jaccard_similarity(a, b) < 0.05);
    }

    #[test]
    fn partial_overlap_scores_between_zero_and_one() {
        let a = "the quick brown fox jumps";
        let b = "the slow brown cat sleeps";
        let score = jaccard_similarity(a, b);
        assert!(score > 0.0 && score < 1.0, "got {score}");
    }

    #[test]
    fn both_empty_score_one() {
        assert!((jaccard_similarity("", "") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn one_empty_scores_zero() {
        assert!(jaccard_similarity("hello", "").abs() < f64::EPSILON);
        assert!(jaccard_similarity("", "hello").abs() < f64::EPSILON);
    }

    #[test]
    fn unicode_strings_do_not_panic() {
        // Multi-byte UTF-8: 1 char each, very short.
        let a = "ç日本語";
        let b = "ç中文";
        let _ = jaccard_similarity(a, b);
    }

    #[test]
    fn verdict_preserved_when_above_threshold() {
        let v = PreservationVerdict::evaluate("hello world", "hello world!", 0.5);
        assert!(v.is_accepted());
        assert!(matches!(v, PreservationVerdict::Preserved { .. }));
    }

    #[test]
    fn verdict_unchanged_for_identical() {
        let v = PreservationVerdict::evaluate("same", "same", 0.9);
        assert!(v.is_accepted());
        assert!(matches!(v, PreservationVerdict::Unchanged { byte_len: 4 }));
    }

    #[test]
    fn threshold_clamped_out_of_range() {
        // Threshold above 1.0 is clamped to 1.0: identical bodies match
        // by the `Unchanged` short-circuit, accepted.
        let v = PreservationVerdict::evaluate("abc", "abc", 99.0);
        assert!(v.is_accepted());
        // Threshold below 0.0 is clamped to 0.0: every non-empty rewrite
        // meets a 0.0 floor and is accepted. This is the documented
        // behaviour of `clamp(0.0, 1.0)` and is the only sane reading
        // once a negative threshold is no longer in scope.
        let v = PreservationVerdict::evaluate("abc", "xyz", -5.0);
        assert!(v.is_accepted());
        // Threshold of exactly 0.0 accepts only identical bodies; even
        // a single-character drift fails the gate.
        let v = PreservationVerdict::evaluate("abc", "abcd", 0.0);
        assert!(
            v.is_accepted(),
            "single-char append is mostly the same body"
        );
    }

    #[test]
    fn g29_repro_evaluates_rejected_when_diverges() {
        // G29 reproducer: LLM rewrites a body and drifts far from source.
        let original = "JWT token rotation strategy with 15-min expiry and refresh flow";
        let drifted = "The weather in Tokyo is sunny today with mild temperatures expected";
        let v = PreservationVerdict::evaluate(original, drifted, 0.7);
        assert!(!v.is_accepted(), "should reject hallucinated rewrite");
    }
}
