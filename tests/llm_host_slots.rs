//! v1.0.82 (GAP-004): integration tests for the LLM host-slot semaphore.
//!
//! The `slots` subcommand exposes the cross-process LLM slot semaphore
//! state. These tests verify the CLI surface and JSON contract rather
//! than the file-locking primitives (which are covered by
//! `tests/loom_lock_slots.rs`).
//!
//! The semaphore has 3 user-visible surfaces:
//! - `slots status --json` — global state of the host (max, active, pids)
//! - `slots release --slot-id N` — explicit release of a stuck slot
//! - `slots cleanup --yes` — purge of orphaned slot files
//!
//! These tests run against the real binary in an isolated temp dir
//! so they exercise the full clap → main → storage path.

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

/// GAP-004 acceptance: `slots status --format json` returns the documented
/// JSON envelope with `action: "slots_status"`, `max` (integer), and
/// `active` (integer ≥ 0). The `pids` field is optional in v1.0.82.
#[test]
#[serial]
fn slots_status_returns_envelope_with_max_and_active() {
    let tmp = TempDir::new().expect("tempdir");
    let out = cmd_base(&tmp)
        .arg("slots")
        .arg("status")
        .arg("--format")
        .arg("json")
        .output()
        .expect("invoke");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout must be valid JSON");
    assert_eq!(parsed["action"], "slots_status");
    assert!(
        parsed["max_concurrency"].is_number(),
        "max_concurrency must be an integer, got: {}",
        parsed["max_concurrency"]
    );
    assert!(
        parsed["active"].is_number(),
        "active must be an integer, got: {}",
        parsed["active"]
    );
    assert!(
        parsed["active"].as_u64().unwrap() <= parsed["max_concurrency"].as_u64().unwrap(),
        "active must be ≤ max_concurrency"
    );
}

/// GAP-004 acceptance: an `ingest` operation increments the active
/// counter briefly, then drops back to zero once the subprocess exits.
/// This is the v1.0.82 guarantee that slot acquisition is RAII-bounded.
/// We verify the post-condition: after a successful `ingest`, the
/// semaphore is back to its baseline state.
#[test]
#[serial]
fn slots_returns_to_baseline_after_ingest() {
    let tmp = TempDir::new().expect("tempdir");
    let fixture = tmp.path().join("fixture.md");
    std::fs::write(&fixture, "# fixture\n\nGAP-004 baseline check.\n").expect("write fixture");

    // Baseline
    let baseline_out = cmd_base(&tmp)
        .arg("slots")
        .arg("status")
        .arg("--format")
        .arg("json")
        .output()
        .expect("invoke");
    let baseline: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&baseline_out.stdout))
            .expect("baseline JSON");
    let baseline_active = baseline["active"].as_u64().unwrap();

    // Ingest (mock LLM returns immediately)
    cmd_base(&tmp)
        .arg("ingest")
        .arg(tmp.path())
        .arg("--type")
        .arg("note")
        .arg("--pattern")
        .arg("*.md")
        .arg("--format")
        .arg("json")
        .assert()
        .success();

    // Post-ingest
    let post_out = cmd_base(&tmp)
        .arg("slots")
        .arg("status")
        .arg("--format")
        .arg("json")
        .output()
        .expect("invoke");
    let post: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&post_out.stdout)).expect("post JSON");
    let post_active = post["active"].as_u64().unwrap();
    assert_eq!(
        post_active, baseline_active,
        "ingest must release slot after subprocess exits (RAII guard)"
    );
}

/// GAP-004 acceptance: `slots release --slot-id 0` for a non-held slot
/// fails with the expected `AppError::NotFound` (exit code 4). The
/// command refuses to operate on slots that are not currently held,
/// preventing accidental clearing of live subprocess leases.
#[test]
#[serial]
fn slots_release_fails_for_unheld_slot() {
    let tmp = TempDir::new().expect("tempdir");
    cmd_base(&tmp)
        .arg("slots")
        .arg("release")
        .arg("--slot-id")
        .arg("0")
        .arg("--yes")
        .assert()
        .failure()
        .code(4);
}

/// GAP-004 acceptance: `slots cleanup --yes` returns success even when
/// no orphan slots are present (idempotent). The JSON envelope
/// reports `removed_count` (integer ≥ 0) and `removed` (array, may be
/// empty).
#[test]
#[serial]
fn slots_cleanup_is_idempotent_with_no_orphans() {
    let tmp = TempDir::new().expect("tempdir");
    let out = cmd_base(&tmp)
        .arg("slots")
        .arg("cleanup")
        .arg("--yes")
        .output()
        .expect("invoke");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout must be valid JSON");
    assert_eq!(parsed["action"], "slots_cleanup");
    assert!(parsed["removed_count"].is_number());
    assert!(parsed["removed"].is_array());
}
