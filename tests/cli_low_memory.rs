#![cfg(feature = "slow-tests")]

//! End-to-end coverage for the `--low-memory` flag and the
//! `SQLITE_GRAPHRAG_LOW_MEMORY` env var on the `ingest` subcommand.
//!
//! Each test owns an isolated `TempDir` and points the binary at it through
//! `SQLITE_GRAPHRAG_DB_PATH` and `SQLITE_GRAPHRAG_CACHE_DIR`. The
//! `--skip-memory-guard` flag prevents the daemon autostart path during
//! parallel test runs. `#[serial]` is mandatory because the binary artefact
//! is shared and the env var these tests manipulate is process-global.

use assert_cmd::prelude::*;
use serde_json::Value;
use serial_test::serial;
use std::process::Command;
use tempfile::TempDir;

fn ingest_cmd(temp: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("sqlite-graphrag").expect("binary not found");
    cmd.env(
        "SQLITE_GRAPHRAG_DB_PATH",
        temp.path().join("graphrag.sqlite"),
    );
    cmd.env("SQLITE_GRAPHRAG_CACHE_DIR", temp.path().join("cache"));
    cmd.env("SQLITE_GRAPHRAG_NAMESPACE", "global");
    // Keep tests deterministic regardless of the host shell's env.
    cmd.env_remove("SQLITE_GRAPHRAG_LOW_MEMORY");
    cmd.arg("--skip-memory-guard");
    cmd
}

fn init_db(temp: &TempDir) {
    let mut c = Command::cargo_bin("sqlite-graphrag").expect("binary not found");
    c.env(
        "SQLITE_GRAPHRAG_DB_PATH",
        temp.path().join("graphrag.sqlite"),
    );
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", temp.path().join("cache"));
    c.env_remove("SQLITE_GRAPHRAG_LOW_MEMORY");
    c.args(["--skip-memory-guard", "init"]).assert().success();
}

fn write_corpus(temp: &TempDir) -> std::path::PathBuf {
    let dir = temp.path().join("corpus");
    std::fs::create_dir_all(&dir).expect("create corpus dir");
    std::fs::write(dir.join("a.md"), "# Alpha\n\nContent of file alpha.").expect("write a.md");
    std::fs::write(dir.join("b.md"), "# Beta\n\nContent of file beta.").expect("write b.md");
    dir
}

/// Locates the final NDJSON summary line emitted by `ingest`.
fn parse_summary(stdout: &[u8]) -> Value {
    let lines: Vec<Value> = String::from_utf8_lossy(stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("ndjson line is valid JSON"))
        .collect();
    lines
        .into_iter()
        .rev()
        .find(|v| v.get("summary").and_then(|x| x.as_bool()).unwrap_or(false))
        .expect("summary line missing")
}

#[test]
#[serial]
fn ingest_low_memory_flag_succeeds() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);
    let dir = write_corpus(&tmp);

    let out = ingest_cmd(&tmp)
        .args([
            "ingest",
            dir.to_str().unwrap(),
            "--type",
            "document",
            "--pattern",
            "*.md",
            "--low-memory",
            "--skip-extraction",
        ])
        .assert()
        .success();

    let summary = parse_summary(&out.get_output().stdout);
    assert_eq!(
        summary["files_succeeded"].as_u64().unwrap(),
        2,
        "summary: {summary}"
    );
    assert_eq!(summary["files_failed"].as_u64().unwrap(), 0);
}

#[test]
#[serial]
fn ingest_low_memory_overrides_explicit_parallelism() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);
    let dir = write_corpus(&tmp);

    // Combine --low-memory with --ingest-parallelism 4. The flag must win and
    // emit a tracing::warn! about the override (visible at -v info level).
    let out = ingest_cmd(&tmp)
        .args([
            "-v",
            "ingest",
            dir.to_str().unwrap(),
            "--type",
            "document",
            "--pattern",
            "*.md",
            "--low-memory",
            "--ingest-parallelism",
            "4",
            "--skip-extraction",
        ])
        .assert()
        .success();

    let stderr = String::from_utf8_lossy(&out.get_output().stderr);
    assert!(
        stderr.contains("--ingest-parallelism overridden by --low-memory"),
        "stderr must announce the override; got:\n{stderr}"
    );
    let summary = parse_summary(&out.get_output().stdout);
    assert_eq!(summary["files_succeeded"].as_u64().unwrap(), 2);
}

#[test]
#[serial]
fn ingest_env_var_low_memory_activates_mode() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);
    let dir = write_corpus(&tmp);

    let out = ingest_cmd(&tmp)
        .env("SQLITE_GRAPHRAG_LOW_MEMORY", "1")
        .args([
            "-v",
            "ingest",
            dir.to_str().unwrap(),
            "--type",
            "document",
            "--pattern",
            "*.md",
            "--skip-extraction",
        ])
        .assert()
        .success();

    let stderr = String::from_utf8_lossy(&out.get_output().stderr);
    assert!(
        stderr.contains("low-memory mode enabled via SQLITE_GRAPHRAG_LOW_MEMORY"),
        "stderr must announce env-driven low-memory mode; got:\n{stderr}"
    );
    let summary = parse_summary(&out.get_output().stdout);
    assert_eq!(summary["files_succeeded"].as_u64().unwrap(), 2);
}
