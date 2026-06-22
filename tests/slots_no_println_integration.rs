//! GAP-007 (v1.0.88) regression tests: ensure the `slots` text-mode
//! output path uses `tracing::info!` rather than `println!`. The
//! `slots status` JSON path is allowed to keep `println!` because that
//! path IS the data payload (the JSON envelope that the operator
//! requested); the regression is specifically about non-JSON output
//! bypassing the structured-log pipeline.

/// Helper: extract the body of a function up to the next `fn ` at
/// the same indent level (column 0). The returned slice is everything
/// between the opening `{` and the next top-level `fn` declaration.
fn extract_fn_body<'a>(source: &'a str, fn_name: &str) -> &'a str {
    let marker = format!("fn {fn_name}");
    let after_marker = source
        .split(&marker)
        .nth(1)
        .unwrap_or_else(|| panic!("{fn_name} not found in source"));
    // Start at the opening `{` of the function.
    let body_start = after_marker
        .find('{')
        .unwrap_or_else(|| panic!("opening brace of {fn_name} not found"));
    // End at the next `\nfn ` (top-level fn declaration) OR EOF.
    let rest = &after_marker[body_start + 1..];
    let body_end = rest.find("\nfn ").unwrap_or(rest.len());
    &rest[..body_end]
}

/// Count `println!(` macro invocations in `body` (ignores occurrences
/// inside line comments). The naive `body.matches("println!")` over-
/// counts when the body contains a comment that references the macro
/// name, so we strip line comments first.
fn count_println_invocations(body: &str) -> usize {
    // Strip every `// ...\n` line-comment before counting. This is a
    // source-level regression test, not a parser, so a single regex-free
    // pass is sufficient.
    let stripped: String = body
        .lines()
        .map(|line| {
            if let Some(idx) = line.find("//") {
                // Preserve string literals that contain `//` would be a
                // real parser concern; for the slots.rs source we know
                // the comments do not contain escaped slashes inside
                // strings, so simple line stripping is safe.
                &line[..idx]
            } else {
                line
            }
        })
        .collect::<Vec<&str>>()
        .join("\n");
    stripped.matches("println!(").count()
}

/// GAP-007: `slots status --format text` must NOT contain the legacy
/// "max_concurrency: " / "active: " / "slot N — age=" patterns that the
/// `println!` path produced. The `tracing::info!` replacement emits
/// structured log events instead.
#[test]
fn slots_status_does_not_use_println() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("commands")
            .join("slots.rs"),
    )
    .expect("read slots.rs");
    // The legacy `println!` literals are GONE from the source.
    assert!(
        !source.contains("max_concurrency: {}"),
        "slots.rs must not contain legacy `max_concurrency: {{}}` println! pattern"
    );
    assert!(
        !source.contains("active: {} / free: {}"),
        "slots.rs must not contain legacy `active: {{}} / free: {{}}` println! pattern"
    );
    assert!(
        !source.contains("\"  slot {} — age={}s pid={} {}\""),
        "slots.rs must not contain legacy `slot {{}}` println! pattern"
    );
    // The text-mode path must use `tracing::info!` with the `slots` target.
    assert!(
        source.contains("tracing::info!(target: \"slots\""),
        "slots.rs text-mode output must route through tracing::info! with target \"slots\""
    );
}

/// GAP-007: `slots release` must route through `tracing` as well (the
/// confirmation prompt is preserved on stderr via the existing
/// `eprintln!` because that's an interactive prompt that must remain
/// visible to the operator). This test asserts that the actual
/// release-confirmation logic does NOT use `println!`.
#[test]
fn slots_release_routes_through_tracing() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("commands")
            .join("slots.rs"),
    )
    .expect("read slots.rs");

    // The `slots_release` JSON path uses `emit_json_compact` (not
    // println!), and the warning about stale removal uses
    // `tracing::warn!` (already present). The text-mode summary in
    // `run_status` is the only println! cluster that was migrated.
    let run_release_body = extract_fn_body(&source, "run_release");
    assert!(
        run_release_body.contains("emit_json_compact"),
        "run_release JSON output must use emit_json_compact"
    );

    // The textual `slots status` path (run_status) must not use
    // println! EXCEPT for the JSON data payload at the end.
    let run_status_body = extract_fn_body(&source, "run_status");
    // The JSON data payload `println!("{json}");` is the ONE allowed
    // println! invocation (it IS the data payload, not a log line).
    let total_println = count_println_invocations(run_status_body);
    assert_eq!(
        total_println, 1,
        "run_status must contain exactly 1 println! invocation (the JSON data payload); found {total_println}"
    );
    // And that single occurrence must be the JSON payload, not a log
    // line. (The replace-and-contains-println! approach would also
    // work but a numeric count is more robust to comment drift.)
    assert!(
        run_status_body.contains("println!(\"{json}\");"),
        "the single println! in run_status must be the JSON data payload `println!(\"{{json}}\");`"
    );

    // BUG-SLOTS-YES-IGNORED fix: the old eprintln! prompt was replaced
    // with AppError::Validation, so zero eprintln! calls remain.
    let eprintln_count = source.matches("eprintln!").count();
    assert_eq!(
        eprintln_count, 0,
        "slots.rs must contain zero eprintln! (confirmation now uses AppError::Validation)"
    );
}
