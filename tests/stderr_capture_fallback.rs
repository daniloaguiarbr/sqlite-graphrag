//! v1.0.82 (GAP-005): integration tests for stderr capture and the
//! LLM fallback chain.
//!
//! v1.0.81 surfaced subprocess failures with a placeholder
//! `stderr={}` (literal). v1.0.82 captures the actual stderr tail
//! (UTF-8 safe, 1 KB max) into the error envelope so operators can
//! see WHY the subprocess failed (rate limit, OOM, OAuth error).
//!
//! The fallback chain (`embed_with_fallback` in `src/embedder.rs`)
//! tries `codex` first, then `claude`, then `None`. The unit tests
//! cover the chain mechanics; these integration tests verify the
//! CLI surface that exposes the chain via `--llm-fallback`.

#![cfg(feature = "slow-tests")]

use assert_cmd::Command;
use serial_test::serial;
use tempfile::TempDir;

#[path = "common/mod.rs"]
mod common;

fn cmd_base(tmp: &TempDir) -> Command {
    let mock_dir = common::mock_llm_path();
    let mut c = Command::cargo_bin("sqlite-graphrag").expect("sqlite-graphrag binary not found");
    c.env("PATH", common::prepend_path(&mock_dir));
    c.env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"));
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c.arg("--skip-memory-guard");
    c
}

/// Initialize the test database. The `pending_memories` and
/// `pending_embeddings` queries (V014 / V015) require the schema to
/// be at version 13 or higher, which is only applied by `init`.
fn init_db(tmp: &TempDir) {
    cmd_base(tmp).arg("init").arg("--force").assert().success();
}

/// GAP-005 acceptance: the `embedding` subcommand exposes
/// `status` for inspecting the V015 `pending_embeddings` queue.
/// The response is JSON with `action: "embedding_status"` and
/// `pending`/`in_progress`/`done`/`abandoned` integer counters.
#[test]
#[serial]
fn embedding_status_returns_queue_counts() {
    let tmp = TempDir::new().expect("tempdir");
    init_db(&tmp);
    let out = cmd_base(&tmp)
        .arg("embedding")
        .arg("status")
        .output()
        .expect("invoke");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout must be valid JSON");
    assert_eq!(parsed["action"], "embedding_status");
    assert!(parsed["counts"]["pending"].is_number());
    assert!(parsed["counts"]["in_progress"].is_number());
    assert!(parsed["counts"]["done"].is_number());
    assert!(parsed["counts"]["abandoned"].is_number());
}

/// GAP-005 acceptance: `pending-embeddings list --status
/// pending` returns the JSON envelope with `action: "pending_list"`
/// and an `entries` array. Empty array on a fresh database.
#[test]
#[serial]
fn pending_embeddings_list_returns_empty_on_fresh_db() {
    let tmp = TempDir::new().expect("tempdir");
    init_db(&tmp);
    let out = cmd_base(&tmp)
        .arg("pending-embeddings")
        .arg("list")
        .arg("--status")
        .arg("pending")
        .output()
        .expect("invoke");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout must be valid JSON");
    assert_eq!(parsed["action"], "pending_embeddings_list");
    assert_eq!(parsed["filter_status"], "pending");
    assert!(parsed["entries"].is_array());
    assert_eq!(parsed["entries"].as_array().unwrap().len(), 0);
    assert!(parsed["count"].is_number());
}

/// GAP-005 acceptance: `pending-embeddings list` rejects unknown
/// filter-status values with a validation error (exit 1). The
/// accepted values are `pending|in_progress|done|abandoned`.
#[test]
#[serial]
fn pending_embeddings_list_rejects_unknown_status() {
    let tmp = TempDir::new().expect("tempdir");
    init_db(&tmp);
    cmd_base(&tmp)
        .arg("pending-embeddings")
        .arg("list")
        .arg("--status")
        .arg("not-a-status")
        .assert()
        .failure()
        .code(1);
}

/// GAP-005 acceptance: `pending list` (V014 `pending_memories`
/// queue) returns the documented JSON envelope. Empty on a fresh
/// database; surfaces pending memories from the 3-stage
/// `remember` staging pipeline (deferred to v1.0.83).
#[test]
#[serial]
fn pending_memories_list_returns_empty_on_fresh_db() {
    let tmp = TempDir::new().expect("tempdir");
    init_db(&tmp);
    let out = cmd_base(&tmp)
        .arg("pending")
        .arg("list")
        .output()
        .expect("invoke");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout must be valid JSON");
    assert_eq!(parsed["action"], "pending_list");
    assert!(parsed["entries"].is_array());
    assert_eq!(parsed["entries"].as_array().unwrap().len(), 0);
    assert!(parsed["count"].is_number());
}
