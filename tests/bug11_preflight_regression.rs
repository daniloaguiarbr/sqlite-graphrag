//! Regression tests for BUG-11 CRITICAL (v1.0.88).
//!
//! ADR-0046 / BUG-11: preflight validation failure must NOT allow the
//! `remember` write path to silently persist a memory with a
//! zero-dimensional embedding. Before the fix, the embedding fallback
//! chain ended with `LlmBackendKind::None`, which returned
//! `Ok((Vec::new(), None))` even when every preceding backend
//! (Codex, Claude) had been rejected by preflight. The `remember`
//! command then committed the row with `backend_invoked: "none"`
//! and exit 0, leaving the memory invisible to `recall` and
//! `hybrid-search`.
//!
//! Strategy: each test sets `CLAUDE_CONFIG_DIR` to a TempDir containing
//! a malicious `settings.json` with `mcpServers`, then invokes
//! `remember` and asserts:
//!  1. exit code is NON-ZERO (preferably 11 = embedding error, or 16 = preflight)
//!  2. JSON envelope has `error: true` (not `action: "created"`)
//!  3. The SQLite database contains ZERO memory rows for the test name

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command as AssertCmd;
use serial_test::serial;
use tempfile::TempDir;

#[path = "common/mod.rs"]
mod common;

/// Creates a TempDir with a `settings.json` that declares an active
/// `mcpServers` entry, which trips the `CLAUDE_CONFIG_DIR` preflight
/// guard in `src/spawn/preflight.rs`.
fn make_evil_claude_config() -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("TempDir::new");
    let settings_path = dir.path().join("settings.json");
    let payload = serde_json::json!({
        "mcpServers": {
            "evil-server": {
                "command": "fake-binary",
                "args": ["--exfil"]
            }
        },
        "hooks": {}
    });
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&payload).expect("serialise json"),
    )
    .expect("write settings.json");
    (dir, settings_path)
}

fn mock_llm_cmd() -> AssertCmd {
    let mock_dir = common::mock_llm_path();
    let mut c = AssertCmd::new(assert_cmd::cargo::cargo_bin!("sqlite-graphrag"));
    c.env("PATH", common::prepend_path(&mock_dir));
    c
}

#[test]
#[serial(env)]
fn remember_aborts_when_claude_config_dir_has_active_mcp_v1088() {
    // SAFETY: serial_test::serial(env) serialises env mutations.
    unsafe {
        std::env::set_var("ANTHROPIC_API_KEY", "");
        std::env::remove_var("ANTHROPIC_API_KEY");
        std::env::set_var("OPENAI_API_KEY", "");
        std::env::remove_var("OPENAI_API_KEY");
    }
    let (cfg_dir, _settings_path) = make_evil_claude_config();
    let db_dir = TempDir::new().expect("TempDir::new");
    let db_path = db_dir.path().join("bug11.sqlite");

    let output = mock_llm_cmd()
        .args([
            "remember",
            "--name",
            "bug11-active-mcp",
            "--type",
            "note",
            "--description",
            "x",
            "--body",
            "y",
            "--db",
        ])
        .arg(&db_path)
        .env("CLAUDE_CONFIG_DIR", cfg_dir.path())
        .env("HOME", cfg_dir.path())
        .timeout(std::time::Duration::from_secs(30))
        .output()
        .expect("spawn sqlite-graphrag");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // BUG-11: exit code MUST be non-zero. The fix routes the preflight
    // error through `embed_via_backend_strict` so `remember` aborts.
    assert!(
        !output.status.success(),
        "BUG-11: remember must abort with non-zero exit when preflight rejects. stdout={stdout} stderr={stderr}"
    );

    // The JSON envelope must declare `error: true`, NOT `action: "created"`.
    assert!(
        stdout.contains("\"error\": true"),
        "BUG-11: stdout must contain JSON error envelope, got: {stdout}"
    );
    assert!(
        !stdout.contains("\"action\": \"created\""),
        "BUG-11: stdout must NOT contain action=created, got: {stdout}"
    );
    assert!(
        !stdout.contains("\"backend_invoked\": \"none\""),
        "BUG-11: stdout must NOT silently degrade to backend_invoked=none, got: {stdout}"
    );

    // The SQLite database must contain ZERO memory rows — the buggy
    // behaviour was to commit the row even though embedding failed.
    // Use rusqlite directly to verify since sqlite3 CLI may not be
    // installed in CI environments.
    if db_path.exists() {
        let conn = rusqlite::Connection::open(&db_path).expect("open db");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE name = 'bug11-active-mcp'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        assert_eq!(
            count, 0,
            "BUG-11: zero memory rows should be persisted when preflight fails, found {count}"
        );
    }
}

#[test]
#[serial(env)]
fn remember_succeeds_when_preflight_passes_v1088() {
    // Sanity check: with a clean `CLAUDE_CONFIG_DIR` (no MCP servers),
    // remember must still persist the memory successfully. This guards
    // against over-correction in the BUG-11 fix.
    unsafe {
        std::env::set_var("ANTHROPIC_API_KEY", "");
        std::env::remove_var("ANTHROPIC_API_KEY");
        std::env::set_var("OPENAI_API_KEY", "");
        std::env::remove_var("OPENAI_API_KEY");
    }
    let cfg_dir = TempDir::new().expect("TempDir::new");
    // Empty CLAUDE_CONFIG_DIR is allowed (the fix in BUG-1 v1.0.87
    // accepts non-empty dirs only when settings.json is free of MCP).
    let db_dir = TempDir::new().expect("TempDir::new");
    let db_path = db_dir.path().join("bug11-clean.sqlite");

    let output = mock_llm_cmd()
        .args([
            "remember",
            "--name",
            "bug11-clean-success",
            "--type",
            "note",
            "--description",
            "x",
            "--body",
            "y",
            "--db",
        ])
        .arg(&db_path)
        .env("CLAUDE_CONFIG_DIR", cfg_dir.path())
        .env("HOME", cfg_dir.path())
        .timeout(std::time::Duration::from_secs(30))
        .output()
        .expect("spawn sqlite-graphrag");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "clean CLAUDE_CONFIG_DIR must allow remember to succeed. stdout={stdout}"
    );
    assert!(
        stdout.contains("\"action\": \"created\""),
        "clean path must produce action=created envelope, got: {stdout}"
    );
}
