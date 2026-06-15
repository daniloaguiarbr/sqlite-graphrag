//! v1.0.82 (GAP-001): integration tests for the `pending_memories`
//! staging queue.
//!
//! The full 3-stage `remember` refactor (`validate → embed → commit`
//! with checkpoint resumability) is **deferred to v1.0.83** (see
//! decision memory `decision-v1-0-82-remember-staging-deferred`).
//! v1.0.82 ships the infrastructure (V014 migration, the
//! `pending_memories` table, the `pending` subcommand, and the
//! `pending_memories` DAO) so operators can already inspect the
//! queue and the next release can wire `remember` to use it
//! incrementally.
//!
//! These tests verify the CLI surface of the `pending` subcommand
//! on a fresh database (empty queue) and the schema migration
//! applied by `init`.

#![cfg(feature = "slow-tests")]

use assert_cmd::Command;
use serde_json::Value;
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

/// GAP-001 acceptance: the V014 `pending_memories` table is created
/// by `init` and the schema version is bumped to 15.
#[test]
#[serial]
fn init_creates_pending_memories_table_at_schema_v15() {
    let tmp = TempDir::new().expect("tempdir");
    let out = cmd_base(&tmp)
        .arg("init")
        .arg("--force")
        .output()
        .expect("invoke");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("init must be JSON");
    assert!(
        parsed["schema_version"].as_u64().unwrap() >= 15,
        "schema_version must be ≥ 15 (V014 + V015); got {}",
        parsed["schema_version"]
    );
}

/// GAP-001 acceptance: `pending list` on a fresh database returns
/// the documented envelope with `count: 0` and an empty `entries`
/// array. The 3-stage staging pipeline is wired into `remember`
/// starting in v1.0.83.
#[test]
#[serial]
fn pending_list_returns_empty_queue_on_fresh_db() {
    let tmp = TempDir::new().expect("tempdir");
    cmd_base(&tmp).arg("init").arg("--force").assert().success();
    let out = cmd_base(&tmp)
        .arg("pending")
        .arg("list")
        .output()
        .expect("invoke");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("pending list must be JSON");
    assert_eq!(parsed["action"], "pending_list");
    assert_eq!(parsed["count"], 0);
    assert!(parsed["entries"].is_array());
    assert_eq!(parsed["entries"].as_array().unwrap().len(), 0);
}

/// GAP-001 acceptance: `pending show` of a non-existent pending_id
/// returns a structured error with `code: 4` (NotFound), matching
/// the v1.0.68+ error-envelope contract.
#[test]
#[serial]
fn pending_show_returns_not_found_for_unknown_id() {
    let tmp = TempDir::new().expect("tempdir");
    cmd_base(&tmp).arg("init").arg("--force").assert().success();
    let out = cmd_base(&tmp)
        .arg("pending")
        .arg("show")
        .arg("999999")
        .output()
        .expect("invoke");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("pending show must be JSON");
    assert_eq!(parsed["error"], true);
    assert_eq!(parsed["code"], 4);
}

/// GAP-001 acceptance: `pending cleanup --staged-cleanup-after 0
/// --yes` is a no-op on a fresh database (no abandoned rows) and
/// returns success with the documented JSON envelope.
#[test]
#[serial]
fn pending_cleanup_is_idempotent_on_fresh_db() {
    let tmp = TempDir::new().expect("tempdir");
    cmd_base(&tmp).arg("init").arg("--force").assert().success();
    let out = cmd_base(&tmp)
        .arg("pending")
        .arg("cleanup")
        .arg("--staged-cleanup-after")
        .arg("0")
        .arg("--yes")
        .output()
        .expect("invoke");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("pending cleanup must be JSON");
    assert_eq!(parsed["action"], "pending_cleanup");
    assert!(parsed["removed"].is_number());
    assert!(parsed["candidates"].is_number());
}
