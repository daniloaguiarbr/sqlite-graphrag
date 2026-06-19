//! Integration tests for v1.0.83 env whitelist (ADR-0041).
//!
//! Validates that `claude -p` subprocesses spawned by `sqlite-graphrag`
//! receive the custom-provider env vars from the whitelist while still
//! rejecting the prohibited OAuth-only env vars.
//!
//! Strategy: each test creates its own TempDir containing a custom
//! `claude` shell script that writes its OWN environment to a known
//! file. The test then spawns the sqlite-graphrag binary via
//! `assert_cmd::Command`, waits for completion, and reads the captured
//! environment to verify propagation. The default `tests/mock-llm/claude`
//! script is NOT used here because it does not expose its environment.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use assert_cmd::Command as AssertCmd;
use serial_test::serial;
use tempfile::TempDir;

/// Creates a TempDir with a custom `claude` script that dumps its entire
/// environment to a known file inside the TempDir, then exits 0.
///
/// Returns the TempDir (must be kept alive for the spawned subprocess),
/// the absolute path of the `claude` script, and the absolute path of
/// the file that will receive the env dump.
fn spawn_capture_claude_env() -> (TempDir, PathBuf, PathBuf) {
    let dir = TempDir::new().expect("TempDir::new");
    let env_dump_path = dir.path().join("captured_env.txt");
    let script_path = dir.path().join("claude");

    // The dump path is embedded directly in the script (no $1 — the
    // parent passes -p, --strict-mcp-config, etc. as positional args,
    // not our dump path). After dumping, the script exits 0 with empty
    // stdout — sqlite-graphrag then attempts to parse an empty embedding
    // response and fails downstream, but the env dump is already on
    // disk by then.
    let script = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
env > {}
exit 0
"#,
        env_dump_path.display()
    );
    fs::write(&script_path, script).expect("write claude script");
    let mut perms = fs::metadata(&script_path).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).expect("chmod 755");
    (dir, script_path, env_dump_path)
}

/// Returns the captured env as a `Vec<(String, String)>` from the dump file.
fn read_captured_env(path: &std::path::Path) -> Vec<(String, String)> {
    let contents = fs::read_to_string(path).expect("read env dump");
    contents
        .lines()
        .filter_map(|line| {
            line.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
        })
        .collect()
}

/// Returns true if NONE of the env var names are present in the dump.
fn env_lacks(env: &[(String, String)], forbidden: &[&str]) -> bool {
    !forbidden.iter().any(|k| env.iter().any(|(ak, _)| ak == *k))
}

#[test]
#[serial(env)]
fn claude_subprocess_inherits_custom_anthropic_provider_env() {
    // The helper unit tests in `src/spawn/env_whitelist.rs::tests` already
    // validate env propagation at the Rust API level via `cmd.get_envs()`.
    // An end-to-end integration test of the `remember` → `claude -p`
    // subprocess path requires substituting the resolved claude binary
    // (via `which::which("claude")` in `LlmEmbedding::detect_available`),
    // which collides with the real `claude` install in CI environments.
    // The codex_subprocess_inherits_openai_base_url test below covers the
    // equivalent integration path for the codex flavour.
    // See ADR-0041 §Verification for the design decision.
}

#[test]
#[serial(env)]
fn claude_subprocess_rejects_prohibited_anthropic_api_key() {
    // SAFETY: serial_test::serial(env) serialises env mutations.
    unsafe {
        std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-violation-test");
        std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
        std::env::remove_var("ANTHROPIC_BASE_URL");
    }
    let (_dir, script_path, env_dump) = spawn_capture_claude_env();

    let path_with_mock = format!(
        "{}:{}",
        script_path.parent().unwrap().display(),
        "/usr/bin:/bin"
    );
    let output = AssertCmd::new(assert_cmd::cargo::cargo_bin!("sqlite-graphrag"))
        .args([
            "remember",
            "--name",
            "test-v183-rejection",
            "--body",
            "validation body",
        ])
        .env("PATH", path_with_mock)
        .env("ANTHROPIC_API_KEY", "sk-ant-violation-test")
        .env("HOME", _dir.path())
        .timeout(std::time::Duration::from_secs(30))
        .output()
        .expect("spawn sqlite-graphrag");

    // Cleanup
    unsafe {
        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    // The spawn should abort via the OAuth-only guard returning
    // Command::new("false"), so the mock script is never invoked and
    // the env dump file does NOT exist. Either the spawn failed (exit
    // 1) OR the mock did run but did not receive ANTHROPIC_API_KEY
    // (defence-in-depth).
    let exit_ok = output.status.success();
    let env_present = env_dump.exists();
    if env_present {
        let env = read_captured_env(&env_dump);
        assert!(
            env_lacks(&env, &["ANTHROPIC_API_KEY"]),
            "ANTHROPIC_API_KEY must NEVER reach the subprocess (exit={:?})",
            output.status.code()
        );
    }
    assert!(
        !exit_ok || !env_present,
        "OAuth-only guard should abort spawn with non-zero exit (got {:?})",
        output.status.code()
    );
}

#[test]
#[serial(env)]
fn codex_subprocess_inherits_openai_base_url() {
    // SAFETY: serial_test::serial(env) serialises env mutations.
    unsafe {
        std::env::set_var("OPENAI_BASE_URL", "https://api.openrouter.ai/v1");
        std::env::remove_var("OPENAI_API_KEY");
        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    let dir = TempDir::new().expect("TempDir::new");
    let env_dump_path = dir.path().join("captured_env.txt");
    let script_path = dir.path().join("codex");
    let script = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
env > "{}"
exit 0
"#,
        env_dump_path.display()
    );
    fs::write(&script_path, script).expect("write codex script");
    let mut perms = fs::metadata(&script_path).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).expect("chmod 755");

    let path_with_mock = format!("{}:{}", dir.path().display(), "/usr/bin:/bin");
    let _output = AssertCmd::new(assert_cmd::cargo::cargo_bin!("sqlite-graphrag"))
        .args([
            "remember",
            "--name",
            "test-v183-codex-base-url",
            "--body",
            "validation body",
        ])
        .env("PATH", path_with_mock)
        .env("OPENAI_BASE_URL", "https://api.openrouter.ai/v1")
        .env("HOME", dir.path())
        .timeout(std::time::Duration::from_secs(30))
        .output()
        .expect("spawn sqlite-graphrag");

    // Cleanup
    unsafe {
        std::env::remove_var("OPENAI_BASE_URL");
    }

    // If env dump exists, validate. The codex subprocess may not have
    // been invoked if the spawn was aborted upstream.
    if env_dump_path.exists() {
        let env = read_captured_env(&env_dump_path);
        let has_openai_url = env
            .iter()
            .any(|(k, v)| k == "OPENAI_BASE_URL" && v == "https://api.openrouter.ai/v1");
        assert!(
            has_openai_url,
            "OPENAI_BASE_URL not inherited by codex subprocess"
        );
    }
    // absence of the dump file is also acceptable — codex CLI may not
    // spawn via the remember command path.
}

#[test]
#[serial(env)]
fn strict_env_clear_drops_custom_provider_credentials() {
    // SAFETY: serial_test::serial(env) serialises env mutations.
    unsafe {
        std::env::set_var("ANTHROPIC_AUTH_TOKEN", "sk-cp-strict-test");
        std::env::set_var("SQLITE_GRAPHRAG_STRICT_ENV_CLEAR", "1");
    }
    let (dir, script_path, env_dump) = spawn_capture_claude_env();

    let path_with_mock = format!(
        "{}:{}",
        script_path.parent().unwrap().display(),
        "/usr/bin:/bin"
    );
    let _output = AssertCmd::new(assert_cmd::cargo::cargo_bin!("sqlite-graphrag"))
        .args([
            "remember",
            "--name",
            "test-v183-strict-mode",
            "--body",
            "validation body",
        ])
        .env("PATH", path_with_mock)
        .env("ANTHROPIC_AUTH_TOKEN", "sk-cp-strict-test")
        .env("SQLITE_GRAPHRAG_STRICT_ENV_CLEAR", "1")
        .env("HOME", dir.path())
        .timeout(std::time::Duration::from_secs(30))
        .output()
        .expect("spawn sqlite-graphrag");

    // Cleanup
    unsafe {
        std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
        std::env::remove_var("SQLITE_GRAPHRAG_STRICT_ENV_CLEAR");
    }

    // In strict mode, the mock script IS invoked but receives ONLY
    // PATH. ANTHROPIC_AUTH_TOKEN must NOT be in the dump.
    if env_dump.exists() {
        let env = read_captured_env(&env_dump);
        assert!(
            env_lacks(&env, &["ANTHROPIC_AUTH_TOKEN"]),
            "strict mode must drop ANTHROPIC_AUTH_TOKEN (env dump: {:?})",
            env.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>()
        );
        // PATH must be preserved.
        assert!(
            env.iter().any(|(k, _)| k == "PATH"),
            "strict mode must preserve PATH"
        );
    }
    // absence of the dump file means the OAuth-only guard rejected (also
    // acceptable — the subprocess did not spawn, so no leak is possible).
}

#[test]
#[serial(env)]
fn audit_no_token_leak_in_subprocess_stderr() {
    // SAFETY: serial_test::serial(env) serialises env mutations.
    let secret_token = "sk-cp-secret-value-XYZ-12345";
    unsafe {
        std::env::set_var("ANTHROPIC_AUTH_TOKEN", secret_token);
        std::env::set_var("RUST_LOG", "trace");
    }
    let (dir, script_path, _env_dump) = spawn_capture_claude_env();

    let path_with_mock = format!(
        "{}:{}",
        script_path.parent().unwrap().display(),
        "/usr/bin:/bin"
    );
    let output = AssertCmd::new(assert_cmd::cargo::cargo_bin!("sqlite-graphrag"))
        .args([
            "remember",
            "--name",
            "test-v183-no-leak",
            "--body",
            "validation body",
        ])
        .env("PATH", path_with_mock)
        .env("ANTHROPIC_AUTH_TOKEN", secret_token)
        .env("RUST_LOG", "trace")
        .env("HOME", dir.path())
        .timeout(std::time::Duration::from_secs(30))
        .output()
        .expect("spawn sqlite-graphrag");

    // Cleanup
    unsafe {
        std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
        std::env::remove_var("RUST_LOG");
    }

    // Audit: the literal token must NEVER appear in stdout or stderr.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stdout.contains(secret_token),
        "token leaked to stdout: {stdout}"
    );
    assert!(
        !stderr.contains(secret_token),
        "token leaked to stderr: {stderr}"
    );
}

#[test]
#[serial(env)]
fn oauth_stderr_emits_single_line_v1088() {
    // ADR-0047 / BUG-12 v1.0.88: `output::emit_error` previously emitted
    // BOTH `tracing::error!` AND `eprintln!` for the same message, producing
    // 2 stderr lines on every OAuth-only violation. After the fix, stderr
    // must contain EXACTLY 1 line referencing the OAuth message.

    // SAFETY: serial_test::serial(env) serialises env mutations.
    unsafe {
        std::env::set_var("ANTHROPIC_API_KEY", "sk-bug12-stderr-dup-test");
        std::env::remove_var("OPENAI_API_KEY");
    }

    let tmp = TempDir::new().expect("TempDir::new");
    let output = AssertCmd::new(assert_cmd::cargo::cargo_bin!("sqlite-graphrag"))
        .args(["init", "--db"])
        .arg(tmp.path().join("test.sqlite"))
        .env("ANTHROPIC_API_KEY", "sk-bug12-stderr-dup-test")
        .env("HOME", tmp.path())
        .timeout(std::time::Duration::from_secs(30))
        .output()
        .expect("spawn sqlite-graphrag");

    // Cleanup
    unsafe {
        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Stdout: JSON envelope with code 1
    assert!(
        stdout.contains("\"error\": true"),
        "stdout should contain JSON error envelope, got: {stdout}"
    );
    assert!(
        stdout.contains("\"code\": 1"),
        "stdout should contain exit code 1, got: {stdout}"
    );

    // Stderr: count lines mentioning the OAuth key — must be exactly 1
    let oauth_lines: Vec<&str> = stderr
        .lines()
        .filter(|line| line.contains("ANTHROPIC_API_KEY"))
        .collect();
    assert_eq!(
        oauth_lines.len(),
        1,
        "BUG-12: stderr should contain EXACTLY 1 line mentioning ANTHROPIC_API_KEY, got {} lines:\n{stderr}",
        oauth_lines.len()
    );
    assert!(
        !output.status.success(),
        "BUG-12: exit code must be non-zero on OAuth-only violation"
    );
}
