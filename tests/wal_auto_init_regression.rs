//! Regression test for the WAL auto-init bug found during the v1.0.34 audit.
//!
//! Before the fix: `remember`/`ingest`/etc. (auto-init path) created databases
//! in `journal_mode = delete`. Only explicit `init` activated WAL.
//! After the fix: every command that goes through `ensure_db_ready()` ends with
//! `journal_mode = wal`, matching the documented contract.

use assert_cmd::Command;
use rusqlite::Connection;
use tempfile::TempDir;

fn assert_wal_after(cmd_args: &[&str], description: &str) {
    let tmp = TempDir::new().expect("create tempdir");
    let db_path = tmp.path().join("graphrag.sqlite");

    let mut cmd = Command::cargo_bin("sqlite-graphrag").expect("binary build");
    cmd.env("SQLITE_GRAPHRAG_DB_PATH", &db_path)
        .env("SQLITE_GRAPHRAG_HOME", tmp.path())
        .env_remove("SQLITE_GRAPHRAG_LANG")
        .args(cmd_args)
        .assert()
        .success();

    let conn = Connection::open(&db_path).expect("open db for assertion");
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .expect("query journal_mode");
    assert_eq!(
        mode, "wal",
        "expected journal_mode=wal after `{description}`, got `{mode}`"
    );
}

#[test]
fn wal_after_remember_skip_extraction() {
    assert_wal_after(
        &[
            "remember",
            "--name",
            "auto-init-test",
            "--type",
            "document",
            "--description",
            "wal regression test",
            "--body",
            "smoke",
            "--skip-extraction",
            "--json",
        ],
        "remember --skip-extraction",
    );
}

#[test]
fn wal_after_explicit_init() {
    assert_wal_after(&["init", "--json"], "init");
}

#[test]
fn wal_after_list_on_fresh_dir() {
    assert_wal_after(&["list", "--json"], "list (auto-init)");
}
