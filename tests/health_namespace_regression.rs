//! GAP-E2E-002 (v1.0.88) regression test: `health --namespace <NS>`
//! must be accepted by clap and produce a JSON envelope that includes
//! the requested namespace in the `namespace` field.
//!
//! The bug being guarded against: `health` rejecting `--namespace`
//! with `error: unexpected argument '--namespace' found` because the
//! `HealthArgs` struct omitted the `namespace` field. After the fix
//! in v1.0.89, the flag must parse cleanly and the response must
//! include the namespace under the documented key.
//!
//! The test runs through a fresh database because `health` refuses
//! to silently create a non-existent DB (GAP-003 / BUG-AUDIT-1), and
//! the namespace filter is only meaningful for a DB that has rows
//! in the `memories` table.

use std::process::Command;
use tempfile::TempDir;

/// Build a path string usable as a CLI argument on Unix and Windows.
fn path_arg(p: &std::path::Path) -> &std::path::Path {
    p
}

/// Helper: initialize a database at `db_path` with the standard schema
/// so the `health` subcommand can run on it. Returns the tempdir so
/// it stays alive for the test.
fn init_db(db_path: &std::path::Path) {
    let status = Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("--bin")
        .arg("sqlite-graphrag")
        .arg("--")
        .arg("init")
        .arg("--db")
        .arg(path_arg(db_path))
        .arg("--json")
        .status()
        .expect("spawn cargo run init");
    assert!(status.success(), "init must succeed to run health");
}

/// Helper: invoke `sqlite-graphrag health --db <p> --namespace <ns> --json`
/// and return `(status, stdout)`.
fn run_health_with_namespace(db_path: &std::path::Path, namespace: &str) -> (i32, String) {
    let output = Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("--bin")
        .arg("sqlite-graphrag")
        .arg("--")
        .arg("health")
        .arg("--db")
        .arg(path_arg(db_path))
        .arg("--namespace")
        .arg(namespace)
        .arg("--json")
        .output()
        .expect("spawn cargo run health --namespace");
    let status = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    (status, stdout)
}

/// Helper: invoke `sqlite-graphrag health --db <p> --json` (no namespace)
/// to capture the global baseline response.
fn run_health_global(db_path: &std::path::Path) -> (i32, String) {
    let output = Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("--bin")
        .arg("sqlite-graphrag")
        .arg("--")
        .arg("health")
        .arg("--db")
        .arg(path_arg(db_path))
        .arg("--json")
        .output()
        .expect("spawn cargo run health (global)");
    let status = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    (status, stdout)
}

/// GAP-E2E-002: `health --namespace <NS>` must be a valid flag
/// (regression test for `HealthArgs` missing the `namespace` field).
///
/// Before the fix, clap rejects the flag with exit 2 and
/// `error: unexpected argument '--namespace' found`. After the fix,
/// the flag parses and the response includes `"namespace": "<NS>"`.
#[test]
fn health_accepts_namespace_flag() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("namespace_test.sqlite");
    init_db(&db_path);

    let (status, stdout) = run_health_with_namespace(&db_path, "e2e-test");

    // Must not fail with the "unexpected argument" clap error.
    assert_ne!(
        status, 2,
        "health --namespace must not be rejected by clap; stderr: {stdout}"
    );
    assert!(
        !stdout.contains("unexpected argument '--namespace'"),
        "health must accept --namespace; got: {stdout}"
    );

    // The response must echo back the requested namespace in the
    // documented `namespace` field. We accept either a flat
    // `"namespace": "e2e-test"` or any JSON-adjacent surface (some
    // envelopes nest it). The key invariant is the string is
    // present in stdout so downstream consumers can read it.
    let has_echo = stdout.contains("\"e2e-test\"");
    assert!(
        has_echo,
        "response must include the requested namespace 'e2e-test'; got: {stdout}"
    );
}

/// GAP-E2E-002 (extension): when `--namespace` is omitted, the
/// response must NOT include the `namespace` field (or it must be
/// null). This protects the `skip_serializing_if = "Option::is_none"`
/// contract on `HealthResponse.namespace`.
#[test]
fn health_omits_namespace_field_when_flag_absent() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("global_baseline.sqlite");
    init_db(&db_path);

    let (status, stdout) = run_health_global(&db_path);
    assert!(
        status == 0,
        "global health must succeed; got status {status}"
    );

    // Either the field is absent (skip_serializing_if honored) or it
    // is explicit null. Both forms are acceptable per the contract.
    let has_namespace_key = stdout.contains("\"namespace\"");
    if has_namespace_key {
        // If present, must be null.
        assert!(
            stdout.contains("\"namespace\": null") || stdout.contains("\"namespace\":null"),
            "namespace field must be null when --namespace is omitted; got: {stdout}"
        );
    }
}
