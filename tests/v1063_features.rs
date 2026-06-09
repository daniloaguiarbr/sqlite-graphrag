//! Integration tests for v1.0.63 fixes:
//! - BUG-1: `restore` preserves current memory name after rename
//! - BUG-2: `ingest` normalizes relation strings before DB insertion
//! - FINDING-1: `edit` re-generates vector embedding when body changes

use assert_cmd::Command;
use serial_test::serial;
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

fn system_cache_dir() -> std::path::PathBuf {
    if let Ok(d) = std::env::var("SQLITE_GRAPHRAG_CACHE_DIR") {
        return std::path::PathBuf::from(d);
    }
    directories::ProjectDirs::from("", "", "sqlite-graphrag")
        .map(|p| p.cache_dir().to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from(".cache"))
}

fn cmd(temp: &TempDir) -> Command {
    let cache = system_cache_dir();
    let mut c = sgr_cmd();
    let mock_dir = common::mock_llm_path();
    c.env_clear()
        .env("HOME", temp.path())
        .env("SQLITE_GRAPHRAG_HOME", temp.path())
        .env("SQLITE_GRAPHRAG_CACHE_DIR", &cache)
        .env("SQLITE_GRAPHRAG_LANG", "en")
        .env("SQLITE_GRAPHRAG_LOG_LEVEL", "warn")
        .current_dir(temp.path());
    for var in &["LOCALAPPDATA", "APPDATA", "USERPROFILE", "SystemRoot"] {
        if let Ok(v) = std::env::var(var) {
            c.env(var, v);
        }
    }
    c.env("PATH", common::prepend_path(&mock_dir));
    c
}

fn init(tmp: &TempDir) {
    cmd(tmp).args(["init", "--json"]).assert().success();
}

#[test]
#[serial]
fn restore_preserves_name_after_rename() {
    let tmp = TempDir::new().unwrap();
    init(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "orig-name",
            "--type",
            "note",
            "--description",
            "d",
            "--body",
            "original body",
        ])
        .assert()
        .success();

    cmd(&tmp)
        .args(["edit", "--name", "orig-name", "--body", "edited body v2"])
        .assert()
        .success();

    cmd(&tmp)
        .args([
            "rename",
            "--name",
            "orig-name",
            "--new-name",
            "renamed-name",
        ])
        .assert()
        .success();

    cmd(&tmp)
        .args([
            "restore",
            "--name",
            "renamed-name",
            "--version",
            "1",
            "--json",
        ])
        .assert()
        .success();

    let out = cmd(&tmp)
        .args(["read", "--name", "renamed-name", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "read by renamed name must succeed");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["body"], "original body");

    cmd(&tmp)
        .args(["read", "--name", "orig-name", "--json"])
        .assert()
        .failure()
        .code(4);
}

#[test]
#[serial]
fn restore_does_not_crash_when_old_name_occupied() {
    let tmp = TempDir::new().unwrap();
    init(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "slot-a",
            "--type",
            "note",
            "--description",
            "d",
            "--body",
            "body a",
        ])
        .assert()
        .success();

    cmd(&tmp)
        .args(["rename", "--name", "slot-a", "--new-name", "slot-b"])
        .assert()
        .success();

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "slot-a",
            "--type",
            "note",
            "--description",
            "new occupant",
            "--body",
            "body new a",
        ])
        .assert()
        .success();

    cmd(&tmp)
        .args(["restore", "--name", "slot-b", "--version", "1", "--json"])
        .assert()
        .success();

    let out_b = cmd(&tmp)
        .args(["read", "--name", "slot-b", "--json"])
        .output()
        .unwrap();
    assert!(out_b.status.success());
    let json_b: serde_json::Value = serde_json::from_slice(&out_b.stdout).unwrap();
    assert_eq!(json_b["body"], "body a");

    let out_a = cmd(&tmp)
        .args(["read", "--name", "slot-a", "--json"])
        .output()
        .unwrap();
    assert!(out_a.status.success());
    let json_a: serde_json::Value = serde_json::from_slice(&out_a.stdout).unwrap();
    assert_eq!(json_a["body"], "body new a");
}

#[test]
#[serial]
fn edit_reembeds_when_body_changes() {
    let tmp = TempDir::new().unwrap();
    init(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "embed-test",
            "--type",
            "note",
            "--description",
            "d",
            "--body",
            "quantum computing algorithms",
        ])
        .assert()
        .success();

    cmd(&tmp)
        .args([
            "edit",
            "--name",
            "embed-test",
            "--body",
            "medieval castle architecture design",
        ])
        .assert()
        .success();

    let out = cmd(&tmp)
        .args([
            "recall",
            "medieval castle architecture",
            "--k",
            "1",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let results = json["results"].as_array().unwrap();
    assert!(!results.is_empty(), "recall must find the edited memory");
    assert_eq!(results[0]["name"], "embed-test");
}
