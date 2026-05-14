//! Integration tests for v1.0.45 features:
//! - A1: FTS5 query preprocessing handles compound terms with separators
//! - S5: `--enable-ner` flag and `SQLITE_GRAPHRAG_ENABLE_NER` env var

use assert_cmd::Command;
use serial_test::serial;
use tempfile::TempDir;

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

#[test]
#[serial]
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
#[serial]
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
