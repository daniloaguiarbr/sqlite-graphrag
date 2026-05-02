#![cfg(feature = "slow-tests")]

//! CLI integration tests for M2 (forget deleted_at_iso) and M3 (ingest truncated/original_name).

use assert_cmd::Command;
use serde_json::Value;
use serial_test::serial;
use tempfile::TempDir;

fn cmd(tmp: &TempDir) -> Command {
    let mut c = Command::cargo_bin("sqlite-graphrag").unwrap();
    c.env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"));
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c.arg("--skip-memory-guard");
    c
}

fn init_db(tmp: &TempDir) {
    cmd(tmp).arg("init").assert().success();
}

#[test]
#[serial]
fn forget_response_emits_deleted_at_iso_when_soft_deleted() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "test-mem",
            "--type",
            "user",
            "--description",
            "a test memory",
            "--body",
            "body text",
        ])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args(["forget", "--name", "test-mem"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).expect("stdout must be valid JSON");

    assert_eq!(json["action"], "soft_deleted");
    assert_eq!(json["forgotten"], true);

    let deleted_at = json.get("deleted_at");
    assert!(
        deleted_at.is_some() && deleted_at.unwrap().is_number(),
        "deleted_at must be a number; got: {json}"
    );

    let deleted_at_iso = json.get("deleted_at_iso");
    assert!(
        deleted_at_iso.is_some() && deleted_at_iso.unwrap().is_string(),
        "deleted_at_iso must be a string; got: {json}"
    );

    // Validate RFC 3339 format: must contain 'T' separator and timezone offset
    let iso_str = deleted_at_iso.unwrap().as_str().unwrap();
    assert!(
        iso_str.contains('T')
            && (iso_str.ends_with('Z')
                || iso_str.contains('+')
                || iso_str.contains("-0")
                || iso_str.contains(":00")),
        "deleted_at_iso must be RFC 3339; got: {iso_str}"
    );
}

#[test]
#[serial]
fn ingest_event_emits_truncated_when_filename_exceeds_60_chars() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    // Create a file with a basename of 80 chars (will exceed DERIVED_NAME_MAX_LEN=60)
    let long_name = format!("{}.md", "a".repeat(80));
    let file_path = tmp.path().join(&long_name);
    std::fs::write(&file_path, "content for truncation test").expect("write file must succeed");

    let output = cmd(&tmp)
        .args([
            "ingest",
            tmp.path().to_str().unwrap(),
            "--type",
            "document",
            "--pattern",
            &long_name,
        ])
        .output()
        .expect("ingest command must run");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Find the file event line (not the summary line)
    let file_event: Value = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|v| v.get("summary").is_none())
        .expect("must find a file event JSON line");

    assert_eq!(
        file_event["truncated"], true,
        "truncated must be true for long filename; got: {file_event}"
    );

    let original_name = file_event.get("original_name");
    assert!(
        original_name.is_some() && original_name.unwrap().is_string(),
        "original_name must be present and a string when truncated=true; got: {file_event}"
    );

    let original = original_name.unwrap().as_str().unwrap();
    assert!(
        original.len() > 60,
        "original_name must be longer than 60 chars; got len={} value={original}",
        original.len()
    );
}

#[test]
#[serial]
fn recall_with_autostart_daemon_false_does_not_spawn_daemon() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "autostart-test-mem",
            "--type",
            "user",
            "--description",
            "autostart test memory",
            "--body",
            "daemon autostart test body",
        ])
        .assert()
        .success();

    // Run recall with --autostart-daemon=false; the daemon should NOT be spawned.
    // We verify absence of the daemon spawn lock file in the control dir.
    cmd(&tmp)
        .args([
            "recall",
            "daemon autostart test",
            "--autostart-daemon=false",
        ])
        .env("SQLITE_GRAPHRAG_DAEMON_DISABLE_AUTOSTART", "1")
        .assert()
        .success();

    // Daemon spawn lock file must not exist when autostart was disabled.
    let cache_dir = tmp.path().join("cache");
    let spawn_lock = cache_dir.join("daemon-spawn.lock");
    assert!(
        !spawn_lock.exists(),
        "daemon-spawn.lock must NOT exist when --autostart-daemon=false; found at: {}",
        spawn_lock.display()
    );
}

#[test]
#[serial]
fn recall_default_autostart_daemon_true_remains_compatible() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "compat-test-mem",
            "--type",
            "user",
            "--description",
            "compatibility test memory",
            "--body",
            "regression guard body text",
        ])
        .assert()
        .success();

    // Default flag value (true) must not break the recall command.
    // Env var disables actual daemon spawn so the test stays fast and isolated.
    cmd(&tmp)
        .args(["recall", "regression guard"])
        .env("SQLITE_GRAPHRAG_DAEMON_DISABLE_AUTOSTART", "1")
        .assert()
        .success();
}

#[test]
#[serial]
fn list_include_deleted_emits_deleted_at_and_deleted_at_iso() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "h1-regression-mem",
            "--type",
            "user",
            "--description",
            "H1 regression memory",
            "--body",
            "body for list include-deleted regression",
        ])
        .assert()
        .success();

    cmd(&tmp)
        .args(["forget", "--name", "h1-regression-mem"])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args(["list", "--include-deleted", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value =
        serde_json::from_slice(&output).expect("list --include-deleted output must be valid JSON");

    let items = json["items"].as_array().expect("items must be an array");
    let item = items
        .iter()
        .find(|v| v["name"] == "h1-regression-mem")
        .expect("soft-deleted memory must appear in list --include-deleted");

    let deleted_at = item.get("deleted_at");
    assert!(
        deleted_at.is_some() && deleted_at.unwrap().is_number(),
        "deleted_at must be a number in list --include-deleted output; got: {item}"
    );

    let deleted_at_iso = item.get("deleted_at_iso");
    assert!(
        deleted_at_iso.is_some() && deleted_at_iso.unwrap().is_string(),
        "deleted_at_iso must be a string in list --include-deleted output; got: {item}"
    );

    let iso_str = deleted_at_iso.unwrap().as_str().unwrap();
    assert!(
        iso_str.contains('T'),
        "deleted_at_iso must be RFC 3339 (contains 'T'); got: {iso_str}"
    );
}

// ---------------------------------------------------------------------------
// Wave 2 logic bug regressions (v1.0.40)
// ---------------------------------------------------------------------------

// Bug M-A6: history JSON populates a non-null `action` field for the
// initial memory version, mapped from the underlying `change_reason`.
#[test]
#[serial]
fn history_initial_version_emits_action_created() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "wave2-history-action",
            "--type",
            "note",
            "--description",
            "test note",
            "--body",
            "Hello world. This is a note about Rust.",
        ])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args(["history", "--name", "wave2-history-action"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).expect("stdout must be valid JSON");
    let versions = json["versions"]
        .as_array()
        .expect("versions must be an array");
    assert!(!versions.is_empty(), "versions must not be empty");

    let v0 = &versions[0];
    let action = v0
        .get("action")
        .expect("history version must expose `action`");
    assert!(
        action.is_string(),
        "action must be a string, got: {action:?}"
    );
    assert_eq!(
        action.as_str().unwrap(),
        "created",
        "first version action must be `created`"
    );
}

// Bug M-A5: recall populates a non-null `score` in [0, 1] for every direct match.
#[test]
#[serial]
fn recall_results_carry_score_in_unit_range() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .args([
            "remember",
            "--name",
            "wave2-recall-score",
            "--type",
            "note",
            "--description",
            "rust adapter mechanism",
            "--body",
            "The rust adapter mechanism uses traits and generics to provide flexible type conversions.",
        ])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args(["recall", "rust adapter mechanism", "--k", "5", "--no-graph"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).expect("stdout must be valid JSON");
    let results = json["results"]
        .as_array()
        .expect("results must be an array");
    assert!(!results.is_empty(), "recall must return at least one match");

    for item in results {
        let score = item
            .get("score")
            .expect("every recall item must expose `score`");
        let s = score.as_f64().unwrap_or_else(|| {
            panic!("score must be a number, got {score:?}");
        });
        assert!((0.0..=1.0).contains(&s), "score must be in [0, 1], got {s}");
    }
}

// Bug M-A3: ingest derives kebab names that preserve the base ASCII letters
// of accented and emoji-bearing filenames instead of collapsing them to
// stray characters. Validates via `list` after ingest.
#[test]
#[serial]
fn ingest_unicode_filenames_yield_meaningful_names() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let corpus = tmp.path().join("corpus");
    std::fs::create_dir(&corpus).unwrap();
    std::fs::write(corpus.join("açaí.md"), "acai berry content").unwrap();
    std::fs::write(corpus.join("naïve-test.md"), "naive test content").unwrap();
    std::fs::write(corpus.join("🚀-rocket.md"), "rocket content").unwrap();

    cmd(&tmp)
        .args([
            "ingest",
            corpus.to_str().unwrap(),
            "--type",
            "note",
            "--pattern",
            "*.md",
            "--skip-extraction",
        ])
        .assert()
        .success();

    let output = cmd(&tmp)
        .args(["list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).expect("stdout must be valid JSON");
    let items = json["items"]
        .as_array()
        .or_else(|| json["memories"].as_array())
        .expect("list must return items or memories array");

    let names: Vec<String> = items
        .iter()
        .filter_map(|i| i.get("name").and_then(|n| n.as_str()).map(String::from))
        .collect();

    assert!(
        names.iter().any(|n| n == "acai"),
        "expected an entry named `acai`, got: {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "naive-test"),
        "expected an entry named `naive-test`, got: {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "rocket"),
        "expected an entry named `rocket`, got: {names:?}"
    );
}

// Bug H-M8: chunks_persisted contract for single-chunk vs multi-chunk bodies.
// Single-chunk bodies live in the memories row directly so chunks_persisted=0.
// Multi-chunk bodies persist every chunk so chunks_persisted=chunks_created.
#[test]
#[serial]
fn remember_single_chunk_body_reports_zero_persisted_chunks() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let output = cmd(&tmp)
        .args([
            "remember",
            "--name",
            "wave2-single-chunk",
            "--type",
            "note",
            "--description",
            "short body",
            "--body",
            "Tiny body fits in one chunk.",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).expect("stdout must be valid JSON");
    assert_eq!(
        json["chunks_created"].as_u64().unwrap(),
        1,
        "single-chunk body must report chunks_created=1"
    );
    assert_eq!(
        json["chunks_persisted"].as_u64().unwrap(),
        0,
        "single-chunk body MUST report chunks_persisted=0 \
         because the memories row itself acts as the chunk \
         (no row in memory_chunks)"
    );
}
