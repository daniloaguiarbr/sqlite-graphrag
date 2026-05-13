//! Integration tests for v1.0.44 features:
//! - H3: `related` falls back to bare entity when no memory matches the seed name
//! - H4: `rename` accepts `<NEW>` as a second positional argument (mv-style)
//! - M6: `cache list` emits structured JSON with `schema_version = 1`
//! - B1 regression: README bash blocks contain no inline `# comment` after a CLI command

use assert_cmd::Command;
use predicates::prelude::*;
use serial_test::serial;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helper: build a Command with a fully isolated environment.
// ---------------------------------------------------------------------------

fn cmd(temp: &TempDir) -> Command {
    let mut c = Command::cargo_bin("sqlite-graphrag").expect("binary present in target/");
    c.env_clear()
        .env("HOME", temp.path())
        .env("SQLITE_GRAPHRAG_HOME", temp.path())
        .env(
            "SQLITE_GRAPHRAG_CACHE_DIR",
            temp.path().join("cache").to_string_lossy().to_string(),
        )
        .env("SQLITE_GRAPHRAG_LANG", "en")
        .env("SQLITE_GRAPHRAG_LOG_LEVEL", "warn")
        .current_dir(temp.path());
    c
}

// ---------------------------------------------------------------------------
// H3 — `related` entity-fallback
// ---------------------------------------------------------------------------

/// When the seed name matches a bare entity (not a memory name), `related`
/// must succeed via the entity fallback path added in H3.
///
/// Entities are injected via `--entities-file` to avoid needing the BERT NER
/// model in CI; then `link` creates a relationship so `related` has edges to
/// traverse.
#[test]
#[serial]
fn related_entity_seed_via_link_succeeds() {
    let temp = TempDir::new().expect("create temp dir");

    // Write a minimal entities JSON file for alice-note.
    let entities_alice = temp.path().join("entities_alice.json");
    std::fs::write(
        &entities_alice,
        r#"[{"name":"Alice","entity_type":"person"},{"name":"Acme","entity_type":"organization"}]"#,
    )
    .expect("write entities_alice.json");

    // Write a minimal entities JSON file for bob-note.
    let entities_bob = temp.path().join("entities_bob.json");
    std::fs::write(
        &entities_bob,
        r#"[{"name":"Bob","entity_type":"person"},{"name":"Acme","entity_type":"organization"}]"#,
    )
    .expect("write entities_bob.json");

    // Persist alice-note with injected entities (no BERT model needed).
    cmd(&temp)
        .args([
            "remember",
            "--name",
            "alice-note",
            "--type",
            "user",
            "--description",
            "note about alice",
            "--body",
            "Alice is a software engineer",
            "--entities-file",
            entities_alice.to_str().expect("utf-8 path"),
        ])
        .assert()
        .success();

    // Persist bob-note with injected entities.
    cmd(&temp)
        .args([
            "remember",
            "--name",
            "bob-note",
            "--type",
            "user",
            "--description",
            "note about bob",
            "--body",
            "Bob is a software engineer",
            "--entities-file",
            entities_bob.to_str().expect("utf-8 path"),
        ])
        .assert()
        .success();

    // Now "Alice" and "Bob" exist as graph entities; link them.
    cmd(&temp)
        .args([
            "link",
            "--from",
            "Alice",
            "--to",
            "Bob",
            "--relation",
            "related",
        ])
        .assert()
        .success();

    // `related Alice` — "Alice" is a bare entity, not a memory name.
    // The H3 fallback must locate it and return success.
    cmd(&temp)
        .args(["related", "Alice", "--max-hops", "2"])
        .assert()
        .success();
}

/// When neither a memory nor an entity named `<seed>` exists, `related` must
/// exit with failure and surface a "not found" message in stderr.
#[test]
#[serial]
fn related_errors_when_neither_memory_nor_entity_exists() {
    let temp = TempDir::new().expect("create temp dir");

    // Warm up auto-init; --skip-extraction avoids model requirement.
    cmd(&temp)
        .args([
            "remember",
            "--name",
            "warm-up",
            "--type",
            "note",
            "--description",
            "init db",
            "--body",
            "warm up",
            "--skip-extraction",
        ])
        .assert()
        .success();

    cmd(&temp)
        .args(["related", "definitely-does-not-exist-xyz-8675309"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found").or(predicate::str::contains("Not Found")));
}

// ---------------------------------------------------------------------------
// H4 — `rename` positional NEW argument (mv-style)
// ---------------------------------------------------------------------------

/// `rename OLD NEW` — both arguments positional — must rename the memory and
/// leave the new name readable.
#[test]
#[serial]
fn rename_positional_new_name_works() {
    let temp = TempDir::new().expect("create temp dir");

    cmd(&temp)
        .args([
            "remember",
            "--name",
            "old-name",
            "--type",
            "note",
            "--description",
            "test",
            "--body",
            "hello rename",
            "--skip-extraction",
        ])
        .assert()
        .success();

    // mv-style: rename OLD NEW
    cmd(&temp)
        .args(["rename", "old-name", "new-name"])
        .assert()
        .success();

    // old name must be gone; new name must return the original body.
    cmd(&temp).args(["read", "old-name"]).assert().failure();

    cmd(&temp)
        .args(["read", "new-name"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello rename"));
}

/// `rename OLD --new-name NEW` (legacy flag form) must still work after the
/// positional NEW was added so existing integrations are not broken.
#[test]
#[serial]
fn rename_legacy_new_name_flag_still_works() {
    let temp = TempDir::new().expect("create temp dir");

    cmd(&temp)
        .args([
            "remember",
            "--name",
            "foo",
            "--type",
            "note",
            "--description",
            "test",
            "--body",
            "bar body",
            "--skip-extraction",
        ])
        .assert()
        .success();

    cmd(&temp)
        .args(["rename", "foo", "--new-name", "bar"])
        .assert()
        .success();

    cmd(&temp)
        .args(["read", "bar"])
        .assert()
        .success()
        .stdout(predicate::str::contains("bar body"));
}

/// Providing both the positional NEW and the `--new-name` flag simultaneously
/// must be rejected by clap (conflicts_with).
#[test]
#[serial]
fn rename_rejects_both_positional_new_and_flag() {
    let temp = TempDir::new().expect("create temp dir");

    cmd(&temp)
        .args([
            "remember",
            "--name",
            "conflict-test",
            "--type",
            "note",
            "--description",
            "test",
            "--body",
            "body",
            "--skip-extraction",
        ])
        .assert()
        .success();

    // Both positional NEW ("bar") and --new-name ("baz") provided — clap must reject.
    cmd(&temp)
        .args(["rename", "conflict-test", "bar", "--new-name", "baz"])
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// M6 — `cache list` JSON output
// ---------------------------------------------------------------------------

/// `cache list --json` must emit valid JSON with `schema_version = 1` and a
/// `files` array regardless of whether any model files are present.
#[test]
#[serial]
fn cache_list_json_returns_schema_version_1() {
    let temp = TempDir::new().expect("create temp dir");

    let output = cmd(&temp)
        .args(["cache", "list", "--json"])
        .output()
        .expect("run cache list --json");

    assert!(
        output.status.success(),
        "cache list --json failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("cache list --json must produce valid JSON");

    assert_eq!(
        parsed["schema_version"], 1,
        "schema_version must be 1; got: {parsed}"
    );
    assert!(
        parsed["files"].is_array(),
        "files must be a JSON array; got: {parsed}"
    );
    assert!(
        parsed.get("total_bytes").is_some(),
        "total_bytes field must be present; got: {parsed}"
    );
}

/// `cache list` (text mode) must either show a TOTAL footer (when files exist)
/// or print "(empty)" when the cache directory is absent or empty.
#[test]
#[serial]
fn cache_list_text_shows_total_or_empty() {
    let temp = TempDir::new().expect("create temp dir");

    cmd(&temp)
        .args(["cache", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("TOTAL").or(predicate::str::contains("(empty)")));
}

// ---------------------------------------------------------------------------
// B1 regression — no inline shell comments after CLI commands in README blocks
// ---------------------------------------------------------------------------

/// Every sqlite-graphrag invocation line inside an **executable** ```bash block
/// (i.e. blocks NOT preceded by `<!-- skip-test -->`) in README.md and
/// README.pt-BR.md must NOT contain an inline ` # ` comment suffix.
///
/// Such suffixes pass the comment text as a literal CLI argument, breaking
/// the readme_examples_executable.rs harness (B1 regression in v1.0.43).
/// Blocks marked `<!-- skip-test -->` are exempted because they document
/// shell pipelines or commands that need external state.
#[test]
fn readme_bash_blocks_have_no_inline_shell_comments_after_command() {
    for readme in &["README.md", "README.pt-BR.md"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(readme);
        let content =
            std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("could not read {readme}"));

        let mut in_bash = false;
        let mut skip_block = false;
        let mut last_non_blank: Option<String> = None;

        for (i, line) in content.lines().enumerate() {
            let trimmed = line.trim_end_matches('\r');
            let trimmed_start = trimmed.trim_start();

            if !in_bash {
                if trimmed_start.starts_with("```bash") {
                    // Detect skip-test marker on the most recent non-blank line.
                    skip_block = last_non_blank
                        .as_deref()
                        .map(|s| s.contains("<!-- skip-test"))
                        .unwrap_or(false);
                    in_bash = true;
                } else if !trimmed.trim().is_empty() {
                    last_non_blank = Some(trimmed.to_string());
                }
            } else if trimmed_start.starts_with("```") {
                in_bash = false;
                skip_block = false;
                last_non_blank = Some(trimmed.to_string());
            } else if !skip_block
                && trimmed_start.starts_with("sqlite-graphrag")
                && line.contains(" # ")
            {
                panic!(
                    "inline shell comment on executable CLI line in {readme}:{}: {:?}",
                    i + 1,
                    line
                );
            }
        }
    }
}
