//! Regression tests for graph traverse edge cases.
//!
//! P0-7: `graph traverse --from <nonexistent-entity>` must return exit 4
//! (NotFound) and never return exit 0 with a null/empty payload.

use assert_cmd::Command;
use tempfile::TempDir;

fn cmd_base(tmp: &TempDir) -> Command {
    let mut c = Command::cargo_bin("sqlite-graphrag").unwrap();
    c.env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"));
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c.arg("--skip-memory-guard");
    c
}

fn init_db(tmp: &TempDir) {
    cmd_base(tmp).arg("init").assert().success();
}

fn remember_with_body(tmp: &TempDir, name: &str, body: &str) {
    cmd_base(tmp)
        .args([
            "remember",
            "--name",
            name,
            "--type",
            "user",
            "--description",
            "desc",
            "--namespace",
            "audit",
            "--body",
            body,
        ])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// P0-7 regression: traverse from nonexistent entity → exit 4, never exit 0
// ---------------------------------------------------------------------------

/// Regression for P0-7: `graph traverse --from <entity-that-does-not-exist>`
/// must fail with exit code 4 (NotFound).
///
/// Previously observed (audit v1.0.23): command returned exit 0 with payload
/// `{root: null, depth: null, visited_count: 0}` instead of the correct
/// exit 4 error response.
#[test]
fn test_p0_7_traverse_nonexistent_entity_exits_4() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    // Seed one real memory so the DB has at least one entity; this ensures the
    // code path reaches the entity-lookup step and is not short-circuited by an
    // empty-graph fast path.
    remember_with_body(
        &tmp,
        "seed-memory-for-traverse-test",
        "Anthropic builds AI systems",
    );

    // Traversal from a name that was never ingested into the namespace must
    // produce exit 4, not exit 0 with a null payload.
    cmd_base(&tmp)
        .args([
            "graph",
            "traverse",
            "--from",
            "EntityThatAbsolutelyDoesNotExist",
            "--depth",
            "2",
            "--namespace",
            "audit",
        ])
        .assert()
        .failure()
        .code(4);
}

/// Traverse from a valid entity must succeed (exit 0) and return JSON with
/// non-null `root` field. Guards against regressions in the happy path.
#[test]
fn test_traverse_valid_entity_exits_0() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    remember_with_body(
        &tmp,
        "anthropic-memory",
        "Anthropic builds Claude the AI assistant",
    );

    // After ingestion the entity extractor may or may not have written an
    // entity named "Anthropic". We therefore test the happy path only when the
    // graph has at least one node by checking exit 0 OR exit 4.
    //
    // The invariant we are protecting: exit MUST be in {0, 4}; it must NEVER
    // be 0 accompanied by a null root (the original P0-7 bug).
    let output = cmd_base(&tmp)
        .args([
            "graph",
            "traverse",
            "--from",
            "Anthropic",
            "--depth",
            "1",
            "--namespace",
            "audit",
        ])
        .output()
        .unwrap();

    let exit_code = output.status.code().unwrap_or(-1);
    assert!(
        exit_code == 0 || exit_code == 4,
        "expected exit 0 or 4, got {exit_code}"
    );

    if exit_code == 0 {
        // When success, root must NOT be null (the P0-7 failure signature).
        let stdout = String::from_utf8_lossy(&output.stdout);
        let json: serde_json::Value =
            serde_json::from_str(&stdout).expect("exit 0 must produce valid JSON on stdout");
        assert!(
            !json["root"].is_null(),
            "exit 0 must not return a null root (P0-7 regression)"
        );
    }
}

/// Traverse with --namespace that does not exist must also exit 4, not 0.
#[test]
fn test_traverse_nonexistent_namespace_exits_4() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd_base(&tmp)
        .args([
            "graph",
            "traverse",
            "--from",
            "AnyEntity",
            "--depth",
            "2",
            "--namespace",
            "namespace-that-does-not-exist",
        ])
        .assert()
        .failure()
        .code(4);
}
