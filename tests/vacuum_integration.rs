use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn cmd(tmp: &TempDir) -> Command {
    let mut c = Command::cargo_bin("neurographrag").unwrap();
    c.env("NEUROGRAPHRAG_DB_PATH", tmp.path().join("test.sqlite"));
    c.env("NEUROGRAPHRAG_CACHE_DIR", tmp.path().join("cache"));
    c.env("NEUROGRAPHRAG_LOG_LEVEL", "error");
    c
}

fn init_db(tmp: &TempDir) {
    cmd(tmp).arg("init").assert().success();
}

#[test]
fn test_vacuum_falha_sem_init() {
    let tmp = TempDir::new().unwrap();
    cmd(&tmp)
        .arg("vacuum")
        .assert()
        .failure()
        .stderr(predicate::str::contains("neurographrag init"));
}

#[test]
fn test_vacuum_sucesso_apos_init() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    cmd(&tmp).arg("vacuum").assert().success();
}

#[test]
fn test_vacuum_retorna_json_com_status_ok() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let output = cmd(&tmp)
        .arg("vacuum")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["status"], "ok");
}

#[test]
fn test_vacuum_json_contem_db_path() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let output = cmd(&tmp)
        .arg("vacuum")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(json["db_path"].is_string());
    assert!(!json["db_path"].as_str().unwrap().is_empty());
}

#[test]
fn test_vacuum_json_contem_tamanhos() {
    let tmp = TempDir::new().unwrap();
    init_db(&tmp);

    let output = cmd(&tmp)
        .arg("vacuum")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(json["size_before_bytes"].is_number());
    assert!(json["size_after_bytes"].is_number());
}

#[test]
fn test_vacuum_via_env_db_path() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("custom.sqlite");

    let mut init_cmd = Command::cargo_bin("neurographrag").unwrap();
    init_cmd
        .env("NEUROGRAPHRAG_DB_PATH", &db_path)
        .env("NEUROGRAPHRAG_CACHE_DIR", tmp.path().join("cache"))
        .env("NEUROGRAPHRAG_LOG_LEVEL", "error")
        .arg("init")
        .assert()
        .success();

    let mut vac_cmd = Command::cargo_bin("neurographrag").unwrap();
    vac_cmd
        .env("NEUROGRAPHRAG_DB_PATH", &db_path)
        .env("NEUROGRAPHRAG_CACHE_DIR", tmp.path().join("cache"))
        .env("NEUROGRAPHRAG_LOG_LEVEL", "error")
        .arg("vacuum")
        .assert()
        .success();
}
