//! GAP-E2E-011: regression tests for `extract_heuristic_description`.
//!
//! These tests exercise the pure-Rust heuristic that derives a memory
//! description from the first meaningful body line, replacing the legacy
//! hardcoded "ingested from <path>" placeholder.

use sqlite_graphrag::commands::ingest_heuristics::extract_heuristic_description;

#[test]
fn auto_describe_uses_body_summary() {
    let body = "\
# Title

This is the actual first sentence of the document that has more than twenty characters.
Second line should be ignored.
";
    let desc = extract_heuristic_description(body, None);
    assert!(
        desc.starts_with("This is the actual"),
        "expected first meaningful line, got: {desc}"
    );
    assert!(
        !desc.starts_with("ingested from"),
        "heuristic must not fall back to legacy placeholder when a meaningful line exists"
    );
}

#[test]
fn auto_describe_falls_back_on_headers_only() {
    // FALTA-6: markdown document containing only headers (no line > 20 chars
    // after trimming, every non-empty line starts with `#`). Without path_hint
    // we fall back to the generic placeholder.
    let body = "\
# Header 1
## Header 2
### Header 3
";
    let desc = extract_heuristic_description(body, None);
    assert_eq!(
        desc, "ingested document",
        "expected fallback to default placeholder when no meaningful line and no path hint"
    );
}

#[test]
fn auto_describe_falls_back_to_stem_when_only_headers() {
    // FALTA-6 follow-up: with a path_hint, fallback uses the file stem
    // (e.g. "headers-only") instead of the generic placeholder.
    let body = "# Header 1\n## Header 2\n";
    let desc = extract_heuristic_description(body, Some("/tmp/headers-only.md"));
    assert_eq!(
        desc, "headers-only",
        "expected path-stem fallback when body has no meaningful line"
    );
}

#[test]
fn auto_describe_truncates_long_line() {
    // Explicit truncation regression: a body with a single long line must
    // produce a description of at most 100 characters.
    let long = "a".repeat(500);
    let body = format!("short\n{long}\nmore");
    let desc = extract_heuristic_description(&body, None);
    assert!(
        desc.chars().count() <= 100,
        "description must be truncated to <= 100 chars, got {} chars",
        desc.chars().count()
    );
    assert_eq!(
        desc,
        "a".repeat(100),
        "truncated description should be 100 a's"
    );
}

#[test]
fn auto_describe_ignores_short_and_blank_lines() {
    // Lines shorter than 21 chars must be skipped, and blank lines must not
    // count as "meaningful". The first qualifying line wins.
    let body = "\
# title


short
This line is more than twenty characters long and should win.
ignored
";
    let desc = extract_heuristic_description(body, None);
    assert!(
        desc.starts_with("This line is more than twenty"),
        "expected long line, got: {desc}"
    );
}
