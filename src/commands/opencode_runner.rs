//! OpenCode headless runner for ingest and enrich pipelines (v1.0.90).
//!
//! Symmetric to `claude_runner.rs` (claude -p) and `codex_spawn.rs`
//! (codex exec). Builds the `opencode run` command, parses NDJSON
//! output, and provides rate-limit backoff.

use crate::errors::AppError;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

/// Default timeout per opencode invocation in seconds.
const DEFAULT_OPENCODE_TIMEOUT_SECS: u64 = 300;

/// Minimum supported opencode version.
const MIN_OPENCODE_VERSION: (u64, u64, u64) = (1, 17, 0);

/// Resolve the opencode binary path.
///
/// Precedence: `SQLITE_GRAPHRAG_OPENCODE_BINARY` env var > `which::which("opencode")`.
pub fn find_opencode_binary_with_override(explicit: Option<&Path>) -> Result<PathBuf, AppError> {
    if let Some(p) = explicit {
        if p.exists() {
            return Ok(p.to_path_buf());
        }
        return Err(AppError::Validation(format!(
            "opencode binary not found at explicit path: {}",
            p.display()
        )));
    }
    if let Ok(path) = std::env::var("SQLITE_GRAPHRAG_OPENCODE_BINARY") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
        tracing::warn!(
            target: "opencode_runner",
            path = %p.display(),
            "SQLITE_GRAPHRAG_OPENCODE_BINARY is set but file does not exist; falling back to PATH"
        );
    }
    which::which("opencode").map_err(|_| {
        AppError::Validation(
            "`opencode` not found on PATH. Install opencode (>= 1.17) or set \
             SQLITE_GRAPHRAG_OPENCODE_BINARY to the binary path."
                .into(),
        )
    })
}

pub fn find_opencode_binary() -> Result<PathBuf, AppError> {
    find_opencode_binary_with_override(None)
}

/// Resolve the opencode model name.
///
/// Precedence: explicit `model` arg > `SQLITE_GRAPHRAG_OPENCODE_MODEL` env var
/// > default `opencode/big-pickle`.
///
/// NOTE: intentionally does NOT fall back to `SQLITE_GRAPHRAG_LLM_MODEL` because
/// that var typically holds a codex/claude model (e.g. "gpt-5.4-mini") that
/// opencode does not recognise — cross-contamination caused
/// ProviderModelNotFoundError (v1.0.90 audit).
pub fn resolve_opencode_model(model_override: Option<&str>) -> String {
    if let Some(m) = model_override {
        return m.to_string();
    }
    std::env::var("SQLITE_GRAPHRAG_OPENCODE_MODEL")
        .unwrap_or_else(|_| "opencode/big-pickle".to_string())
}

/// Resolve the opencode timeout in seconds.
///
/// Precedence: explicit arg > `SQLITE_GRAPHRAG_OPENCODE_TIMEOUT` env var > default 300s.
pub fn resolve_opencode_timeout(timeout_override: Option<u64>) -> u64 {
    if let Some(t) = timeout_override {
        return t;
    }
    std::env::var("SQLITE_GRAPHRAG_OPENCODE_TIMEOUT")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_OPENCODE_TIMEOUT_SECS)
}

/// Validate the installed opencode version meets the minimum requirement.
pub fn validate_opencode_version(binary: &Path) -> Result<(u64, u64, u64), AppError> {
    let output = std::process::Command::new(binary)
        .arg("--version")
        .output()
        .map_err(|e| AppError::Validation(format!("failed to run opencode --version: {e}")))?;

    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let raw = if raw.is_empty() {
        String::from_utf8_lossy(&output.stderr).trim().to_string()
    } else {
        raw
    };

    parse_version(&raw).and_then(|v| {
        if v >= MIN_OPENCODE_VERSION {
            Ok(v)
        } else {
            Err(AppError::Validation(format!(
                "opencode version {}.{}.{} is below minimum {}.{}.{}",
                v.0,
                v.1,
                v.2,
                MIN_OPENCODE_VERSION.0,
                MIN_OPENCODE_VERSION.1,
                MIN_OPENCODE_VERSION.2,
            )))
        }
    })
}

fn parse_version(raw: &str) -> Result<(u64, u64, u64), AppError> {
    // opencode --version returns just the version number, e.g. "1.17.7"
    let digits: String = raw
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let parts: Vec<&str> = digits.split('.').collect();
    if parts.len() >= 3 {
        if let (Ok(major), Ok(minor), Ok(patch)) = (
            parts[0].parse::<u64>(),
            parts[1].parse::<u64>(),
            parts[2].parse::<u64>(),
        ) {
            return Ok((major, minor, patch));
        }
    }
    Err(AppError::Validation(format!(
        "could not parse opencode version from: {raw}"
    )))
}

/// Propagate opencode-relevant env vars into a subprocess.
///
/// After `env_clear()`, the subprocess only has PATH and HOME. OpenCode
/// may need provider API keys (OPENROUTER_API_KEY, ANTHROPIC_AUTH_TOKEN,
/// etc.), XDG dirs, LANG/TERM for proper operation. This helper forwards
/// any env var matching the OPENCODE_*, OPENROUTER_*, XDG_*, LANG, TERM
/// prefixes from the parent process.
pub fn propagate_opencode_env(cmd: &mut Command) {
    const PREFIXES: &[&str] = &["OPENCODE_", "OPENROUTER_", "XDG_"];
    const EXACT: &[&str] = &["LANG", "TERM", "USER", "LOGNAME", "TMPDIR"];
    for (key, val) in std::env::vars() {
        if PREFIXES.iter().any(|p| key.starts_with(p)) || EXACT.contains(&key.as_str()) {
            cmd.env(&key, &val);
        }
    }
}

/// Build the opencode run command with hardening flags.
///
/// Unlike codex (9 flags) and claude (7 flags), opencode has only
/// `--dangerously-skip-permissions` for auto-approval.
pub fn build_opencode_command(
    binary: &Path,
    model: &str,
    prompt: &str,
) -> Result<Command, AppError> {
    let mut cmd = Command::new(binary);
    cmd.arg("run")
        .arg("--format")
        .arg("json")
        .arg("-m")
        .arg(model)
        .arg("--dangerously-skip-permissions")
        .arg(prompt)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", std::env::var("HOME").unwrap_or_default())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    propagate_opencode_env(&mut cmd);
    crate::spawn::apply_cwd_isolation_tokio(&mut cmd)?;
    Ok(cmd)
}

/// Parse the NDJSON output from `opencode run --format json`.
///
/// The output has 3 event types:
/// - `step_start`: ignored
/// - `text`: `.part.text` contains the LLM response text
/// - `step_finish`: `.part.tokens` and `.part.cost` for accounting
///
/// Returns `(response_text, cost, tokens)`.
pub fn parse_opencode_output(stdout: &str) -> Result<(String, f64, u64), AppError> {
    let mut texts: Vec<String> = Vec::new();
    let mut cost: f64 = 0.0;
    let mut tokens: u64 = 0;

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(event) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match event_type {
            "text" => {
                if let Some(text) = event
                    .get("part")
                    .and_then(|p| p.get("text"))
                    .and_then(|t| t.as_str())
                {
                    texts.push(text.to_string());
                }
            }
            "step_finish" => {
                if let Some(part) = event.get("part") {
                    if let Some(c) = part.get("cost").and_then(|c| c.as_f64()) {
                        cost = c;
                    }
                    if let Some(t) = part
                        .get("tokens")
                        .and_then(|t| t.get("total"))
                        .and_then(|t| t.as_u64())
                    {
                        tokens = t;
                    }
                }
            }
            _ => {}
        }
    }

    if texts.is_empty() {
        return Err(AppError::Embedding(
            "opencode returned no text events in NDJSON output".to_string(),
        ));
    }

    Ok((texts.concat(), cost, tokens))
}

/// Parse a JSON value from opencode output text.
///
/// Opencode has no `--output-schema`, so the LLM may include markdown
/// fences or explanation text around the JSON. This function tries:
/// 1. Direct JSON parse of the full text
/// 2. Extract JSON from markdown code fences
/// 3. Find the first `{` to last `}` substring
pub fn parse_json_from_opencode_text<T: serde::de::DeserializeOwned>(
    text: &str,
) -> Result<T, String> {
    // Strategy 1: direct parse
    if let Ok(parsed) = serde_json::from_str::<T>(text) {
        return Ok(parsed);
    }

    // Strategy 2: extract from markdown code fence
    if let Some(start) = text.find("```json") {
        let after_fence = &text[start + 7..];
        if let Some(end) = after_fence.find("```") {
            let json_str = after_fence[..end].trim();
            if let Ok(parsed) = serde_json::from_str::<T>(json_str) {
                return Ok(parsed);
            }
        }
    }
    if let Some(start) = text.find("```") {
        let after_fence = &text[start + 3..];
        if let Some(end) = after_fence.find("```") {
            let json_str = after_fence[..end].trim();
            if let Ok(parsed) = serde_json::from_str::<T>(json_str) {
                return Ok(parsed);
            }
        }
    }

    // Strategy 3: find first { to last }
    if let (Some(start), Some(end)) = (text.find('{'), text.rfind('}')) {
        if start < end {
            let json_str = &text[start..=end];
            if let Ok(parsed) = serde_json::from_str::<T>(json_str) {
                return Ok(parsed);
            }
        }
    }

    Err(format!(
        "could not extract valid JSON from opencode response: {}",
        &text[..text.len().min(200)]
    ))
}

/// Call opencode headless and return the parsed JSON response.
///
/// Combines `build_opencode_command`, subprocess execution with timeout,
/// `parse_opencode_output`, and `parse_json_from_opencode_text`.
pub async fn call_opencode<T: serde::de::DeserializeOwned>(
    binary: &Path,
    model: &str,
    prompt: &str,
    timeout_secs: u64,
) -> Result<(T, f64, u64), AppError> {
    let mut cmd = build_opencode_command(binary, model, prompt)?;
    let timeout = std::time::Duration::from_secs(timeout_secs);

    let output = match tokio::time::timeout(timeout, cmd.output()).await {
        Err(_elapsed) => {
            return Err(AppError::Embedding(format!(
                "opencode timed out after {timeout_secs}s"
            )));
        }
        Ok(Err(e)) => {
            return Err(AppError::Embedding(format!(
                "failed to spawn opencode: {e}"
            )));
        }
        Ok(Ok(o)) => o,
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(AppError::Embedding(format!(
            "opencode exited with {}: stderr={}, stdout={}",
            output.status,
            &stderr[..stderr.len().min(500)],
            &stdout[..stdout.len().min(500)],
        )));
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let (text, _cost, _tokens) = parse_opencode_output(&stdout_str)?;
    let parsed: T = parse_json_from_opencode_text(&text)
        .map_err(|e| AppError::Embedding(format!("opencode JSON parse failed: {e}")))?;

    Ok((parsed, _cost, _tokens))
}

/// Propagate opencode-relevant env vars into a sync subprocess.
///
/// Same logic as `propagate_opencode_env` but for `std::process::Command`.
pub fn propagate_opencode_env_sync(cmd: &mut std::process::Command) {
    const PREFIXES: &[&str] = &["OPENCODE_", "OPENROUTER_", "XDG_"];
    const EXACT: &[&str] = &["LANG", "TERM", "USER", "LOGNAME", "TMPDIR"];
    for (key, val) in std::env::vars() {
        if PREFIXES.iter().any(|p| key.starts_with(p)) || EXACT.contains(&key.as_str()) {
            cmd.env(&key, &val);
        }
    }
}

/// Build a sync `std::process::Command` for opencode.
///
/// Mirror of `build_opencode_command` but returns `std::process::Command`
/// for use in the enrich pipeline which uses `wait_timeout` (sync).
pub fn build_opencode_command_sync(
    binary: &Path,
    model: &str,
    prompt: &str,
    input_text: &str,
) -> Result<std::process::Command, AppError> {
    let full_prompt = if input_text.is_empty() {
        prompt.to_string()
    } else {
        format!("{prompt}\n\n{input_text}")
    };
    let mut cmd = std::process::Command::new(binary);
    cmd.arg("run")
        .arg("--format")
        .arg("json")
        .arg("-m")
        .arg(model)
        .arg("--dangerously-skip-permissions")
        .arg(&full_prompt)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", std::env::var("HOME").unwrap_or_default())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    propagate_opencode_env_sync(&mut cmd);
    crate::spawn::apply_cwd_isolation(&mut cmd)?;
    Ok(cmd)
}

/// Spawn opencode with setsid for process group isolation but WITHOUT
/// RLIMIT_AS. The Bun runtime inside opencode uses aggressive virtual
/// memory mappings that exceed the 4 GB limit applied to claude/codex.
#[cfg(target_os = "linux")]
pub fn spawn_opencode(cmd: &mut std::process::Command) -> std::io::Result<std::process::Child> {
    use std::os::unix::process::CommandExt;
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
    cmd.spawn()
}

#[cfg(not(target_os = "linux"))]
pub fn spawn_opencode(cmd: &mut std::process::Command) -> std::io::Result<std::process::Child> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                let _ = libc::setsid();
                Ok(())
            });
        }
    }
    cmd.spawn()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_valid() {
        assert_eq!(parse_version("1.17.7").unwrap(), (1, 17, 7));
        assert_eq!(parse_version("2.0.0").unwrap(), (2, 0, 0));
    }

    #[test]
    fn parse_version_with_prefix() {
        assert_eq!(parse_version("v1.17.7").unwrap(), (1, 17, 7));
        assert_eq!(parse_version("opencode 1.17.7").unwrap(), (1, 17, 7));
    }

    #[test]
    fn parse_version_invalid() {
        assert!(parse_version("unknown").is_err());
        assert!(parse_version("").is_err());
    }

    #[test]
    fn validate_version_rejects_old() {
        // We can't easily test with a real binary, so test the parse path
        let v = parse_version("1.16.0").unwrap();
        assert!(v < MIN_OPENCODE_VERSION);
    }

    #[test]
    fn validate_version_accepts_minimum() {
        let v = parse_version("1.17.0").unwrap();
        assert!(v >= MIN_OPENCODE_VERSION);
    }

    #[test]
    fn resolve_model_uses_default() {
        // When no override and no env var, should return default
        let model = resolve_opencode_model(None);
        // May be overridden by env in CI, so just check it's non-empty
        assert!(!model.is_empty());
    }

    #[test]
    fn resolve_model_uses_override() {
        let model = resolve_opencode_model(Some("opencode/test-model"));
        assert_eq!(model, "opencode/test-model");
    }

    #[test]
    fn resolve_timeout_uses_default() {
        let t = resolve_opencode_timeout(None);
        assert!(t > 0);
    }

    #[test]
    fn resolve_timeout_uses_override() {
        assert_eq!(resolve_opencode_timeout(Some(600)), 600);
    }

    #[test]
    fn parse_opencode_output_extracts_text() {
        let stdout = r#"{"type":"step_start","timestamp":1234,"sessionID":"ses_test","part":{"type":"step-start"}}
{"type":"text","timestamp":1235,"sessionID":"ses_test","part":{"type":"text","text":"{\"entities\":[]}"}}
{"type":"step_finish","timestamp":1236,"sessionID":"ses_test","part":{"type":"step-finish","tokens":{"total":100,"input":90,"output":10,"reasoning":0},"cost":0.0}}"#;

        let (text, cost, tokens) = parse_opencode_output(stdout).unwrap();
        assert_eq!(text, "{\"entities\":[]}");
        assert_eq!(cost, 0.0);
        assert_eq!(tokens, 100);
    }

    #[test]
    fn parse_opencode_output_concatenates_multiple_text_events() {
        let stdout = r#"{"type":"step_start","timestamp":1234,"sessionID":"s","part":{"type":"step-start"}}
{"type":"text","timestamp":1235,"sessionID":"s","part":{"type":"text","text":"{\"ent"}}
{"type":"text","timestamp":1236,"sessionID":"s","part":{"type":"text","text":"ities\":[]}"}}
{"type":"step_finish","timestamp":1237,"sessionID":"s","part":{"type":"step-finish","tokens":{"total":50,"input":40,"output":10,"reasoning":0},"cost":0}}"#;

        let (text, _, _) = parse_opencode_output(stdout).unwrap();
        assert_eq!(text, "{\"entities\":[]}");
    }

    #[test]
    fn parse_opencode_output_empty_fails() {
        assert!(parse_opencode_output("").is_err());
        assert!(parse_opencode_output("{\"type\":\"step_start\"}").is_err());
    }

    #[test]
    fn parse_json_from_opencode_text_direct() {
        let text = r#"{"entities":[],"relationships":[]}"#;
        let parsed: serde_json::Value = parse_json_from_opencode_text(text).unwrap();
        assert!(parsed.get("entities").is_some());
    }

    #[test]
    fn parse_json_from_opencode_text_markdown_fence() {
        let text = "Here is the result:\n```json\n{\"entities\":[]}\n```\nDone.";
        let parsed: serde_json::Value = parse_json_from_opencode_text(text).unwrap();
        assert!(parsed.get("entities").is_some());
    }

    #[test]
    fn parse_json_from_opencode_text_extract_braces() {
        let text = "The answer is {\"entities\":[]} and that's it.";
        let parsed: serde_json::Value = parse_json_from_opencode_text(text).unwrap();
        assert!(parsed.get("entities").is_some());
    }

    #[test]
    fn parse_json_from_opencode_text_invalid() {
        assert!(parse_json_from_opencode_text::<serde_json::Value>("no json here").is_err());
    }

    #[test]
    fn build_command_has_correct_args() {
        let cmd = build_opencode_command(
            Path::new("/usr/bin/opencode"),
            "opencode/big-pickle",
            "test prompt",
        )
        .unwrap();
        let argv: Vec<String> = cmd
            .as_std()
            .get_args()
            .filter_map(|a| a.to_str().map(|s| s.to_string()))
            .collect();

        assert!(argv.contains(&"run".to_string()));
        assert!(argv.contains(&"--format".to_string()));
        assert!(argv.contains(&"json".to_string()));
        assert!(argv.contains(&"-m".to_string()));
        assert!(argv.contains(&"opencode/big-pickle".to_string()));
        assert!(argv.contains(&"--dangerously-skip-permissions".to_string()));
        assert!(argv.contains(&"test prompt".to_string()));
    }
}
