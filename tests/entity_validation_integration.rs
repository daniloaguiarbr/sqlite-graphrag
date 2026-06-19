use assert_cmd::Command;
use serial_test::serial;
use tempfile::TempDir;

/// Builds a fresh `Command` with the mock LLM PATH prepended.
///
/// v1.0.76 spawns `claude` or `codex` on every `remember` / `ingest` /
/// `edit`. The bundled mocks under `tests/mock-llm/` return a fixed
/// 64-dim zero vector so the binary finishes without a real OAuth
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

fn cmd_base(tmp: &TempDir) -> Command {
    let mut c = sgr_cmd();
    c.env("SQLITE_GRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"));
    c.env("SQLITE_GRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("SQLITE_GRAPHRAG_LOG_LEVEL", "error");
    c.arg("--skip-memory-guard");
    c
}

fn init_db(tmp: &TempDir) {
    cmd_base(tmp).arg("init").assert().success();
}

fn create_entity(tmp: &TempDir, name: &str) {
    cmd_base(tmp)
        .args([
            "link",
            "--from",
            name,
            "--to",
            "e2e-anchor-entity",
            "--relation",
            "related",
            "--create-missing",
        ])
        .assert()
        .success();
}

#[test]
#[serial]
fn entity_name_too_short_rejected_via_link() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd_base(&tmp)
        .args([
            "link",
            "--from",
            "x",
            "--to",
            "valid-entity",
            "--relation",
            "uses",
            "--create-missing",
        ])
        .assert()
        .failure();
}

#[test]
#[serial]
fn entity_name_all_caps_short_rejected_via_link_v1088() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    // ADR-0046 / BUG-13 v1.0.88: link --create-missing now validates the
    // ORIGINAL args.from BEFORE normalization, so short ALL_CAPS names
    // (≤4 chars, all uppercase) are rejected even though normalize would
    // produce "ram". This restores parity with rename-entity and remember
    // --graph-stdin which already validated the original.
    cmd_base(&tmp)
        .args([
            "link",
            "--from",
            "RAM",
            "--to",
            "valid-entity",
            "--relation",
            "uses",
            "--create-missing",
        ])
        .assert()
        .failure();
}

#[test]
#[serial]
fn entity_name_valid_passes_via_link() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd_base(&tmp)
        .args([
            "link",
            "--from",
            "valid-name",
            "--to",
            "valid-target",
            "--relation",
            "uses",
            "--create-missing",
        ])
        .assert()
        .success();
}

#[test]
#[serial]
fn rename_entity_rejects_short_new_name() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);
    create_entity(&tmp, "rename-source-entity");

    cmd_base(&tmp)
        .args([
            "rename-entity",
            "--name",
            "rename-source-entity",
            "--new-name",
            "z",
        ])
        .assert()
        .failure();
}

#[test]
#[serial]
fn rename_entity_rejects_all_caps_short_new_name() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);
    create_entity(&tmp, "rename-caps-entity");

    cmd_base(&tmp)
        .args([
            "rename-entity",
            "--name",
            "rename-caps-entity",
            "--new-name",
            "WAL",
        ])
        .assert()
        .failure();
}

#[test]
#[serial]
fn link_rejects_all_caps_short_to_arg_v1088() {
    // ADR-0046 / BUG-13 v1.0.88: --to side must also validate the original
    // before normalize, otherwise "--to API" would silently create entity "api".
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd_base(&tmp)
        .args([
            "link",
            "--from",
            "valid-source",
            "--to",
            "API",
            "--relation",
            "uses",
            "--create-missing",
        ])
        .assert()
        .failure();
}

#[test]
#[serial]
fn link_rejects_four_char_all_caps_v1088() {
    // ADR-0046 / BUG-13 v1.0.88: 4-char ALL_CAPS ("RUST") must also be rejected.
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd_base(&tmp)
        .args([
            "link",
            "--from",
            "RUST",
            "--to",
            "valid-target",
            "--relation",
            "uses",
            "--create-missing",
        ])
        .assert()
        .failure();
}

#[test]
#[serial]
fn link_accepts_five_char_all_caps_v1088() {
    // ADR-0046 / BUG-13 v1.0.88: 5+ char ALL_CAPS passes (e.g. "RUSTC", "LLMDS").
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd_base(&tmp)
        .args([
            "link",
            "--from",
            "RUSTC",
            "--to",
            "valid-target",
            "--relation",
            "uses",
            "--create-missing",
        ])
        .assert()
        .success();
}
