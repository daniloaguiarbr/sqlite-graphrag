//! GAP-003 (v1.0.88) regression test: `health --db <nonexistent> --json`
//! must return a non-zero exit (or an `error: true` JSON envelope).
//! The bug being guarded against: silently creating the database on
//! `health` invocation, which would mask operator typos in the
//! `--db` path.
//!
//! The test is integration-level (invokes the compiled binary through
//! `cargo run --bin sqlite-graphrag`) so it covers the full CLI surface
//! that operators interact with.

use std::process::Command;
use tempfile::TempDir;

/// Invoke `sqlite-graphrag health --db <path> --json` via `cargo run`
/// and return `(status, stdout)`. We pin `--quiet` so the cargo log
/// does not pollute stdout.
fn run_health(db_path: &std::path::Path) -> (i32, String) {
    let output = Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("--bin")
        .arg("sqlite-graphrag")
        .arg("--")
        .arg("health")
        .arg("--db")
        .arg(db_path)
        .arg("--json")
        .output()
        .expect("spawn cargo run");
    let status = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    (status, stdout)
}

/// GAP-003: a non-existent database path must NOT be silently created.
/// The expected behaviour is one of:
///
/// 1. non-zero exit code (validation/database error path)
/// 2. JSON envelope with `"error": true`
///
/// If the database is created (i.e. the file appears on disk after
/// the invocation), the test FAILS — this is the regression we guard
/// against.
#[test]
fn health_nonexistent_db_returns_error_not_silent_create() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("does_not_exist_yet.sqlite");
    // Pre-condition: the file must not exist.
    assert!(
        !db_path.exists(),
        "precondition: db_path must not exist before invocation"
    );

    let (status, stdout) = run_health(&db_path);

    // After the invocation: the file must STILL not exist (no silent create).
    let file_created = db_path.exists();
    let json_has_error = stdout.contains("\"error\": true")
        || stdout.contains("\"error\":true")
        || stdout.contains("\"code\": 10") // AppError::Database exit 10
        || stdout.contains("\"code\":10")
        || stdout.contains("\"code\": 4"); // AppError::NotFound exit 4

    assert!(
        !file_created,
        "REGRESSION: `health --db <nonexistent>` silently created the database at {}. \
         The CLI must surface a validation/database error instead of bootstrapping an empty file.\n\
         stdout: {}\nstatus: {}",
        db_path.display(),
        stdout,
        status,
    );
    assert!(
        status != 0 || json_has_error,
        "REGRESSION: `health --db <nonexistent>` returned status={status} and stdout that does not \
         contain an error envelope.\nstdout: {stdout}",
    );
}
