//! GAP-E2E-009 (v1.0.88) regression test: `migrate --dry-run` must
//! be accepted by clap, list pending migrations, and exit without
//! mutating the schema or `refinery_schema_history`.
//!
//! The bug being guarded against: `migrate` rejecting `--dry-run`
//! with `error: unexpected argument '--dry-run' found` because the
//! `MigrateArgs` struct omitted the `dry_run` field. After the fix
//! in v1.0.89, the flag parses cleanly and produces a structured
//! `DryRunReport` JSON envelope.
//!
//! The test runs in two phases:
//! 1. `init` creates a fresh database and applies all migrations.
//! 2. `migrate --dry-run --db <p>` is invoked against a brand-new,
//!    empty path. No `refinery_schema_history` exists there, so the
//!    report must list every embedded migration as pending. The
//!    database file MAY be created (open_rw creates an empty DB),
//!    but no schema tables or history rows must be present after
//!    the dry-run.

use rusqlite::Connection;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// Run `sqlite-graphrag migrate --db <p> --dry-run --json` and return
/// `(status, stdout)`.
fn run_dry_run(db_path: &Path) -> (i32, String) {
    let output = Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("--bin")
        .arg("sqlite-graphrag")
        .arg("--")
        .arg("migrate")
        .arg("--db")
        .arg(db_path)
        .arg("--dry-run")
        .arg("--json")
        .output()
        .expect("spawn cargo run migrate --dry-run");
    let status = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    (status, stdout)
}

/// Count the user-defined tables (excluding sqlite_master entries) in
/// a SQLite database file. A value of 0 means dry-run did not apply
/// any schema.
fn count_user_tables(db_path: &Path) -> i64 {
    let conn = match Connection::open(db_path) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        [],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
}

/// GAP-E2E-009: `migrate --dry-run` must be a valid flag (regression
/// test for `MigrateArgs` missing the `dry_run` field).
///
/// Before the fix, clap rejects the flag with exit 2 and
/// `error: unexpected argument '--dry-run' found`. After the fix,
/// the flag parses and the response includes a `pending_migrations`
/// array (or a `pending_count` field) and exits 0 without applying
/// any schema to the target database.
#[test]
fn dry_run_does_not_mutate_schema_history() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("dryrun_target.sqlite");

    // Pre-condition: the target file must not exist (we test dry-run
    // against a path with no schema history).
    assert!(
        !db_path.exists(),
        "precondition: db_path must not exist before invocation"
    );

    let (status, stdout) = run_dry_run(&db_path);

    // The dry-run command must NOT be rejected by clap.
    assert_ne!(
        status, 2,
        "migrate --dry-run must not be rejected by clap; stdout: {stdout}"
    );
    assert!(
        !stdout.contains("unexpected argument '--dry-run'"),
        "migrate must accept --dry-run; got: {stdout}"
    );

    // The dry-run must exit 0 and produce the structured report.
    assert_eq!(status, 0, "dry-run must exit 0; got status {status}");

    // The response must include the `pending_migrations` array and
    // the `status` field from the DryRunReport contract.
    let has_pending_key =
        stdout.contains("\"pending_migrations\"") || stdout.contains("\"pending_count\"");
    assert!(
        has_pending_key,
        "response must include pending_migrations/pending_count; got: {stdout}"
    );
    let has_status = stdout.contains("\"status\"");
    assert!(
        has_status,
        "response must include status field; got: {stdout}"
    );

    // The strongest assertion: no schema was applied. The file may
    // exist (open_rw creates an empty DB) but there must be zero
    // user tables and zero refinery_schema_history rows.
    if db_path.exists() {
        let table_count = count_user_tables(&db_path);
        assert_eq!(
            table_count, 0,
            "dry-run must not create user tables; found {table_count}"
        );

        // Open and verify refinery_schema_history is absent.
        let conn = Connection::open(&db_path).expect("open dry-run db");
        let history_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='refinery_schema_history'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;
        assert!(
            !history_exists,
            "dry-run must not create refinery_schema_history"
        );
    }
}

/// Companion test: dry-run on an already-initialized database must
/// report `ok_no_pending` because all embedded migrations have been
/// applied. This guards against the dry-run path accidentally
/// double-reporting migrations that are already in the history.
#[test]
fn dry_run_on_fresh_db_reports_no_pending_after_init() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("init_then_dryrun.sqlite");

    // Initialize the database (this applies all embedded migrations).
    let init_status = Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("--bin")
        .arg("sqlite-graphrag")
        .arg("--")
        .arg("init")
        .arg("--db")
        .arg(&db_path)
        .arg("--json")
        .status()
        .expect("spawn init");
    assert!(init_status.success(), "init must succeed for this test");

    // Capture the dry-run output for sanity.
    let (status, stdout) = run_dry_run(&db_path);
    assert!(status == 0, "dry-run must succeed; got status {status}");
    assert!(
        stdout.contains("ok_no_pending") || stdout.contains("\"pending_count\": 0"),
        "after init, dry-run must report ok_no_pending or pending_count=0; got: {stdout}"
    );
}
