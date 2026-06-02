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
    "ANTHROPIC_API_KEY",
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
const DEFAULT_SUBPROCESS_MEMORY_LIMIT_MB: u64 = 4096;

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
pub fn build_claude_command(
    binary: &Path,
    prompt: &str,
    json_schema: &str,
    model: Option<&str>,
    max_turns: u32,
) -> Command {
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

    cmd.arg("-p")
        .arg(prompt)
        .arg("--output-format")
        .arg("json")
        .arg("--json-schema")
        .arg(json_schema)
        .arg("--max-turns")
        .arg(max_turns.to_string())
        .arg("--no-session-persistence");

    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        cmd.arg("--bare");
    } else {
        cmd.arg("--dangerously-skip-permissions")
            .arg("--settings")
            .arg(r#"{"hooks":{}}"#);
    }

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
}
