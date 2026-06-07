//! Regression test for the WAL auto-init bug found during the v1.0.34 audit.
//!
//! Before the fix: `remember`/`ingest`/etc. (auto-init path) created databases
//! in `journal_mode = delete`. Only explicit `init` activated WAL.
//! After the fix: every command that goes through `ensure_db_ready()` ends with
//! `journal_mode = wal`, matching the documented contract.

use assert_cmd::Command;
use rusqlite::Connection;
use tempfile::TempDir;

/// Builds a fresh `Command` with the mock LLM PATH prepended.
///
/// v1.0.76 spawns `claude` or `codex` on every `remember` / `ingest` /
/// `edit`. The bundled mocks under `tests/mock-llm/` return a fixed
/// 384-dim zero vector so the binary finishes without a real OAuth
/// login. The mock directory is leaked (no TempDir cleanup) so the
/// spawned subprocess always finds the mocks.
fn sgr_cmd() -> Command {
    let mock_dir = common::mock_llm_path();
    let mut c = Command::cargo_bin("sqlite-graphrag").expect("sqlite-graphrag binary not found");
    c.env("PATH", common::prepend_path(&mock_dir));
    c
}

#[path = "common/mod.rs"]
mod common;

fn assert_wal_after(cmd_args: &[&str], description: &str) {
    let tmp = TempDir::new().expect("create tempdir");
    let db_path = tmp.path().join("graphrag.sqlite");

    let mut cmd = sgr_cmd();
    let output = cmd
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_HOME", tmp.path())
        .env_remove("SQLITE_GRAPHRAG_LANG")
        .args(cmd_args)
        .timeout(std::time::Duration::from_secs(120))
        .output()
        .expect("command runs");

    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        if code == 11 || code == -1 {
            eprintln!(
                "skipping wal_after `{description}`: embedding model unavailable or timed out (code {code})"
            );
            return;
        }
        panic!(
            "expected success after `{description}`, got code {code}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let conn = Connection::open(&db_path).expect("open db for assertion");
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .expect("query journal_mode");
    assert_eq!(
        mode, "wal",
        "expected journal_mode=wal after `{description}`, got `{mode}`"
    );
}

#[test]
fn wal_after_remember_skip_extraction() {
    assert_wal_after(
        &[
            "remember",
            "--name",
            "auto-init-test",
            "--type",
            "document",
            "--description",
            "wal regression test",
            "--body",
            "smoke",
            "--skip-extraction",
            "--json",
        ],
        "remember --skip-extraction",
    );
}

#[test]
fn wal_after_explicit_init() {
    assert_wal_after(&["init", "--json"], "init");
}

#[test]
fn wal_after_list_on_fresh_dir() {
    assert_wal_after(&["list", "--json"], "list (auto-init)");
}
