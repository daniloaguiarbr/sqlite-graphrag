//! Integration tests for v1.0.45 features:
//! - A1: FTS5 query preprocessing handles compound terms with separators
//! - S5: `--enable-ner` flag and `SQLITE_GRAPHRAG_ENABLE_NER` env var

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

fn cmd(temp: &TempDir) -> Command {
    let cache = temp.path().join("cache");
    let mut c = sgr_cmd();
    let mock_dir = common::mock_llm_path();
    c.env_clear()
        .env("HOME", temp.path())
        .env("SQLITE_GRAPHRAG_HOME", temp.path())
        .env("SQLITE_GRAPHRAG_CACHE_DIR", &cache)
        .env("SQLITE_GRAPHRAG_DAEMON_DISABLE_AUTOSTART", "1")
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

fn init_db(tmp: &TempDir) {
    cmd(tmp).arg("init").assert().success();
}

fn remember_with_body(tmp: &TempDir, name: &str, body: &str) {
    cmd(tmp)
        .args([
            "remember",
            "--name",
            name,
            "--type",
            "note",
            "--description",
            "test memory",
            "--body",
            body,
            "--skip-extraction",
        ])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// A1: FTS5 compound term search via hybrid-search
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn hybrid_search_finds_hyphenated_compound_term() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);
    remember_with_body(
        &tmp,
        "fts-hyphen-test",
        "the graphrag-precompact script runs daily",
    );

    let output = cmd(&tmp)
        .args(["hybrid-search", "graphrag-precompact", "--k", "5"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let names: Vec<&str> = json["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"fts-hyphen-test"),
        "should find memory by hyphenated term; got {names:?}"
    );
}

#[test]
#[serial]
fn hybrid_search_finds_dotted_version() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);
    remember_with_body(
        &tmp,
        "fts-dot-test",
        "release notes for v1.0.44 are published",
    );

    let output = cmd(&tmp)
        .args(["hybrid-search", "v1.0.44", "--k", "5"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let names: Vec<&str> = json["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"fts-dot-test"),
        "should find memory by dotted version; got {names:?}"
    );
}

// ---------------------------------------------------------------------------
// S5: SQLITE_GRAPHRAG_ENABLE_NER env var accepts 1/true
// ---------------------------------------------------------------------------
// v1.0.76 removed GLiNER and NER. The flag is still accepted (it parses
// without error) but no entity extraction is performed because the
// pipeline that consumes the flag was deleted. These tests now exercise
// the validation path: the flag MUST coexist with the absence of
// --skip-extraction or the CLI refuses. We mark them `#[ignore]` until
// the v1.0.77 cleanup either restores NER or deletes the flag.

#[test]
#[serial]
// TODO v1.0.89: aguardando decisão arquitetural — NER removido em v1.0.76 (ADR-0025); avaliar se deve ser restaurado via glue de subprocesso LLM (similar ao embedding G42).
#[ignore = "NER removed in v1.0.76; see ADR-0025"]
fn enable_ner_env_var_accepts_1() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .env("SQLITE_GRAPHRAG_ENABLE_NER", "1")
        .args([
            "remember",
            "--name",
            "ner-env-1",
            "--type",
            "note",
            "--description",
            "env var test with value 1",
            "--body",
            "Microsoft announced a deal",
            "--skip-extraction",
        ])
        .assert()
        .success();
}

#[test]
// TODO v1.0.89: aguardando decisão arquitetural — NER removido em v1.0.76 (ADR-0025); avaliar se deve ser restaurado via glue de subprocesso LLM (similar ao embedding G42).
#[serial]
#[ignore = "NER removed in v1.0.76; see ADR-0025"]
fn enable_ner_env_var_accepts_true() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp)
        .env("SQLITE_GRAPHRAG_ENABLE_NER", "true")
        .args([
            "remember",
            "--name",
            "ner-env-true",
            "--type",
            "note",
            "--description",
            "env var test with value true",
            "--body",
            "Google acquired DeepMind",
            "--skip-extraction",
        ])
        .assert()
        .success();
}
