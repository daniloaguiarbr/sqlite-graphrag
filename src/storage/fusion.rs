//! RRF (Reciprocal Rank Fusion) utilities shared between `hybrid-search` and
//! `deep-research`.
//!
//! The formula used is the canonical RRF score:
//!
//! ```text
//! score(d) = sum_over_lists { weight * 1 / (rrf_k + rank(d)) }
//! ```
//!
//! where `rank` is 1-indexed position in each ordered list.  The map returned
//! by [`rrf_fuse`] contains un-normalised scores; callers that need a `[0,1]`
//! range should divide by the theoretical maximum:
//!
//! ```text
//! max_possible = sum_over_lists { weight * 1 / (rrf_k + 1) }
//! ```

use std::collections::HashMap;

/// Fuse multiple ranked lists of integer IDs via Reciprocal Rank Fusion.
///
/// Each element of `lists` is `(weight, ranked_ids)` where `ranked_ids` is
/// ordered best-first (index 0 = rank 1).
///
/// Returns a `HashMap<id, combined_score>` using un-normalised RRF scores.
/// Higher score means higher relevance.
///
/// # Examples
///
/// ```
/// use sqlite_graphrag::storage::fusion::rrf_fuse;
///
/// // Two lists with equal weight — item 1 appears in both at rank 1 and 2
/// // so it accumulates more score than item 2 (rank 2) or item 3 (rank 1 only).
/// let knn: Vec<i64> = vec![1, 2];
/// let fts: Vec<i64> = vec![1, 3];
/// let scores = rrf_fuse(&[(1.0, &knn), (1.0, &fts)], 60.0);
/// assert!(scores[&1] > scores[&2]);
/// assert!(scores[&1] > scores[&3]);
/// ```
pub fn rrf_fuse(lists: &[(f64, &Vec<i64>)], rrf_k: f64) -> HashMap<i64, f64> {
    let mut combined: HashMap<i64, f64> = HashMap::new();
    for (weight, ids) in lists {
        for (rank, &id) in ids.iter().enumerate() {
            // rank is 0-indexed here; formula uses 1-indexed, so we add 1.
            let contribution = weight * (1.0 / (rrf_k + rank as f64 + 1.0));
            *combined.entry(id).or_insert(0.0) += contribution;
        }
    }
    combined
}

/// Compute the theoretical maximum RRF score for a given set of weights and
/// `rrf_k`.
///
/// Useful for normalising `rrf_fuse` scores to `[0, 1]`:
///
/// ```
/// use sqlite_graphrag::storage::fusion::{rrf_fuse, rrf_max_possible};
///
/// let weights = vec![1.0_f64, 1.0_f64];
/// let max = rrf_max_possible(&weights, 60.0);
/// assert!(max > 0.0);
/// ```
pub fn rrf_max_possible(weights: &[f64], rrf_k: f64) -> f64 {
    weights.iter().map(|w| w * (1.0 / (rrf_k + 1.0))).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_fuse_single_list_rank_order_preserved() {
        // Items at lower rank index get higher scores.
        let list = vec![10i64, 20, 30];
        let scores = rrf_fuse(&[(1.0, &list)], 60.0);
        assert!(scores[&10] > scores[&20]);
        assert!(scores[&20] > scores[&30]);
    }

    #[test]
    fn rrf_fuse_two_lists_overlap_accumulates() {
        // Item 1 appears first in both lists — must beat item 2 (rank 1 in one list only).
        let knn = vec![1i64, 2];
        let fts = vec![1i64, 3];
        let scores = rrf_fuse(&[(1.0, &knn), (1.0, &fts)], 60.0);
        assert!(scores[&1] > scores[&2], "overlap item must score higher");
        assert!(scores[&1] > scores[&3], "overlap item must score higher");
    }

    #[test]
    fn rrf_fuse_empty_lists_returns_empty() {
        let empty: Vec<i64> = vec![];
        let scores = rrf_fuse(&[(1.0, &empty)], 60.0);
        assert!(scores.is_empty());
    }

    #[test]
    fn rrf_fuse_zero_weight_list_has_no_effect() {
        let list_a = vec![1i64, 2];
        let list_b = vec![3i64, 4];
        let scores_with = rrf_fuse(&[(1.0, &list_a), (0.0, &list_b)], 60.0);
        // Items 3 and 4 should have score 0.0 (or not present).
        assert_eq!(scores_with.get(&3).copied().unwrap_or(0.0), 0.0);
        assert_eq!(scores_with.get(&4).copied().unwrap_or(0.0), 0.0);
    }

    #[test]
    fn rrf_fuse_weights_scale_contribution() {
        // Higher weight means higher score for same rank.
        let list = vec![1i64];
        let low = rrf_fuse(&[(0.5, &list)], 60.0);
        let high = rrf_fuse(&[(2.0, &list)], 60.0);
        assert!(high[&1] > low[&1]);
    }

    #[test]
    fn rrf_max_possible_sums_weights() {
        // With rrf_k=60, max for one list of weight 1.0 is 1/(60+1) ≈ 0.01639.
        let max = rrf_max_possible(&[1.0], 60.0);
        let expected = 1.0 / 61.0;
        assert!((max - expected).abs() < 1e-9);

        // Two equal-weight lists: sum of both.
        let max2 = rrf_max_possible(&[1.0, 1.0], 60.0);
        assert!((max2 - 2.0 / 61.0).abs() < 1e-9);
    }

    #[test]
    fn rrf_fuse_deterministic_for_same_input() {
        let list_a = vec![1i64, 2, 3];
        let list_b = vec![2i64, 1, 4];
        let scores_1 = rrf_fuse(&[(1.0, &list_a), (1.0, &list_b)], 60.0);
        let scores_2 = rrf_fuse(&[(1.0, &list_a), (1.0, &list_b)], 60.0);
        for id in [1i64, 2, 3, 4] {
            assert_eq!(
                scores_1.get(&id).copied().unwrap_or(0.0),
                scores_2.get(&id).copied().unwrap_or(0.0)
            );
        }
    }
}
