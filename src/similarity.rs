//! Cosine similarity and ranking helpers for the in-process vector
//! search introduced in v1.0.76.
//!
//! v1.0.76: the `sqlite-vec` extension was removed. Vector similarity is
//! computed in pure Rust on the BLOB embeddings stored in
//! `memory_embeddings`, `entity_embeddings`, and `chunk_embeddings`. The
//! performance characteristics are O(N × D) per call where N is the
//! number of rows in the embedding table and D is the embedding
//! dimensionality (default 384). This is acceptable for the
//! tens-of-thousands scale that the GraphRAG memory is designed for;
//! operators with million-scale corpora should partition by namespace
//! and rely on FTS5 for coarse filtering before reaching these helpers.

/// Cosine similarity in the range `[-1.0, 1.0]`. Returns 0.0 when
/// either vector has zero norm. Inputs are NOT mutated.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0_f32;
    let mut norm_a = 0.0_f32;
    let mut norm_b = 0.0_f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    let denom = (norm_a * norm_b).sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

/// Converts a cosine similarity to a "distance" in `[0.0, 2.0]` so the
/// result is compatible with the previous sqlite-vec KNN API. Existing
/// recall / hybrid-search code that interprets `distance` as "lower is
/// better" can keep doing so without code changes.
pub fn similarity_to_distance(sim: f32) -> f32 {
    1.0 - sim
}

/// Returns the top-`k` `(index, score)` pairs sorted by `score`
/// descending. Stable for ties. `O(N log k)` via a simple sort.
pub fn top_k_by_score<I>(items: I, k: usize) -> Vec<(usize, f32)>
where
    I: IntoIterator<Item = f32>,
{
    let mut scored: Vec<(usize, f32)> = items
        .into_iter()
        .enumerate()
        .map(|(i, s)| (i, s))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(k);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_vectors_have_similarity_one() {
        let v = vec![0.5, 0.5, 0.5, 0.5];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn orthogonal_vectors_have_similarity_zero() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn opposite_vectors_have_similarity_minus_one() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        assert!((cosine_similarity(&a, &b) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn zero_vector_returns_zero() {
        let zero = vec![0.0, 0.0, 0.0];
        let v = vec![1.0, 2.0, 3.0];
        assert_eq!(cosine_similarity(&zero, &v), 0.0);
        assert_eq!(cosine_similarity(&v, &zero), 0.0);
    }

    #[test]
    fn mismatched_lengths_return_zero() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn similarity_to_distance_inverts_correctly() {
        assert!((similarity_to_distance(1.0) - 0.0).abs() < 1e-6);
        assert!((similarity_to_distance(0.0) - 1.0).abs() < 1e-6);
        assert!((similarity_to_distance(-1.0) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn top_k_returns_sorted_truncated() {
        let items = vec![0.1, 0.9, 0.5, 0.3, 0.7];
        let top = top_k_by_score(items, 3);
        assert_eq!(top, vec![(1, 0.9), (4, 0.7), (2, 0.5)]);
    }
}
