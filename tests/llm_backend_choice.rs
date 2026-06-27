//! v1.0.82 (GAP-003): integration tests for the `--llm-backend` flag
//! propagation across the 6 write/read paths (`remember`, `edit`,
//! `ingest`, `enrich`, `recall`, `hybrid-search`).
//!
//! The flag is a global `Cli` flag added in v1.0.82 (GAP-003). Each
//! command accepts `LlmBackendChoice::{Auto,Claude,Codex,None}` and
//! routes the embedding call through `embedder::embed_with_fallback`
//! or `embedder::try_embed_query_with_choice`.
//!
//! These tests verify the `None` path (which short-circuits the LLM
//! and returns an empty vector) because the mock LLM cannot reliably
//! emit deterministic vectors across releases — the `None` path is
//! the only one that produces a deterministic, reproducible outcome
//! without OAuth.

#![cfg(feature = "slow-tests")]

use assert_cmd::Command;
use serde_json::Value;
use serial_test::serial;
use tempfile::TempDir;

#[path = "common/mod.rs"]
mod common;

/// Builds a fresh `Command` with the mock LLM PATH prepended so any
/// accidental fallback to `codex`/`claude` (rather than `none`) does
/// not crash the test — the mock returns a fixed 64-dim zero vector.
fn sgr_cmd() -> Command {
    let mock_dir = common::mock_llm_path();
    let mut c = Command::cargo_bin("sqlite-graphrag").expect("sqlite-graphrag binary not found");
    c.env("PATH", common::prepend_path(&mock_dir));
    c
}

fn cmd_base(tmp: &TempDir) -> Command {
    let mut c = sgr_cmd();
    c.env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"));
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c.arg("--skip-memory-guard");
    c
}

/// BUG-11 (v1.0.88, ADR-0046) contract: a fallback chain of only
/// `[None]` without `skip_on_failure` MUST abort rather than persist a
/// memory with an invisible zero-dimensional embedding. So `remember
/// --llm-backend none` with no embedding backend available fails with
/// exit 11 and "no LLM backends available; fallback chain exhausted".
/// This supersedes the pre-BUG-11 GAP-003 (v1.0.82) contract, which
/// expected a silent empty-embedding success.
#[test]
#[serial]
fn llm_backend_none_without_embedding_aborts_exit11() {
    let tmp = TempDir::new().expect("tempdir");
    let assert = cmd_base(&tmp)
        .arg("remember")
        .arg("--name")
        .arg("smoke-none")
        .arg("--type")
        .arg("note")
        .arg("--description")
        .arg("BUG-11 none backend")
        .arg("--body")
        .arg("body without LLM call")
        .arg("--llm-backend")
        .arg("none")
        .arg("--json")
        .assert()
        .failure()
        .code(11);
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.contains("no LLM backends available"),
        "expected the BUG-11 fallback-exhausted guard, got stderr: {stderr}"
    );
}

/// GAP-003 acceptance: `--llm-backend=codex` is accepted on the CLI
/// surface and the value round-trips through the `LlmBackendChoice`
/// parser. The actual fallback chain is exercised by the unit tests
/// in `src/embedder.rs`; the integration test only confirms the flag
/// is wired into the command and the response JSON parses.
#[test]
#[serial]
fn llm_backend_codex_is_accepted_on_command_line() {
    let tmp = TempDir::new().expect("tempdir");
    let out = cmd_base(&tmp)
        .arg("remember")
        .arg("--name")
        .arg("smoke-codex")
        .arg("--type")
        .arg("note")
        .arg("--description")
        .arg("GAP-003 codex backend")
        .arg("--body")
        .arg("body via mock codex")
        .arg("--llm-backend")
        .arg("codex")
        .arg("--json")
        .output()
        .expect("invoke");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: Result<Value, _> = serde_json::from_str(&stdout);
    assert!(parsed.is_ok(), "stdout must be valid JSON, got: {stdout}");
}

/// BUG-11 (v1.0.88, ADR-0046) contract via env var: the
/// `SQLITE_GRAPHRAG_LLM_BACKEND=none` precedence still routes to the
/// `None` backend, which — with no embedding backend available — aborts
/// with exit 11 instead of persisting a zero-dimensional embedding.
/// Confirms the precedence (CLI flag > env var > default `auto`) reaches
/// the same BUG-11 guard as the explicit flag.
#[test]
#[serial]
fn llm_backend_none_via_env_var_aborts() {
    let tmp = TempDir::new().expect("tempdir");
    let assert = cmd_base(&tmp)
        .env("SQLITE_GRAPHRAG_LLM_BACKEND", "none")
        .arg("remember")
        .arg("--name")
        .arg("smoke-env-none")
        .arg("--type")
        .arg("note")
        .arg("--description")
        .arg("BUG-11 env override")
        .arg("--body")
        .arg("body via env var")
        .arg("--json")
        .assert()
        .failure()
        .code(11);
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.contains("no LLM backends available"),
        "expected the BUG-11 fallback-exhausted guard, got stderr: {stderr}"
    );
}

/// GAP-003 acceptance: invalid values are rejected at clap parse time
/// with exit code 2 (clap arg-parsing error). The error envelope
/// surfaces the accepted values via the `--help` text of the flag.
#[test]
#[serial]
fn llm_backend_rejects_unknown_value() {
    let tmp = TempDir::new().expect("tempdir");
    cmd_base(&tmp)
        .arg("remember")
        .arg("--name")
        .arg("smoke-invalid")
        .arg("--type")
        .arg("note")
        .arg("--description")
        .arg("GAP-003 invalid value")
        .arg("--body")
        .arg("x")
        .arg("--llm-backend")
        .arg("totally-bogus")
        .arg("--json")
        .assert()
        .failure()
        .code(2);
}
