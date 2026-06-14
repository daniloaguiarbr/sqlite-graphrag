//! End-to-end tests for the `completions` subcommand (A1 G8).
//!
//! v1.0.80 closes the gap where the 5 supported shells (bash, zsh, fish,
//! powershell, elvish) had zero test coverage. Each test invokes the CLI
//! binary built in `target/debug/sqlite-graphrag` and asserts the output
//! contains the expected completion script markers.
//!
//! These tests REQUIRE a local debug build. If the binary is missing they
//! auto-skip via the `binary_exists` check at the top of each test.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn workspace_root() -> PathBuf {
    // tests/ lives one level below the workspace root.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p
}

fn debug_binary() -> PathBuf {
    workspace_root()
        .join("target")
        .join("debug")
        .join("sqlite-graphrag")
}

fn binary_exists() -> bool {
    debug_binary().exists()
}

fn run_completions(shell: &str) -> (i32, String, String) {
    let mut cmd = Command::new(debug_binary());
    cmd.arg("completions").arg(shell);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let output = cmd.output().expect("spawn sqlite-graphrag completions");
    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (code, stdout, stderr)
}

#[test]
fn completions_bash_emits_script() {
    if !binary_exists() {
        eprintln!("skipping: debug binary not found at {:?}", debug_binary());
        return;
    }
    let (code, stdout, stderr) = run_completions("bash");
    assert_eq!(code, 0, "bash completions exited non-zero: stderr={stderr}");
    // bash completions for clap use a `_CLI_COMPLETE` source guard.
    assert!(
        stdout.contains("complete") || stdout.contains("_sqlite-graphrag"),
        "bash completions output missing completion markers: {}",
        &stdout[..stdout.len().min(200)]
    );
}

#[test]
fn completions_zsh_emits_script() {
    if !binary_exists() {
        eprintln!("skipping: debug binary not found");
        return;
    }
    let (code, stdout, stderr) = run_completions("zsh");
    assert_eq!(code, 0, "zsh completions exited non-zero: stderr={stderr}");
    assert!(
        stdout.contains("#compdef") || stdout.contains("_sqlite-graphrag"),
        "zsh completions output missing completion markers"
    );
}

#[test]
fn completions_fish_emits_script() {
    if !binary_exists() {
        eprintln!("skipping: debug binary not found");
        return;
    }
    let (code, stdout, stderr) = run_completions("fish");
    assert_eq!(code, 0, "fish completions exited non-zero: stderr={stderr}");
    assert!(
        stdout.contains("complete") || stdout.contains("sqlite-graphrag"),
        "fish completions output missing completion markers"
    );
}

#[test]
fn completions_powershell_emits_script() {
    if !binary_exists() {
        eprintln!("skipping: debug binary not found");
        return;
    }
    let (code, stdout, stderr) = run_completions("powershell");
    assert_eq!(
        code, 0,
        "powershell completions exited non-zero: stderr={stderr}"
    );
    assert!(
        stdout.contains("Register-ArgumentCompleter") || stdout.contains("sqlite-graphrag"),
        "powershell completions output missing completion markers"
    );
}

#[test]
fn completions_elvish_emits_script() {
    if !binary_exists() {
        eprintln!("skipping: debug binary not found");
        return;
    }
    let (code, stdout, stderr) = run_completions("elvish");
    assert_eq!(
        code, 0,
        "elvish completions exited non-zero: stderr={stderr}"
    );
    assert!(
        stdout.contains("edit:completion:arg-completer") || stdout.contains("sqlite-graphrag"),
        "elvish completions output missing completion markers"
    );
}

#[test]
fn completions_invalid_shell_exits_nonzero() {
    if !binary_exists() {
        eprintln!("skipping: debug binary not found");
        return;
    }
    let (code, _stdout, _stderr) = run_completions("not-a-real-shell");
    // clap ValueEnum rejects unknown shells with exit code 2 (arg error).
    assert_ne!(code, 0, "invalid shell should fail");
}

#[test]
fn completions_emits_nonempty_output_for_each_shell() {
    if !binary_exists() {
        eprintln!("skipping: debug binary not found");
        return;
    }
    for shell in &["bash", "zsh", "fish", "powershell", "elvish"] {
        let (code, stdout, _) = run_completions(shell);
        assert_eq!(code, 0, "{shell} completions exited non-zero");
        assert!(
            stdout.len() > 50,
            "{shell} completions output suspiciously short ({} bytes)",
            stdout.len()
        );
        // Sanity write to a tempfile so the test is not optimised away.
        let mut tmp = tempfile::NamedTempFile::new().expect("create tempfile");
        tmp.write_all(stdout.as_bytes()).expect("write to tempfile");
    }
}
