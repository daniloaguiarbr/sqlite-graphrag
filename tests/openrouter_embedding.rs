//! Integration tests for v1.0.93 OpenRouter embedding backend.
//!
//! Tests the `--embedding-backend` flag, the `config` subcommand
//! (add-key, list-keys, remove-key, doctor), and the interaction
//! between embedding backend selection and API key resolution.
//!
//! All tests run offline — no real OpenRouter API calls.

#![cfg(feature = "slow-tests")]

use assert_cmd::Command;
use predicates::prelude::*;
use serial_test::serial;
use tempfile::TempDir;

#[path = "common/mod.rs"]
mod common;

fn sgr_cmd() -> Command {
    let mock_dir = common::mock_llm_path();
    let mut c = Command::cargo_bin("sqlite-graphrag").expect("binary not found");
    c.env("PATH", common::prepend_path(&mock_dir));
    c
}

fn cmd_base(tmp: &TempDir) -> Command {
    let mut c = sgr_cmd();
    c.env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"));
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c.env_remove("OPENROUTER_API_KEY");
    c.arg("--skip-memory-guard");
    c
}

#[test]
#[serial]
fn embedding_backend_flag_accepts_auto() {
    let tmp = TempDir::new().expect("tempdir");
    cmd_base(&tmp)
        .arg("--embedding-backend")
        .arg("auto")
        .arg("--llm-backend")
        .arg("none")
        .arg("--skip-embedding-on-failure")
        .arg("remember")
        .arg("--name")
        .arg("test-auto")
        .arg("--type")
        .arg("note")
        .arg("--description")
        .arg("test auto backend")
        .arg("--body")
        .arg("hello world")
        .arg("--json")
        .assert()
        .success();
}

#[test]
#[serial]
fn embedding_backend_flag_accepts_llm() {
    let tmp = TempDir::new().expect("tempdir");
    cmd_base(&tmp)
        .arg("--embedding-backend")
        .arg("llm")
        .arg("--llm-backend")
        .arg("none")
        .arg("--skip-embedding-on-failure")
        .arg("remember")
        .arg("--name")
        .arg("test-llm")
        .arg("--type")
        .arg("note")
        .arg("--description")
        .arg("test llm backend")
        .arg("--body")
        .arg("hello world")
        .arg("--json")
        .assert()
        .success();
}

#[test]
#[serial]
fn embedding_backend_openrouter_without_key_fails() {
    let tmp = TempDir::new().expect("tempdir");
    cmd_base(&tmp)
        .arg("--embedding-backend")
        .arg("openrouter")
        .arg("remember")
        .arg("--name")
        .arg("test-fail")
        .arg("--type")
        .arg("note")
        .arg("--description")
        .arg("should fail without key")
        .arg("--body")
        .arg("hello")
        .arg("--json")
        .assert()
        .failure();
}

#[test]
#[serial]
fn embedding_backend_flag_rejects_invalid_value() {
    let tmp = TempDir::new().expect("tempdir");
    cmd_base(&tmp)
        .arg("--embedding-backend")
        .arg("invalid-value")
        .arg("health")
        .arg("--json")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value"));
}

#[test]
#[serial]
fn config_add_key_and_list_roundtrip() {
    let tmp = TempDir::new().expect("tempdir");
    let config_home = tmp.path().join("xdg-config");
    std::fs::create_dir_all(&config_home).unwrap();

    sgr_cmd()
        .env("XDG_CONFIG_HOME", &config_home)
        .env("HOME", tmp.path())
        .arg("config")
        .arg("add-key")
        .arg("--provider")
        .arg("openrouter")
        .arg("--from-stdin")
        .write_stdin("sk-or-v1-test-key-for-integration-test")
        .assert()
        .success()
        .stdout(predicate::str::contains("key_added"));

    sgr_cmd()
        .env("XDG_CONFIG_HOME", &config_home)
        .env("HOME", tmp.path())
        .arg("config")
        .arg("list-keys")
        .assert()
        .success()
        .stdout(predicate::str::contains("openrouter"));
}

#[test]
#[serial]
fn config_doctor_with_env_key_shows_resolved() {
    sgr_cmd()
        .env("OPENROUTER_API_KEY", "sk-or-v1-doctor-test-long-key")
        .arg("config")
        .arg("doctor")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("\"resolved\":true")
                .or(predicate::str::contains("\"resolved\": true")),
        )
        .stdout(predicate::str::contains("env"));
}

#[test]
#[serial]
fn config_remove_key_by_fingerprint() {
    let tmp = TempDir::new().expect("tempdir");
    let config_home = tmp.path().join("xdg-config");
    std::fs::create_dir_all(&config_home).unwrap();

    let output = sgr_cmd()
        .env("XDG_CONFIG_HOME", &config_home)
        .env("HOME", tmp.path())
        .arg("config")
        .arg("add-key")
        .arg("--provider")
        .arg("openrouter")
        .arg("--from-stdin")
        .write_stdin("sk-or-v1-remove-test-key-long-enough")
        .output()
        .expect("add key");
    assert!(output.status.success(), "add-key failed");

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse add-key json");
    let fingerprint = json["fingerprint"]
        .as_str()
        .expect("fingerprint field missing");

    sgr_cmd()
        .env("XDG_CONFIG_HOME", &config_home)
        .env("HOME", tmp.path())
        .arg("config")
        .arg("remove-key")
        .arg(fingerprint)
        .assert()
        .success()
        .stdout(predicate::str::contains("key_removed"));

    sgr_cmd()
        .env("XDG_CONFIG_HOME", &config_home)
        .env("HOME", tmp.path())
        .arg("config")
        .arg("list-keys")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("\"keys\":[]").or(predicate::str::contains("\"keys\": []")),
        );
}
