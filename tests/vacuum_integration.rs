#![cfg(feature = "slow-tests")]

use assert_cmd::Command;
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

fn cmd(tmp: &TempDir) -> Command {
    let mut c = sgr_cmd();
    c.env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"));
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c
}

fn init_db(tmp: &TempDir) {
    cmd(tmp).arg("init").assert().success();
}

#[test]
fn test_vacuum_auto_inits_when_missing() {
    let tmp = TempDir::new().unwrap();
    cmd(&tmp).arg("vacuum").assert().success();
}

#[test]
fn test_vacuum_success_after_init() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp).arg("vacuum").assert().success();
}

#[test]
fn test_vacuum_returns_json_with_status_ok() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let output = cmd(&tmp)
        .arg("vacuum")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["status"], "ok");
}

#[test]
fn test_vacuum_json_contem_db_path() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let output = cmd(&tmp)
        .arg("vacuum")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(json["db_path"].is_string());
    assert!(!json["db_path"].as_str().unwrap().is_empty());
}

#[test]
fn test_vacuum_json_contem_tamanhos() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let output = cmd(&tmp)
        .arg("vacuum")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(json["size_before_bytes"].is_number());
    assert!(json["size_after_bytes"].is_number());
}

#[test]
fn test_vacuum_via_env_db_path() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("custom.sqlite");

    let mut init_cmd = sgr_cmd();
    init_cmd
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"))
        .env("SQLITE_GRAPHRAG_LOG_LEVEL", "error")
        .arg("init")
        .assert()
        .success();

    let mut vac_cmd = sgr_cmd();
    vac_cmd
        .env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"))
        .env("SQLITE_GRAPHRAG_LOG_LEVEL", "error")
        .arg("vacuum")
        .assert()
        .success();
}
