//! Integration tests for `recall --all-namespaces` (P0-1).
//!
//! These tests run the binary end-to-end and verify that:
//! - `--all-namespaces` returns results from every namespace,
//! - without `--all-namespaces` the default namespace isolation is preserved,
//! - `--all-namespaces` and `--namespace` are mutually exclusive (exit 2).

#![cfg(feature = "slow-tests")]

use assert_cmd::Command;
use serial_test::serial;
use tempfile::TempDir;

fn cmd(tmp: &TempDir) -> Command {
    let mut c = Command::cargo_bin("sqlite-graphrag").unwrap();
    c.env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"));
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c
}

fn init_db(tmp: &TempDir) {
    cmd(tmp).arg("init").assert().success();
}

fn remember_in_ns(tmp: &TempDir, ns: &str, name: &str, body: &str) {
    cmd(tmp)
        .args([
            "remember",
            "--namespace",
            ns,
            "--name",
            name,
            "--type",
            "user",
            "--description",
            "test memory",
            "--body",
            body,
        ])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// Test 1: --all-namespaces finds memories across multiple namespaces
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn recall_all_namespaces_finds_across_namespaces() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    // Insert one memory in 'global' and one in 'team-a'
    remember_in_ns(
        &tmp,
        "global",
        "global-rust-memory",
        "Rust ownership system ensures memory safety without garbage collection",
    );
    remember_in_ns(
        &tmp,
        "team-a",
        "team-a-rust-memory",
        "Rust borrow checker prevents data races at compile time",
    );

    let output = cmd(&tmp)
        .args(["recall", "--all-namespaces", "--no-graph", "rust memory"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let results = json["results"].as_array().unwrap();

    let namespaces: Vec<&str> = results
        .iter()
        .filter_map(|r| r["namespace"].as_str())
        .collect();

    // Must see results from both namespaces
    assert!(
        namespaces.contains(&"global"),
        "expected 'global' in results, got: {namespaces:?}"
    );
    assert!(
        namespaces.contains(&"team-a"),
        "expected 'team-a' in results, got: {namespaces:?}"
    );
}

// ---------------------------------------------------------------------------
// Test 2: without --all-namespaces, recall isolates to its namespace
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn recall_without_flag_isolates_to_single_namespace() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    remember_in_ns(
        &tmp,
        "global",
        "global-rust-memory",
        "Rust ownership system ensures memory safety without garbage collection",
    );
    remember_in_ns(
        &tmp,
        "team-a",
        "team-a-rust-memory",
        "Rust borrow checker prevents data races at compile time",
    );

    // Query in 'global' namespace only (default or explicit)
    let output = cmd(&tmp)
        .args([
            "recall",
            "--namespace",
            "global",
            "--no-graph",
            "rust memory",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let results = json["results"].as_array().unwrap();

    let namespaces: Vec<&str> = results
        .iter()
        .filter_map(|r| r["namespace"].as_str())
        .collect();

    // Must NOT contain 'team-a'
    assert!(
        !namespaces.contains(&"team-a"),
        "unexpected 'team-a' in global-only results: {namespaces:?}"
    );
}

// ---------------------------------------------------------------------------
// Test 3: --all-namespaces and --namespace are mutually exclusive (exit 2)
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn recall_all_namespaces_conflicts_with_namespace_flag() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    // clap must reject the combination before any DB operation
    cmd(&tmp)
        .args([
            "recall",
            "--all-namespaces",
            "--namespace",
            "global",
            "any query",
        ])
        .assert()
        .failure()
        .code(2);
}
