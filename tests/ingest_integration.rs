#![cfg(feature = "slow-tests")]

// Suite — `ingest` end-to-end behaviour
//
// ISOLATION: every test owns an exclusive `TempDir` and points the binary at it
// through `SQLITE_GRAPHRAG_DB_PATH` and `SQLITE_GRAPHRAG_CACHE_DIR`. The
// `--skip-memory-guard` global flag prevents the daemon autostart path from
// being triggered by parallel test runs.
//
// CHILD PROCESS NOTE: `ingest` spawns `remember` as a child process via
// `current_exe()`. The child inherits the parent's environment, so the
// `SQLITE_GRAPHRAG_DAEMON_DISABLE_AUTOSTART=1` flag set by the parent's
// `--skip-memory-guard` propagates automatically.
//
// `#[serial]` is mandatory: although every test owns its DB, the binary
// artefact is shared and process-global resources (sqlite-vec auto-extension,
// fastembed model cache) are loaded per child. Serialising eliminates races.

use assert_cmd::prelude::*;
use serde_json::Value;
use serial_test::serial;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Builds an `ingest` command bound to an isolated TempDir.
fn ingest_cmd(temp: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("sqlite-graphrag").expect("binary not found");
    cmd.env(
        "SQLITE_GRAPHRAG_DB_PATH",
        temp.path().join("graphrag.sqlite"),
    );
    cmd.env("SQLITE_GRAPHRAG_CACHE_DIR", temp.path().join("cache"));
    cmd.env("SQLITE_GRAPHRAG_NAMESPACE", "global");
    cmd.arg("--skip-memory-guard");
    cmd
}

/// Initialises an isolated database with V001..V009 applied.
fn init_db(temp: &TempDir) {
    Command::cargo_bin("sqlite-graphrag")
        .expect("binary not found")
        .env(
            "SQLITE_GRAPHRAG_DB_PATH",
            temp.path().join("graphrag.sqlite"),
        )
        .env("SQLITE_GRAPHRAG_CACHE_DIR", temp.path().join("cache"))
        .args(["--skip-memory-guard", "init"])
        .assert()
        .success();
}

/// Writes a Markdown file with the given basename and a deterministic body.
fn write_md(dir: &Path, basename: &str, body: &str) {
    std::fs::write(dir.join(basename), body).expect("write file must succeed");
}

/// Splits NDJSON stdout into trimmed non-empty lines.
fn ndjson_lines(stdout: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

/// Parses every line as JSON and panics on the first failure.
fn parse_all_lines(lines: &[String]) -> Vec<Value> {
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            serde_json::from_str::<Value>(line)
                .unwrap_or_else(|e| panic!("line {i} is not valid JSON: {e}\nline: {line}"))
        })
        .collect()
}

/// Returns the summary value (last line) and per-file events (preceding lines).
fn split_events_and_summary(values: Vec<Value>) -> (Vec<Value>, Value) {
    assert!(!values.is_empty(), "expected at least the summary line");
    let summary = values.last().cloned().expect("summary present");
    assert_eq!(
        summary.get("summary"),
        Some(&Value::Bool(true)),
        "last line must be the summary, got {summary}"
    );
    let events = values[..values.len() - 1].to_vec();
    (events, summary)
}

// ---------------------------------------------------------------------------
// Test 1 — every emitted line is valid standalone JSON (NDJSON contract)
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_ingest_emits_valid_ndjson() {
    let tmp = TempDir::new().expect("TempDir");
    init_db(&tmp);

    let docs = tmp.path().join("docs");
    std::fs::create_dir(&docs).expect("create docs dir");
    write_md(&docs, "alpha.md", "alpha body content");
    write_md(&docs, "beta.md", "beta body content");
    write_md(&docs, "gamma.md", "gamma body content");

    let output = ingest_cmd(&tmp)
        .args([
            "ingest",
            docs.to_str().expect("path utf-8"),
            "--type",
            "document",
            "--skip-extraction",
        ])
        .output()
        .expect("ingest must run");

    assert!(
        output.status.success(),
        "ingest failed: status={:?}\nstderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let lines = ndjson_lines(&output.stdout);
    assert!(lines.len() >= 4, "expected at least 4 lines, got {lines:?}");

    let values = parse_all_lines(&lines);
    let (events, summary) = split_events_and_summary(values);

    assert_eq!(events.len(), 3, "expected 3 file events, got {events:?}");
    for event in &events {
        assert_eq!(event.get("status"), Some(&Value::String("indexed".into())));
    }

    assert_eq!(summary["files_total"], 3);
    assert_eq!(summary["files_succeeded"], 3);
    assert_eq!(summary["files_failed"], 0);
    assert_eq!(summary["files_skipped"], 0);
}

// ---------------------------------------------------------------------------
// Test 2 — when no --db / SQLITE_GRAPHRAG_DB_PATH is provided, `graphrag.sqlite`
// is created relative to the current working directory.
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_ingest_creates_db_in_cwd() {
    let tmp = TempDir::new().expect("TempDir");
    let cwd = tmp.path().join("workspace");
    std::fs::create_dir(&cwd).expect("create cwd");
    let cache = tmp.path().join("cache");

    // init must run inside the same CWD so the implicit DB path resolves there.
    Command::cargo_bin("sqlite-graphrag")
        .expect("binary not found")
        .current_dir(&cwd)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", &cache)
        .env_remove("SQLITE_GRAPHRAG_DB_PATH")
        .args(["--skip-memory-guard", "init"])
        .assert()
        .success();

    let docs = cwd.join("docs");
    std::fs::create_dir(&docs).expect("create docs dir");
    write_md(&docs, "only.md", "only body");

    let output = Command::cargo_bin("sqlite-graphrag")
        .expect("binary not found")
        .current_dir(&cwd)
        .env("SQLITE_GRAPHRAG_CACHE_DIR", &cache)
        .env("SQLITE_GRAPHRAG_NAMESPACE", "global")
        .env_remove("SQLITE_GRAPHRAG_DB_PATH")
        .args([
            "--skip-memory-guard",
            "ingest",
            "docs",
            "--type",
            "document",
            "--skip-extraction",
        ])
        .output()
        .expect("ingest must run");

    assert!(
        output.status.success(),
        "ingest failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let db = cwd.join("graphrag.sqlite");
    assert!(
        db.exists(),
        "graphrag.sqlite must exist in CWD after ingest, looked at {}",
        db.display()
    );
}

// ---------------------------------------------------------------------------
// Test 3 — `--skip-extraction` propagates to the child `remember` invocation
// without breaking the indexed status.
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_ingest_skip_extraction_flag() {
    let tmp = TempDir::new().expect("TempDir");
    init_db(&tmp);

    let docs = tmp.path().join("docs");
    std::fs::create_dir(&docs).expect("create docs dir");
    write_md(&docs, "first.md", "first body");
    write_md(&docs, "second.md", "second body");

    let output = ingest_cmd(&tmp)
        .args([
            "ingest",
            docs.to_str().expect("utf-8"),
            "--type",
            "document",
            "--skip-extraction",
        ])
        .output()
        .expect("ingest must run");

    assert!(
        output.status.success(),
        "ingest failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let values = parse_all_lines(&ndjson_lines(&output.stdout));
    let (events, summary) = split_events_and_summary(values);

    assert_eq!(events.len(), 2);
    for event in &events {
        assert_eq!(event["status"], Value::String("indexed".into()));
        assert!(
            event.get("memory_id").and_then(Value::as_i64).is_some(),
            "memory_id must be present on indexed events: {event}"
        );
    }
    assert_eq!(summary["files_succeeded"], 2);
    assert_eq!(summary["files_failed"], 0);
}

// ---------------------------------------------------------------------------
// Test 4 — `--pattern` filters by basename suffix (`*.md`-style globs only).
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_ingest_pattern_filter() {
    let tmp = TempDir::new().expect("TempDir");
    init_db(&tmp);

    let docs = tmp.path().join("docs");
    std::fs::create_dir(&docs).expect("create docs dir");
    write_md(&docs, "keep.md", "keep");
    write_md(&docs, "drop.txt", "drop");
    write_md(&docs, "drop.log", "drop");

    let output = ingest_cmd(&tmp)
        .args([
            "ingest",
            docs.to_str().expect("utf-8"),
            "--type",
            "document",
            "--pattern",
            "*.md",
            "--skip-extraction",
        ])
        .output()
        .expect("ingest must run");

    assert!(
        output.status.success(),
        "ingest failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let values = parse_all_lines(&ndjson_lines(&output.stdout));
    let (events, summary) = split_events_and_summary(values);

    assert_eq!(summary["files_total"], 1);
    assert_eq!(events.len(), 1);
    assert!(events[0]["file"]
        .as_str()
        .expect("file string")
        .ends_with("keep.md"));
}

// ---------------------------------------------------------------------------
// Test 5 — exceeding `--max-files` is a validation error (no partial ingest).
// ---------------------------------------------------------------------------
// The current contract treats `max_files` as a safety cap: if the discovered
// file set exceeds it, the run aborts with a validation error before any file
// is processed. The test pins this contract so a future "process up to N"
// behaviour change is a deliberate decision.

#[test]
#[serial]
fn test_ingest_max_files_cap() {
    let tmp = TempDir::new().expect("TempDir");
    init_db(&tmp);

    let docs = tmp.path().join("docs");
    std::fs::create_dir(&docs).expect("create docs dir");
    for i in 0..12 {
        write_md(&docs, &format!("file-{i:02}.md"), "body");
    }

    let output = ingest_cmd(&tmp)
        .args([
            "ingest",
            docs.to_str().expect("utf-8"),
            "--type",
            "document",
            "--max-files",
            "5",
            "--skip-extraction",
        ])
        .output()
        .expect("ingest must run");

    assert!(
        !output.status.success(),
        "ingest must fail when files exceed --max-files cap"
    );
    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    assert!(
        stderr.contains("max-files") || stderr.contains("cap") || stderr.contains("exceeds"),
        "stderr should mention the cap, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Test 6 — `--fail-fast` aborts after the first failure; the default keeps
// going. Failures are forced by pointing `--db` at an unwritable path so that
// every child `remember` call fails to open the database.
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_ingest_fail_fast_aborts_on_first_error() {
    let tmp = TempDir::new().expect("TempDir");
    init_db(&tmp);

    let docs = tmp.path().join("docs");
    std::fs::create_dir(&docs).expect("create docs dir");
    write_md(&docs, "a.md", "a");
    write_md(&docs, "b.md", "b");
    write_md(&docs, "c.md", "c");

    // An unwritable absolute path — `/proc` is read-only on Linux, so any DB
    // file requested under it cannot be created. Each child `remember` will
    // fail with an I/O error.
    let bad_db = "/proc/sqlite-graphrag-must-not-create.sqlite";

    // Without --fail-fast: every file fails but the run reaches the summary.
    let output = ingest_cmd(&tmp)
        .args([
            "ingest",
            docs.to_str().expect("utf-8"),
            "--type",
            "document",
            "--db",
            bad_db,
            "--skip-extraction",
        ])
        .output()
        .expect("ingest must run");

    let values = parse_all_lines(&ndjson_lines(&output.stdout));
    let (events, summary) = split_events_and_summary(values);
    assert_eq!(events.len(), 3, "all 3 files should have been attempted");
    assert_eq!(summary["files_failed"], 3);
    assert_eq!(summary["files_succeeded"], 0);

    // With --fail-fast: stops after the first failure.
    let output = ingest_cmd(&tmp)
        .args([
            "ingest",
            docs.to_str().expect("utf-8"),
            "--type",
            "document",
            "--db",
            bad_db,
            "--fail-fast",
            "--skip-extraction",
        ])
        .output()
        .expect("ingest must run");

    assert!(
        !output.status.success(),
        "fail-fast must surface a non-zero exit code"
    );
    let values = parse_all_lines(&ndjson_lines(&output.stdout));
    let (events, summary) = split_events_and_summary(values);
    assert_eq!(events.len(), 1, "only the first file should be attempted");
    assert_eq!(summary["files_failed"], 1);
    assert_eq!(summary["files_succeeded"], 0);
}

// ---------------------------------------------------------------------------
// Test 7 — `--recursive` walks subdirectories; the default does not.
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_ingest_recursive_walks_subdirs() {
    let tmp = TempDir::new().expect("TempDir");
    init_db(&tmp);

    let root = tmp.path().join("docs");
    let nested = root.join("a").join("b").join("c");
    std::fs::create_dir_all(&nested).expect("create nested dir");
    write_md(&root, "top.md", "top");
    write_md(&nested, "deep.md", "deep");

    // Without --recursive: only the top-level file is found.
    let output = ingest_cmd(&tmp)
        .args([
            "ingest",
            root.to_str().expect("utf-8"),
            "--type",
            "document",
            "--skip-extraction",
        ])
        .output()
        .expect("ingest must run");
    assert!(output.status.success());
    let values = parse_all_lines(&ndjson_lines(&output.stdout));
    let (_, summary) = split_events_and_summary(values);
    assert_eq!(summary["files_total"], 1);

    // With --recursive: both files are picked up.
    let output = ingest_cmd(&tmp)
        .args([
            "ingest",
            root.to_str().expect("utf-8"),
            "--type",
            "document",
            "--recursive",
            "--skip-extraction",
        ])
        .output()
        .expect("ingest must run");
    assert!(output.status.success());
    let values = parse_all_lines(&ndjson_lines(&output.stdout));
    let (events, summary) = split_events_and_summary(values);
    assert_eq!(summary["files_total"], 2);
    let names: Vec<&str> = events
        .iter()
        .map(|e| e["name"].as_str().unwrap_or(""))
        .collect();
    assert!(names.contains(&"top"));
    assert!(names.contains(&"deep"));
}

// ---------------------------------------------------------------------------
// Test 8 — derived names are truncated to a maximum of 60 characters.
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_ingest_name_truncation_60_chars() {
    let tmp = TempDir::new().expect("TempDir");
    init_db(&tmp);

    let docs = tmp.path().join("docs");
    std::fs::create_dir(&docs).expect("create docs dir");

    // 80 ASCII lowercase characters — well under the Linux 255-byte filename
    // limit and large enough to force truncation.
    let long_stem = "a".repeat(80);
    write_md(&docs, &format!("{long_stem}.md"), "body");

    let output = ingest_cmd(&tmp)
        .args([
            "ingest",
            docs.to_str().expect("utf-8"),
            "--type",
            "document",
            "--skip-extraction",
        ])
        .output()
        .expect("ingest must run");
    assert!(
        output.status.success(),
        "ingest failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let values = parse_all_lines(&ndjson_lines(&output.stdout));
    let (events, _) = split_events_and_summary(values);
    assert_eq!(events.len(), 1);
    let name = events[0]["name"].as_str().expect("name string");
    assert!(
        name.len() <= 60,
        "derived name must be truncated to <= 60 chars, got len={} ({name})",
        name.len()
    );
}

// ---------------------------------------------------------------------------
// Test 9 — the default pattern is `*.md`; `.txt` files are ignored unless an
// explicit pattern asks for them.
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_ingest_default_pattern_is_md() {
    let tmp = TempDir::new().expect("TempDir");
    init_db(&tmp);

    let docs = tmp.path().join("docs");
    std::fs::create_dir(&docs).expect("create docs dir");
    write_md(&docs, "doc.md", "md body");
    write_md(&docs, "doc.txt", "txt body");

    let output = ingest_cmd(&tmp)
        .args([
            "ingest",
            docs.to_str().expect("utf-8"),
            "--type",
            "document",
            "--skip-extraction",
        ])
        .output()
        .expect("ingest must run");
    assert!(output.status.success());

    let values = parse_all_lines(&ndjson_lines(&output.stdout));
    let (events, summary) = split_events_and_summary(values);
    assert_eq!(summary["files_total"], 1);
    assert_eq!(events.len(), 1);
    assert!(events[0]["file"]
        .as_str()
        .expect("file string")
        .ends_with("doc.md"));
}

// ---------------------------------------------------------------------------
// Test 10 — the last NDJSON line is the summary and exposes the documented
// counter fields.
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_ingest_summary_is_last_line() {
    let tmp = TempDir::new().expect("TempDir");
    init_db(&tmp);

    let docs = tmp.path().join("docs");
    std::fs::create_dir(&docs).expect("create docs dir");
    write_md(&docs, "one.md", "one body");
    write_md(&docs, "two.md", "two body");

    let output = ingest_cmd(&tmp)
        .args([
            "ingest",
            docs.to_str().expect("utf-8"),
            "--type",
            "document",
            "--skip-extraction",
        ])
        .output()
        .expect("ingest must run");
    assert!(output.status.success());

    let lines = ndjson_lines(&output.stdout);
    let last = lines.last().expect("at least one line");
    let summary: Value = serde_json::from_str(last).expect("summary must be JSON");

    assert_eq!(summary["summary"], Value::Bool(true));
    for key in [
        "files_total",
        "files_succeeded",
        "files_failed",
        "files_skipped",
        "elapsed_ms",
        "dir",
        "pattern",
        "recursive",
    ] {
        assert!(
            summary.get(key).is_some(),
            "summary must expose `{key}`, got: {summary}"
        );
    }
    // Earlier lines must NOT carry `summary: true`.
    for line in &lines[..lines.len() - 1] {
        let v: Value = serde_json::from_str(line).expect("event JSON");
        assert_ne!(
            v.get("summary"),
            Some(&Value::Bool(true)),
            "only the final line should be flagged as summary, got: {line}"
        );
    }
}
