//! GAP-005 (v1.0.88) integration tests for the deterministic-order
//! guarantee of `rrf_fuse` after the `HashMap` → `BTreeMap` switch.
//!
//! Both tests below depend on the `BTreeMap` iteration order being
//! sorted by key. If a future refactor re-introduces `HashMap`, these
//! tests would still pass (both maps produce the same entries) but the
//! `serializes_to_canonical_json` test would catch the case where the
//! iteration order drifts (e.g. a hybrid `HashMap` + manual sort path).

use sqlite_graphrag::storage::fusion::rrf_fuse;

/// GAP-005: two consecutive calls to `rrf_fuse` with the same input
/// must return the same iteration order. The BTreeMap guarantee makes
/// this automatic (sort by id) but the assertion here documents the
/// contract so a regression to `HashMap` would still hold numerically
/// even if iteration order drifted.
#[test]
fn rrf_fuse_produces_deterministic_order() {
    let list_a = vec![5i64, 1, 3];
    let list_b = vec![2i64, 4, 1];
    let first: Vec<(i64, f64)> = rrf_fuse(&[(1.0, &list_a), (1.0, &list_b)], 60.0)
        .into_iter()
        .collect();
    let second: Vec<(i64, f64)> = rrf_fuse(&[(1.0, &list_a), (1.0, &list_b)], 60.0)
        .into_iter()
        .collect();
    assert_eq!(
        first, second,
        "two rrf_fuse invocations with the same input must produce identical order"
    );
    // And the order must be sorted by id (the BTreeMap contract).
    let ids: Vec<i64> = first.iter().map(|(id, _)| *id).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(
        ids, sorted,
        "rrf_fuse iteration must be sorted by id (BTreeMap contract)"
    );
}

/// GAP-005: serialising the `rrf_fuse` result to JSON must produce a
/// canonical byte sequence. This is the property that downstream
/// pipelines (cache, log diffing, snapshot tests) depend on for stable
/// snapshots.
#[test]
fn rrf_fuse_serializes_to_canonical_json() {
    let list_a = vec![5i64, 1, 3];
    let list_b = vec![2i64, 4, 1];
    let s1 = rrf_fuse(&[(1.0, &list_a), (1.0, &list_b)], 60.0);
    let s2 = rrf_fuse(&[(1.0, &list_a), (1.0, &list_b)], 60.0);
    let j1 = serde_json::to_string(&s1).expect("serialize s1");
    let j2 = serde_json::to_string(&s2).expect("serialize s2");
    assert_eq!(j1, j2, "serialised rrf_fuse output must be byte-identical");

    // Sanity: the serialised order must reflect ascending id keys.
    // serde_json serialises BTreeMap as an object with keys in
    // ascending order; we verify the first key is the smallest id.
    let first_key = s1.keys().next().copied().expect("non-empty");
    let min_id = *s1.keys().min().expect("non-empty");
    assert_eq!(
        first_key, min_id,
        "serialised BTreeMap must start with the smallest id"
    );
}
