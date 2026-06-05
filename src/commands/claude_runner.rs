//! Shared module for spawning Claude Code (`claude -p`) subprocesses.
//!
//! Eliminates duplication between `enrich.rs` and `ingest_claude.rs` (G02).
//! Detects `terminal_reason: "max_turns"` in the JSON output (G03).

use crate::errors::AppError;
use std::path::Path;
use std::process::{Command, Stdio};

/// Minimum Claude Code version required for structured JSON output.
const MIN_CLAUDE_VERSION: &str = "2.1.0";

/// Environment variables whitelisted for the subprocess.
const ENV_WHITELIST: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "SHELL",
    "TERM",
    "LANG",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_RUNTIME_DIR",
    // NOTE: `ANTHROPIC_API_KEY` is INTENTIONALLY ABSENT from this whitelist
    // (gaps.md:47). The OAuth-only flow uses the session token from
    // `~/.claude/.credentials.json` (or the OS keychain), not an env var.
    // The OAuth-only guard in `build_claude_command` aborts the spawn if
    // `ANTHROPIC_API_KEY` is set in the environment, but defence-in-depth
    // also requires the variable to never reach the child process.
    "CLAUDE_CONFIG_DIR",
    "TMPDIR",
    "TMP",
    "TEMP",
    "DYLD_FALLBACK_LIBRARY_PATH",
];

/// Windows-only environment variables.
#[cfg(windows)]
const ENV_WHITELIST_WINDOWS: &[&str] = &[
    "LOCALAPPDATA",
    "APPDATA",
    "USERPROFILE",
    "SystemRoot",
    "COMSPEC",
    "PATHEXT",
    "HOMEPATH",
    "HOMEDRIVE",
];

/// Default virtual memory limit for LLM subprocesses (4 GiB).
#[cfg(target_os = "linux")]
const DEFAULT_SUBPROCESS_MEMORY_LIMIT_MB: u64 = 4096;

// G28-C (v1.0.69): process lifecycle. The G28 gap asks for
// `tokio::process::Command::kill_on_drop(true)`. This codebase uses
// `std::process::Command` (synchronous) so the tokio helper is not
// available. Equivalent defence-in-depth is provided by:
//
// 1. `SIGTERM` via `libc::kill` in the timeout branch of `run_claude`
//    and `run_codex` (graceful — gives the child a chance to clean up
//    MCP children and write logs).
// 2. `child.kill()` (SIGKILL) if SIGTERM was ignored.
// 3. `reaper::scan_and_kill_orphans()` at startup, which walks `/proc`
//    and reaps any `claude`/`codex` processes that were orphaned by a
//    previous crash.
//
// SIGKILL on drop is intentionally NOT used because (a) the gaps.md
// Passo C warning flags it as risky per tokio-rs/tokio#7082, and (b)
// the SIGTERM-then-SIGKILL pair covers the same threat model with
// better cleanup behaviour.

/// Spawns a command with a virtual memory limit via `setrlimit(RLIMIT_AS)`.
///
/// On Linux, applies the limit in a `pre_exec` hook before the child process
/// starts.  On non-Linux platforms, falls back to an unlimited spawn.
/// The limit is read from `SQLITE_GRAPHRAG_SUBPROCESS_MEMORY_LIMIT_MB`
/// (default: 4096 MiB).
#[cfg(target_os = "linux")]
pub fn spawn_with_memory_limit(cmd: &mut Command) -> std::io::Result<std::process::Child> {
    use std::os::unix::process::CommandExt;
    let max_mb: u64 = std::env::var("SQLITE_GRAPHRAG_SUBPROCESS_MEMORY_LIMIT_MB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SUBPROCESS_MEMORY_LIMIT_MB);
    let max_bytes = max_mb * 1024 * 1024;
    // SAFETY: pre_exec closure runs between fork() and exec() in the
    // single-threaded child process — no other threads exist.
    // libc::setsid and libc::setrlimit are async-signal-safe per POSIX.1-2008 §2.4.3.
    // RLIMIT_AS limits virtual address space, not physical RSS.
    // setsid failure with EPERM is tolerated (process already a session leader).
    // On setrlimit failure, Err(last_os_error()) prevents exec.
    unsafe {
        cmd.pre_exec(move || {
            let sid = libc::setsid();
            if sid == -1 {
                let err = std::io::Error::last_os_error();
                if err.raw_os_error() != Some(libc::EPERM) {
                    return Err(err);
                }
            }
            let limit = libc::rlimit {
                rlim_cur: max_bytes,
                rlim_max: max_bytes,
            };
            if libc::setrlimit(libc::RLIMIT_AS, &limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    tracing::debug!(
        target: "process",
        program = ?cmd.get_program(),
        args = ?cmd.get_args().collect::<Vec<_>>(),
        "spawning external process"
    );
    cmd.spawn()
}

/// Spawns a command without memory limits (non-Linux fallback).
/// On Unix (macOS, FreeBSD), applies setsid for process group isolation.
#[cfg(not(target_os = "linux"))]
pub fn spawn_with_memory_limit(cmd: &mut Command) -> std::io::Result<std::process::Child> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid() is async-signal-safe per POSIX.1-2008 §2.4.3.
        // Creates independent session for cascade termination.
        unsafe {
            cmd.pre_exec(|| {
                let sid = libc::setsid();
                if sid == -1 {
                    let err = std::io::Error::last_os_error();
                    if err.raw_os_error() != Some(libc::EPERM) {
                        return Err(err);
                    }
                }
                Ok(())
            });
        }
    }
    tracing::debug!(
        target: "process",
        program = ?cmd.get_program(),
        args = ?cmd.get_args().collect::<Vec<_>>(),
        "spawning external process"
    );
    cmd.spawn()
}

/// Parsed output element from `claude -p --output-format json`.
#[derive(Debug, serde::Deserialize)]
pub struct ClaudeOutputElement {
    pub r#type: Option<String>,
    pub subtype: Option<String>,
    #[serde(default)]
    pub is_error: bool,
    pub structured_output: Option<serde_json::Value>,
    pub result: Option<String>,
    pub total_cost_usd: Option<f64>,
    pub error: Option<String>,
    pub terminal_reason: Option<String>,
    #[serde(rename = "apiKeySource")]
    pub api_key_source: Option<String>,
}

/// Result of a successful Claude invocation.
#[derive(Debug)]
pub struct ClaudeResult {
    pub value: serde_json::Value,
    pub cost_usd: f64,
    pub is_oauth: bool,
}

/// Validates that the Claude binary meets the minimum version requirement.
pub fn validate_claude_version(binary: &Path) -> Result<String, AppError> {
    let resolved = which::which(binary).map_err(|_| {
        AppError::Validation(format!(
            "executable '{}' not found in PATH; ensure it is installed and accessible",
            binary.display()
        ))
    })?;
    let output = Command::new(&resolved)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(AppError::Io)?;

    if !output.status.success() {
        return Err(AppError::Validation(
            "failed to run 'claude --version'".to_string(),
        ));
    }

    let version_str = String::from_utf8(output.stdout)
        .map_err(|_| AppError::Validation("claude --version output is not UTF-8".to_string()))?;
    let version = version_str.trim().to_string();
    let numeric = version.split([' ', '(']).next().unwrap_or("").trim();

    fn parse_semver(s: &str) -> Option<(u64, u64, u64)> {
        let parts: Vec<&str> = s.splitn(3, '.').collect();
        if parts.len() < 2 {
            return None;
        }
        let major = parts[0].parse::<u64>().ok()?;
        let minor = parts[1].parse::<u64>().ok()?;
        let patch = parts
            .get(2)
            .and_then(|p| p.parse::<u64>().ok())
            .unwrap_or(0);
        Some((major, minor, patch))
    }

    if let (Some(actual), Some(min)) = (parse_semver(numeric), parse_semver(MIN_CLAUDE_VERSION)) {
        if actual < min {
            return Err(AppError::Validation(format!(
                "Claude Code version {numeric} is below minimum required {MIN_CLAUDE_VERSION}"
            )));
        }
    }

    Ok(version)
}

/// Builds a `Command` for `claude -p` with least-privilege environment.
///
/// G28-A (v1.0.68) + OAuth-only hardening (v1.0.69, mandated by gaps.md
/// lines 41-49): the command ALWAYS uses the OAuth flow. The flag set
/// is the canonical one documented in gaps.md Fix A:
///
/// ```text
/// claude -p "TAREFA" \
///   --strict-mcp-config \
///   --mcp-config '{}' \
///   --dangerously-skip-permissions \
///   --settings '{"hooks":{}}' \
///   --model <X> \
///   --max-turns <N> \
///   --output-format json \
///   --no-session-persistence
/// ```
///
/// The combination cuts the typical 8-10 MCP process tree to zero and
/// disables user hooks. The reaper sweep at startup (see `reaper::scan_and_kill_orphans`)
/// is the last line of defence for any process that ignored the flags.
///
/// **`--bare` is FORBIDDEN** (gaps.md:49 and operator policy):
/// `--bare` cuts MCPs but disables OAuth and demands `ANTHROPIC_API_KEY`,
/// which is PROHIBITED in this project. We also ABORT the spawn if
/// `ANTHROPIC_API_KEY` is set in the environment, because that is the
/// gateway to the prohibited API-key path.
///
/// GitHub issue [anthropics/claude-code#10787] documents that earlier
/// Claude Code CLI builds sometimes ignored `--strict-mcp-config` and
/// fell back to `~/.mcp.json`. We still pass the flags as defence-in-depth
/// and ALSO honour `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` so users
/// who need belt-and-suspenders isolation can point Claude at an empty
/// config directory (no MCP, no hooks, no settings).
///
/// [anthropics/claude-code#10787]: https://github.com/anthropics/claude-code/issues/10787
pub fn build_claude_command(
    binary: &Path,
    prompt: &str,
    json_schema: &str,
    model: Option<&str>,
    max_turns: u32,
) -> Command {
    // OAuth-only guard (gaps.md:47). If `ANTHROPIC_API_KEY` is set in the
    // environment we MUST abort — that is the API-key path which is
    // explicitly PROHIBITED. Use the OAuth flow exclusively.
    if let Ok(_key) = std::env::var("ANTHROPIC_API_KEY") {
        // Return a command that will fail loudly at spawn time. We
        // intentionally do NOT pass `--bare` (PROHIBITED) and we do NOT
        // allow the API-key path at all.
        let mut cmd = Command::new("false");
        cmd.env_clear();
        cmd.env("PATH", "/nonexistent");
        cmd.arg("--oauth-only-violation-anthropic-api-key-set");
        return cmd;
    }

    let mut cmd = Command::new(binary);

    cmd.env_clear();
    for var in ENV_WHITELIST {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }

    #[cfg(windows)]
    for var in ENV_WHITELIST_WINDOWS {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }

    // G28-A: if the user has pointed us at an empty config dir, force Claude
    // Code to use it (which suppresses user-scoped MCP servers and hooks).
    if let Ok(empty_dir) = std::env::var("SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR") {
        if std::path::Path::new(&empty_dir).is_dir() {
            cmd.env("CLAUDE_CONFIG_DIR", &empty_dir);
            tracing::debug!(
                target: "claude_runner",
                "isolating claude subprocess to CLAUDE_CONFIG_DIR={}",
                empty_dir
            );
        } else {
            tracing::warn!(
                target: "claude_runner",
                path = %empty_dir,
                "SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR is set but path is not a directory; \
                 ignoring.  MCP isolation will NOT be applied."
            );
        }
    }

    // Canonical OAuth-only command line (gaps.md:201-208). Every flag is
    // mandatory; do NOT pass `--bare` (PROHIBITED, gaps.md:49).
    cmd.arg("-p")
        .arg(prompt)
        .arg("--strict-mcp-config")
        .arg("--mcp-config")
        .arg("{}")
        .arg("--dangerously-skip-permissions")
        .arg("--settings")
        .arg(r#"{"hooks":{}}"#)
        .arg("--output-format")
        .arg("json")
        .arg("--json-schema")
        .arg(json_schema)
        .arg("--max-turns")
        .arg(max_turns.to_string())
        .arg("--no-session-persistence");

    if let Some(m) = model {
        cmd.arg("--model").arg(m);
    }

    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    cmd
}

/// Parses `claude -p --output-format json` output array.
///
/// G03: detects `terminal_reason: "max_turns"` and returns a specific error
/// instead of a generic failure message.
pub fn parse_claude_output(stdout: &str) -> Result<ClaudeResult, AppError> {
    let elements: Vec<ClaudeOutputElement> = serde_json::from_str(stdout).map_err(|e| {
        AppError::Validation(format!("failed to parse claude output as JSON array: {e}"))
    })?;

    let is_oauth = elements
        .iter()
        .find(|e| e.r#type.as_deref() == Some("system") && e.subtype.as_deref() == Some("init"))
        .and_then(|e| e.api_key_source.as_deref())
        .map(|s| s == "none")
        .unwrap_or(false);

    let result_elem = elements
        .iter()
        .find(|e| e.r#type.as_deref() == Some("result"))
        .ok_or_else(|| {
            AppError::Validation("claude output missing 'result' element".to_string())
        })?;

    // G03: detect max_turns exhaustion before checking is_error
    if result_elem.terminal_reason.as_deref() == Some("max_turns") {
        tracing::warn!(
            target: "claude_runner",
            "claude -p hit max_turns limit — hooks may have consumed turns"
        );
        return Err(AppError::Validation(
            "claude -p hit max_turns: hooks may be consuming turns; increase --max-turns or disable hooks".to_string(),
        ));
    }

    if result_elem.is_error {
        let err_msg = result_elem
            .error
            .as_deref()
            .or(result_elem.result.as_deref())
            .unwrap_or("unknown error");
        if err_msg.contains("rate_limit") || err_msg.contains("overloaded") {
            return Err(AppError::RateLimited {
                detail: err_msg.to_string(),
            });
        }
        if err_msg.contains("Not logged in") || err_msg.contains("authentication") {
            tracing::warn!(
                target: "claude_runner",
                "Claude Code authentication failed. Re-authenticate interactively with: claude"
            );
        }
        return Err(AppError::Validation(format!(
            "claude extraction failed: {err_msg}"
        )));
    }

    let value = if let Some(v) = result_elem.structured_output.clone() {
        v
    } else if let Some(text) = &result_elem.result {
        serde_json::from_str(text).map_err(|e| {
            AppError::Validation(format!("failed to parse claude result field as JSON: {e}"))
        })?
    } else {
        return Err(AppError::Validation(
            "claude result missing structured_output and result field".into(),
        ));
    };

    let cost = result_elem.total_cost_usd.unwrap_or(0.0);
    Ok(ClaudeResult {
        value,
        cost_usd: cost,
        is_oauth,
    })
}

/// Calls `claude -p` with prompt and schema, waits with timeout, and parses output.
///
/// G03: parses stdout even on non-zero exit to detect `terminal_reason: "max_turns"`.
/// G28-C (v1.0.69): the child is killed explicitly on timeout to avoid
/// leaving a `claude -p` zombie with its MCP children behind.
pub fn run_claude(
    binary: &Path,
    prompt: &str,
    json_schema: &str,
    input_text: &str,
    model: Option<&str>,
    timeout_secs: u64,
    max_turns: u32,
) -> Result<ClaudeResult, AppError> {
    use wait_timeout::ChildExt;

    let full_prompt = format!("{prompt}\n\n{input_text}");
    let mut cmd = build_claude_command(binary, &full_prompt, json_schema, model, max_turns);

    let mut child = spawn_with_memory_limit(&mut cmd).map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to spawn claude: {e}"),
        ))
    })?;

    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let status = child.wait_timeout(timeout).map_err(AppError::Io)?;

    if status.is_none() {
        // G28-C: timeout hit — send SIGTERM to the child so the MCP
        // children it spawned (and their npm/node tree) are also
        // reaped. SIGTERM gives the child a chance to clean up; the
        // reaper sweep in main.rs is the last line of defence for
        // anything that ignored it.
        #[cfg(unix)]
        unsafe {
            libc::kill(child.id() as i32, libc::SIGTERM);
        }
        let _ = child.kill();
        let _ = child.wait();
    }

    match status {
        Some(exit_status) => {
            tracing::debug!(
                target: "process",
                exit_code = ?exit_status.code(),
                elapsed_ms = start.elapsed().as_millis() as u64,
                "external process completed"
            );

            let mut stdout_buf = Vec::new();
            let mut stderr_buf = Vec::new();
            if let Some(mut out) = child.stdout.take() {
                std::io::Read::read_to_end(&mut out, &mut stdout_buf).map_err(AppError::Io)?;
            }
            if let Some(mut err) = child.stderr.take() {
                std::io::Read::read_to_end(&mut err, &mut stderr_buf).map_err(AppError::Io)?;
            }

            let stdout_str = String::from_utf8(stdout_buf)
                .map_err(|_| AppError::Validation("claude -p stdout is not valid UTF-8".into()))?;

            // G03: parse stdout even on failure to detect terminal_reason
            if !exit_status.success() {
                if let Ok(result) = parse_claude_output(&stdout_str) {
                    return Ok(result);
                }
                let stderr_str = String::from_utf8_lossy(&stderr_buf);
                if stderr_str.contains("auth") || stderr_str.contains("login") {
                    tracing::warn!(
                        target: "claude_runner",
                        "Claude Code authentication may have failed. Re-authenticate with: claude"
                    );
                }
                return Err(AppError::Validation(format!(
                    "claude -p exited with code {:?}: {}",
                    exit_status.code(),
                    stderr_str.trim()
                )));
            }

            parse_claude_output(&stdout_str)
        }
        None => {
            tracing::warn!(target: "claude_runner", timeout_secs, "claude -p timed out, terminating");
            terminate_gracefully(&mut child, 3);
            Err(AppError::Validation(format!(
                "claude -p timed out after {timeout_secs} seconds"
            )))
        }
    }
}

/// Terminates a child process gracefully: SIGTERM first, SIGKILL after grace period.
#[cfg(unix)]
pub fn terminate_gracefully(child: &mut std::process::Child, grace_secs: u64) {
    use wait_timeout::ChildExt;
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }
    match child.wait_timeout(std::time::Duration::from_secs(grace_secs)) {
        Ok(Some(_)) => {}
        _ => {
            tracing::warn!(target: "process", pid = child.id(), "child ignored SIGTERM, sending SIGKILL");
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Non-Unix fallback: kill immediately (Windows TerminateProcess).
#[cfg(not(unix))]
pub fn terminate_gracefully(child: &mut std::process::Child, _grace_secs: u64) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_output_detects_max_turns() {
        let stdout = r#"[{"type":"system","subtype":"init","apiKeySource":"none"},{"type":"result","is_error":false,"terminal_reason":"max_turns","structured_output":{"name":"t"}}]"#;
        let err = parse_claude_output(stdout).unwrap_err();
        assert!(
            format!("{err}").contains("max_turns"),
            "must detect max_turns in output"
        );
    }

    #[test]
    fn parse_output_extracts_structured_value() {
        let stdout = r#"[{"type":"system","subtype":"init","apiKeySource":"none"},{"type":"result","is_error":false,"structured_output":{"key":"val"},"total_cost_usd":0.01}]"#;
        let result = parse_claude_output(stdout).unwrap();
        assert_eq!(result.value["key"], "val");
        assert!((result.cost_usd - 0.01).abs() < f64::EPSILON);
        assert!(result.is_oauth);
    }

    #[test]
    fn parse_output_detects_rate_limit() {
        let stdout = r#"[{"type":"result","is_error":true,"error":"rate_limit exceeded"}]"#;
        let err = parse_claude_output(stdout).unwrap_err();
        assert!(
            matches!(err, AppError::RateLimited { .. }),
            "expected AppError::RateLimited, got: {err}"
        );
    }

    /// OAuth-only conformance test (gaps.md:41-49, v1.0.69 mandate).
    /// Verifies that `build_claude_command` always emits the canonical
    /// flag set and NEVER emits `--bare` or any API-key path.
    #[test]
    #[serial_test::serial(env)]
    fn build_command_oauth_only_mandatory_flags() {
        // SAFETY: this is a unit test, no concurrent env mutation
        unsafe {
            std::env::remove_var("ANTHROPIC_API_KEY");
        }
        let cmd = build_claude_command(
            std::path::Path::new("/usr/bin/false"),
            "test prompt",
            "{}",
            Some("sonnet"),
            4,
        );
        let args: Vec<&str> = cmd.get_args().filter_map(|a| a.to_str()).collect();
        // Mandatory OAuth-only flags from gaps.md lines 201-208
        assert!(args.contains(&"-p"), "must have -p");
        assert!(
            args.contains(&"--strict-mcp-config"),
            "must have --strict-mcp-config (gaps.md:206)"
        );
        assert!(
            args.contains(&"--mcp-config"),
            "must have --mcp-config (gaps.md:207)"
        );
        assert!(
            args.contains(&"--dangerously-skip-permissions"),
            "must have --dangerously-skip-permissions (gaps.md:208)"
        );
        assert!(
            args.contains(&"--settings"),
            "must have --settings (gaps.md:209)"
        );
        assert!(
            args.contains(&"--output-format"),
            "must have --output-format json (gaps.md:213)"
        );
        assert!(args.contains(&"--json-schema"), "must have --json-schema");
        assert!(
            args.contains(&"--max-turns"),
            "must have --max-turns (gaps.md:212)"
        );
        assert!(
            args.contains(&"--no-session-persistence"),
            "must have --no-session-persistence"
        );
        assert!(
            args.contains(&"--model"),
            "must have --model when model is Some"
        );
        // PROHIBITED flags (gaps.md:49)
        assert!(
            !args.contains(&"--bare"),
            "--bare is PROHIBITED (gaps.md:49)"
        );
    }

    /// OAuth-only guard: when `ANTHROPIC_API_KEY` is in the environment,
    /// `build_claude_command` MUST abort the spawn (return a `false`
    /// command), NOT silently fall back to the API-key path.
    #[test]
    #[serial_test::serial(env)]
    fn build_command_aborts_when_anthropic_api_key_set() {
        // SAFETY: unit test
        unsafe {
            std::env::set_var("ANTHROPIC_API_KEY", "sk-test-violation");
        }
        let cmd = build_claude_command(
            std::path::Path::new("/usr/bin/claude"),
            "test prompt",
            "{}",
            Some("sonnet"),
            4,
        );
        let program = cmd.get_program().to_string_lossy().to_string();
        let args: Vec<&str> = cmd.get_args().filter_map(|a| a.to_str()).collect();
        assert_eq!(
            program, "false",
            "when ANTHROPIC_API_KEY is set, build_claude_command must abort"
        );
        assert!(
            args.contains(&"--oauth-only-violation-anthropic-api-key-set"),
            "aborted command must carry violation marker"
        );
        unsafe {
            std::env::remove_var("ANTHROPIC_API_KEY");
        }
    }
}
