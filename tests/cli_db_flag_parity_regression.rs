//! GAP-E2E-008 (v1.0.89) regression test: every namespace-scoped subcommand
//! must accept `--db <PATH>` for parity with the rest of the CLI surface.
//!
//! Five subcommands were identified during the v1.0.88 audit as missing the
//! standard `db` field on their `Args` struct:
//!
//! 1. `EmbeddingStatusArgs` (in `src/commands/embedding.rs`)
//! 2. `EmbeddingListArgs`   (in `src/commands/embedding.rs`)
//! 3. `EmbeddingAbandonArgs` (in `src/commands/embedding.rs`)
//! 4. `PendingListArgs`     (in `src/commands/pending.rs`)
//! 5. `PendingShowArgs`     (in `src/commands/pending.rs`)
//!
//! The test invokes each subcommand through the compiled binary with an
//! explicit `--db <PATH>` and asserts that the flag is accepted by clap
//! (i.e. clap does NOT reject it as "unexpected argument" or "unknown
//! option"). A database is initialised in a per-test temp directory via
//! `sqlite-graphrag init --db <path>` so that the subcommands under test
//! find a real schema and execute their storage paths.
//!
//! This is an integration test, not a unit test, because it pins the
//! public CLI surface that operators interact with.

use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// Helper that runs `cargo run --quiet --bin sqlite-graphrag <args>...`
/// with an explicit `--db <PATH>` and returns `(status, stdout, stderr)`.
/// PATH is left untouched; this test does not depend on LLM subprocesses.
fn run_with_db(subcommand_args: &[&str], db_path: &Path) -> (i32, String, String) {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run")
        .arg("--quiet")
        .arg("--bin")
        .arg("sqlite-graphrag")
        .arg("--");
    for a in subcommand_args {
        cmd.arg(a);
    }
    cmd.arg("--db").arg(db_path);

    let output = cmd
        .output()
        .expect("spawn cargo run for cli_db_flag_parity_regression");
    let status = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (status, stdout, stderr)
}

/// Initialises a fresh database at `db_path` so that the subcommands under
/// test find a real schema. Returns the status of the init invocation.
fn init_db(db_path: &Path) -> i32 {
    let output = Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("--bin")
        .arg("sqlite-graphrag")
        .arg("--")
        .arg("init")
        .arg("--db")
        .arg(db_path)
        .output()
        .expect("spawn cargo run for init");
    output.status.code().unwrap_or(-1)
}

/// Sets up a per-test tempdir, initialises a database inside, and runs
/// the closure with the resolved db path. Cleans up on drop.
fn with_initialised_db<F: FnOnce(&Path)>(body: F) {
    let tmp = TempDir::new().expect("tempdir for cli_db_flag_parity_regression");
    let db_path = tmp.path().join("parity.sqlite");
    let init_status = init_db(&db_path);
    assert!(
        init_status == 0,
        "FATAL: `init --db {}` returned status={}; cannot run parity checks. \
         Test setup requires a bootstrapped database.",
        db_path.display(),
        init_status
    );
    body(&db_path);
}

/// Asserts that clap accepted the `--db` flag. The two failure shapes we
/// guard against are:
///   1. clap rejects with "unexpected argument" / "unknown option"
///   2. clap rejects with "error: the following required arguments were
///      not provided" pointing at the db slot
///
/// We treat any clap-level error message (status != 0 AND stderr mentions
/// "error:" or "unrecognized" or "unexpected") as a regression.
fn assert_db_flag_accepted(label: &str, subcommand_args: &[&str], db_path: &Path) {
    let (status, stdout, stderr) = run_with_db(subcommand_args, db_path);

    // The clap-rejection signature is "error:" / "unrecognized argument" /
    // "unexpected argument" in stderr with status 2. Storage errors at
    // runtime (status 4, 10, etc.) are acceptable as long as clap accepted
    // the flag — those prove the arg reached the handler.
    let clap_rejected = stderr.contains("error:")
        || stderr.contains("unrecognized")
        || stderr.contains("unexpected argument")
        || stderr.contains("unknown option");

    assert!(
        !clap_rejected,
        "REGRESSION GAP-E2E-008: subcommand `{label}` rejected `--db` flag.\n\
         stderr: {stderr}\nstdout: {stdout}\nstatus: {status}\n\
         Expected: clap accepts `--db <PATH>` as a valid argument.\n\
         The Args struct for this subcommand is missing the standard \
         `#[arg(long, env = \"SQLITE_GRAPHRAG_DB_PATH\")] pub db: Option<String>` field.",
    );
}

// ---------------------------------------------------------------------------
// EmbeddingStatusArgs — embedding status
// ---------------------------------------------------------------------------

#[test]
fn assert_db_flag_on_embedding_status() {
    with_initialised_db(|db_path| {
        assert_db_flag_accepted("embedding status", &["embedding", "status"], db_path);
    });
}

// ---------------------------------------------------------------------------
// EmbeddingListArgs — embedding list
// ---------------------------------------------------------------------------

#[test]
fn assert_db_flag_on_embedding_list() {
    with_initialised_db(|db_path| {
        assert_db_flag_accepted(
            "embedding list",
            &["embedding", "list", "--limit", "10"],
            db_path,
        );
    });
}

// ---------------------------------------------------------------------------
// EmbeddingAbandonArgs — embedding abandon
// ---------------------------------------------------------------------------

#[test]
fn assert_db_flag_on_embedding_abandon() {
    with_initialised_db(|db_path| {
        // Use an obviously invalid pending_id so the storage layer rejects
        // it (exit 4, NotFound) — that proves the `--db` flag reached the
        // handler and `AppPaths::resolve` opened the database at the given
        // path.
        assert_db_flag_accepted(
            "embedding abandon <id>",
            &["embedding", "abandon", "999999", "--yes"],
            db_path,
        );
    });
}

// ---------------------------------------------------------------------------
// PendingListArgs — pending list
// ---------------------------------------------------------------------------

#[test]
fn assert_db_flag_on_pending_list() {
    with_initialised_db(|db_path| {
        assert_db_flag_accepted(
            "pending list",
            &["pending", "list", "--limit", "10"],
            db_path,
        );
    });
}

// ---------------------------------------------------------------------------
// PendingShowArgs — pending show
// ---------------------------------------------------------------------------

#[test]
fn assert_db_flag_on_pending_show() {
    with_initialised_db(|db_path| {
        // pending_id 0 is a guaranteed-missing row; storage layer returns
        // NotFound (exit 4). The fact that `--db` reached the handler is
        // proven by the storage path executing against our temp database.
        assert_db_flag_accepted("pending show <id>", &["pending", "show", "0"], db_path);
    });
}

// ---------------------------------------------------------------------------
// Aggregation — every regression assertion in one place for `cargo test`
// grep filters like `-- assert_db_flag_on_all_namespace_subcommands`.
// ---------------------------------------------------------------------------

/// Single entrypoint that exercises all five subcommands in sequence so
/// a CI runner can run a single test name and assert the entire surface.
#[test]
fn assert_db_flag_on_all_namespace_subcommands() {
    with_initialised_db(|db_path| {
        assert_db_flag_accepted("embedding status", &["embedding", "status"], db_path);
        assert_db_flag_accepted(
            "embedding list",
            &["embedding", "list", "--limit", "10"],
            db_path,
        );
        assert_db_flag_accepted(
            "embedding abandon <id>",
            &["embedding", "abandon", "999999", "--yes"],
            db_path,
        );
        assert_db_flag_accepted(
            "pending list",
            &["pending", "list", "--limit", "10"],
            db_path,
        );
        assert_db_flag_accepted("pending show <id>", &["pending", "show", "0"], db_path);
    });
}
