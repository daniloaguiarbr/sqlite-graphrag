//! Tests for the v1.0.75 (G18) adaptive concurrency solution.

use sqlite_graphrag::lock::calculate_safe_concurrency;
use sqlite_graphrag::memory_guard::calculate_safe_concurrency as legacy_calculate;

#[test]
fn safe_concurrency_returns_at_least_one() {
    let n = calculate_safe_concurrency();
    assert!(n >= 1, "safe_concurrency must return at least 1, got {n}");
}

#[test]
fn safe_concurrency_respects_max_ceiling() {
    let n = calculate_safe_concurrency();
    let max_caps = 16usize;
    assert!(
        n <= max_caps,
        "safe_concurrency must respect MAX cap, got {n}"
    );
}

#[test]
fn legacy_formula_no_half_margin() {
    // v1.0.75 (G18): 8000 MB / 1000 MB per worker = 8, min(8 cpus, 8) = 8.
    let permits = legacy_calculate(8_000, 8, 1_000, 16);
    assert_eq!(
        permits, 8,
        "halving margin removed: expected 8, got {permits}"
    );
}

#[test]
fn legacy_formula_llm_worker_budget() {
    // LLM workers cost less RAM, so they get higher concurrency.
    // 64_000 MB / 350 MB = 182, min(8 cpus, 182) = 8.
    let permits = legacy_calculate(64_000, 8, 350, 16);
    assert_eq!(permits, 8);
}

#[test]
fn legacy_formula_never_returns_zero() {
    let permits = legacy_calculate(100, 1, 10_000, 16);
    assert_eq!(permits, 1);
}

#[test]
fn legacy_formula_respects_max_ceiling() {
    let permits = legacy_calculate(128_000, 64, 500, 16);
    assert_eq!(permits, 16);
}
