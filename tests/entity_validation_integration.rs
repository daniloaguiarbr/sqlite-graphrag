use assert_cmd::Command;
use serial_test::serial;
use tempfile::TempDir;

fn cmd_base(tmp: &TempDir) -> Command {
    let mut c = Command::cargo_bin("sqlite-graphrag").unwrap();
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
fn entity_name_all_caps_short_rejected_via_link() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

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
