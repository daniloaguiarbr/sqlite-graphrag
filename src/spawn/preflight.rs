//! Pre-flight validation layer for LLM subprocess spawners (v1.0.87, ADR-0045).
//!
//! GAP-META-005: closes the architectural gap between `build_argv` and
//! `cmd.spawn()` in the four real subprocess spawn sites
//! (`claude_runner.rs:255`, `codex_spawn.rs:273`, `ingest_claude.rs:297`,
//! `extract/llm_embedding.rs:671`). Before this module, the 4-stage pipeline
//! was:
//!
//! ```text
//! 1. build_argv(mode, prompt, body)  -> Vec<OsString>
//! 2. apply_env_whitelist(cmd)         -> void (helper v1.0.83, ADR-0041)
//! 3. Command::spawn()                 -> io::Result<Child>
//! 4. child.wait_with_output()         -> io::Result<Output>
//! ```
//!
//! Stage 3 discovered failures AFTER the kernel fork and AFTER Claude Code
//! started executing, wasting tokens, locking job-singleton, and producing
//! opaque diagnostics. This module inserts a gate between stages 2 and 3
//! that catches the 5 bug-symptom classes documented in `gaps.md` BEFORE
//! the fork:
//!
//! - Bug 1 — `ingest --extraction-backend llm` extracts 0 entities silently
//! - Bug 2 — `--mcp-config '{}'` rejected by Claude Code 2.1.177
//! - Bug 3 — argv > ARG_MAX post-fork E2BIG
//! - Bug 4 — output parser truncates at 65536 chars
//! - Bug 5 — `.mcp.json` walk-up fails Zod validation
//!
//! Pattern: sibling of `env_whitelist.rs` (v1.0.83, ADR-0041). Same
//! design philosophy (helper consumed by all 4 spawn sites, no
//! caller-local reimplementation, opt-out via env var for emergencies).
//!
//! ## Invariante imposta
//!
//! `sqlite-graphrag` executa Claude Code e Codex CLI **obrigatoriamente
//! headless sem MCP**. Pre-flight rejeita argv que carrega MCP servers
//! explícitos antes do fork, fechando o caminho onde
//! `~/.claude/settings.json` ou walk-up de `.mcp.json` herdado poderia
//! reintroduzir plugins contra a policy.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Safety margin subtracted from `ARG_MAX` to leave room for env vars
/// and the binary path itself (those flow through a different syscall).
const ARG_MAX_SAFETY_MARGIN_BYTES: usize = 4_096;

/// Default fallback when `libc::sysconf(_SC_ARG_MAX)` returns -1 (rare
/// but documented on hardened kernels). Matches the Windows `CreateProcess`
/// cap of 32767 chars per command line. Visible on both unix and non-unix
/// so `arg_max_bytes()` can reference it from either branch.
const DEFAULT_ARG_MAX_BYTES: usize = 32_768;

/// Default max output bytes that downstream JSON parsers tolerate
/// without truncation. Matches the previous 64 KiB parser cap that
/// `serde_json::from_str` silently truncated in v1.0.86.
const DEFAULT_OUTPUT_BUFFER_LIMIT_BYTES: usize = 65_536;

/// Walk-up depth cap for `.mcp.json` traversal. Prevents pathological
/// `..` climbs on hosts with deeply nested CWDs.
const WALKUP_MAX_DEPTH: usize = 16;

/// Skip pre-flight checks entirely. Emergency escape hatch — strongly
/// discouraged. Operators accept the 5-bug-class risk by setting this.
pub fn is_skipped() -> bool {
    matches!(
        std::env::var("SQLITE_GRAPHRAG_SKIP_PREFLIGHT")
            .ok()
            .as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes")
    )
}

/// Arguments for the pre-flight validation gate.
///
/// Each caller populates exactly what the gate needs to validate without
/// relying on global env vars. The gate never mutates the argv in place —
/// it only reads and reports. Callers act on `PreFlightError` to substitute
/// alternatives (e.g. swap inline `--mcp-config '{}'` for a tempfile path).
#[derive(Debug)]
pub struct PreFlightArgs<'a> {
    /// Resolved path to the binary that will be spawned.
    pub binary_path: &'a Path,
    /// argv after `build_argv` finished. Includes binary path as argv[0].
    pub argv: &'a [OsString],
    /// CWD-style anchor for walk-up detection of `.mcp.json`.
    pub workspace_root: &'a Path,
    /// If the spawner constructs `--mcp-config '{...}'` literally, the
    /// gate returns `McpConfigInlineJsonRejected` with a suggested
    /// tempfile path the caller can substitute.
    pub mcp_config_inline_json: Option<&'a str>,
    /// Caller's estimate of the maximum output payload size in bytes.
    /// Triggers `OutputBufferTooSmall` when above the documented parser cap.
    pub expected_output_bytes: usize,
    /// Stable label emitted in telemetry. One of `"claude_runner"`,
    /// `"codex_spawn"`, `"ingest_claude"`, `"ingest_codex"`,
    /// `"llm_embedding"`.
    pub spawner_name: &'static str,
}

/// Structured errors from the pre-flight gate. Each variant carries the
/// data needed for an operator to diagnose without re-running.
///
/// `thiserror` produces the `Display` impl that `AppError::PreFlightFailed`
/// captures into the `detail` field for i18n.
#[derive(Debug, Error)]
pub enum PreFlightError {
    /// Binary at `path` does not exist on the filesystem.
    #[error("binary not found: {path}")]
    BinaryNotFound { path: PathBuf },

    /// Total bytes of argv (binary + args + separators) exceed
    /// `ARG_MAX - 4096`. Spawn would fail with `E2BIG` post-fork.
    #[error("argv exceeds ARG_MAX: total_bytes={total_bytes}, arg_max={arg_max}, safety_margin_bytes={ARG_MAX_SAFETY_MARGIN_BYTES}")]
    ArgvExceedsArgMax { total_bytes: usize, arg_max: usize },

    /// `--mcp-config '{...}'` was passed literally as the inline JSON.
    /// Claude Code 2.1.177+ expects a filepath. Caller should use the
    /// `suggested_tempfile` (already written with empty `mcpServers` map).
    #[error("--mcp-config expects filepath, got inline JSON '{0}'; Claude Code 2.1.177 rejects this form; substitute suggested tempfile")]
    McpConfigInlineJsonRejected(String),

    /// `--mcp-config <PATH>` was passed but the path does not exist.
    #[error("--mcp-config path missing: {path}")]
    McpConfigPathMissing { path: PathBuf },

    /// `--mcp-config <PATH>` was passed but the file is not valid JSON.
    #[error("--mcp-config path invalid JSON at {path}: {error}")]
    McpConfigPathInvalidJson { path: PathBuf, error: String },

    /// `.mcp.json` walk-up found an invalid file at `path`. Override
    /// `CLAUDE_CONFIG_DIR` to an empty directory to suppress walk-up.
    #[error(".mcp.json walk-up found invalid file at {path}: {error}; set CLAUDE_CONFIG_DIR to an empty directory or move the workspace to a parent without .mcp.json")]
    WalkUpMcpJsonInvalid { path: PathBuf, error: String },

    /// Caller's expected output exceeds the documented JSON parser cap.
    /// The downstream parser truncates silently above this size.
    #[error("output buffer too small: expected={expected} bytes, configured_limit={configured} bytes; chunk the request or increase the buffer cap")]
    OutputBufferTooSmall { expected: usize, configured: usize },

    /// `CLAUDE_CONFIG_DIR` is set and `settings.json` declares active
    /// `mcpServers`. Claude Code would load them and defeat
    /// `--strict-mcp-config --mcp-config <empty>`. Hooks are NOT
    /// flagged here because the spawners pass
    /// `--settings '{"hooks":{}}'` which overrides the user-level
    /// hooks at the CLI invocation boundary; MCP servers are NOT
    /// overridden by any flag we pass, so they are the only class of
    /// `settings.json` entry that can leak into the subprocess.
    #[error("CLAUDE_CONFIG_DIR={path} contains settings.json with active MCP servers ({reason}); unset the env var or remove the offending entries")]
    ClaudeConfigDirNotEmpty { path: PathBuf, reason: &'static str },
}

/// Returns `Ok(())` when all checks pass, or the first failing variant.
///
/// Short-circuits on first failure to give operators a single actionable
/// diagnostic. When `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` is set, returns
/// `Ok(())` unconditionally after logging a warning (emergency escape
/// hatch).
pub fn preflight_check(args: &PreFlightArgs) -> Result<(), PreFlightError> {
    if is_skipped() {
        tracing::warn!(
            target: "preflight",
            event = "preflight_skipped",
            spawner = args.spawner_name,
            "SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1 — pre-flight checks bypassed; the 5-bug-class risk is accepted"
        );
        return Ok(());
    }

    // Order matters: cheap in-memory checks first, I/O-bound checks last
    // so a binary-missing operator sees the actionable error first.
    let argv_total = compute_argv_bytes(args.argv);

    check_argv_size(argv_total)?;
    check_binary_exists(args.binary_path)?;
    check_output_buffer(args.expected_output_bytes)?;
    check_mcp_config_inline(args.mcp_config_inline_json)?;
    check_mcp_config_path(args.argv)?;
    check_walkup_mcp_json(args.workspace_root)?;
    check_claude_config_dir()?;

    tracing::info!(
        target: "preflight",
        event = "preflight_passed",
        spawner = args.spawner_name,
        argv_bytes = argv_total,
        workspace_root = %args.workspace_root.display(),
        "pre-flight validation passed"
    );
    Ok(())
}

/// Writes an empty MCP config tempfile with `{"mcpServers":{}}` and
/// returns the path. Callers should `cmd.arg(path.as_os_str())` to
/// substitute for the inline `'{}'` literal rejected by Claude Code 2.1.177.
///
/// Tempfile lives in the OS temp dir with a `graphrag-mcp-` prefix.
/// Caller is responsible for keeping the path alive until the spawned
/// process terminates; `tempfile::NamedTempFile` cleans up on Drop.
pub fn write_empty_mcp_config_tempfile() -> Result<PathBuf, std::io::Error> {
    use std::io::Write;
    let mut tmp = tempfile::Builder::new()
        .prefix("graphrag-mcp-")
        .suffix(".json")
        .tempfile()?;
    tmp.write_all(br#"{"mcpServers":{}}"#)?;
    tmp.flush()?;
    // Persist (do not auto-delete) so the spawned claude can read it
    // after this function returns. The caller spawns and waits, then
    // the tempfile is dropped and cleaned.
    let (_, path) = tmp.keep()?;
    Ok(path)
}

// ---------------------------------------------------------------------------
// Individual guards
// ---------------------------------------------------------------------------

/// Sums byte sizes of each argv element plus 1 byte for the NUL separator
/// in the kernel's `execve` argument buffer layout.
fn compute_argv_bytes(argv: &[OsString]) -> usize {
    argv.iter().map(|s| s.as_os_str().len() + 1).sum()
}

fn arg_max_bytes() -> usize {
    #[cfg(unix)]
    {
        // SAFETY: `sysconf(_SC_ARG_MAX)` is async-signal-safe per POSIX.1-2008
        // §2.4.3. It returns -1 on error (which we treat as "use the safe
        // fallback"); a positive value is the kernel's ARG_MAX in bytes.
        let n = unsafe { libc::sysconf(libc::_SC_ARG_MAX) };
        if n > 0 {
            n as usize
        } else {
            DEFAULT_ARG_MAX_BYTES
        }
    }
    #[cfg(not(unix))]
    {
        DEFAULT_ARG_MAX_BYTES
    }
}

fn check_argv_size(argv_total: usize) -> Result<(), PreFlightError> {
    let max = arg_max_bytes();
    if argv_total + ARG_MAX_SAFETY_MARGIN_BYTES > max {
        return Err(PreFlightError::ArgvExceedsArgMax {
            total_bytes: argv_total,
            arg_max: max,
        });
    }
    Ok(())
}

fn check_binary_exists(binary_path: &Path) -> Result<(), PreFlightError> {
    if binary_path.exists() {
        Ok(())
    } else {
        Err(PreFlightError::BinaryNotFound {
            path: binary_path.to_path_buf(),
        })
    }
}

fn check_output_buffer(expected: usize) -> Result<(), PreFlightError> {
    if expected > DEFAULT_OUTPUT_BUFFER_LIMIT_BYTES {
        Err(PreFlightError::OutputBufferTooSmall {
            expected,
            configured: DEFAULT_OUTPUT_BUFFER_LIMIT_BYTES,
        })
    } else {
        Ok(())
    }
}

fn check_mcp_config_inline(inline: Option<&str>) -> Result<(), PreFlightError> {
    if let Some(s) = inline {
        // Any literal JSON starting with `{` and `}` is treated as
        // inline. Caller must convert to filepath.
        let trimmed = s.trim();
        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            return Err(PreFlightError::McpConfigInlineJsonRejected(s.to_string()));
        }
    }
    Ok(())
}

fn check_mcp_config_path(argv: &[OsString]) -> Result<(), PreFlightError> {
    let mut iter = argv.iter();
    while let Some(arg) = iter.next() {
        // BUG-5 fix (v1.0.88): accept the `--mcp-config=PATH` form
        // (single argv slot) alongside the GNU `--mcp-config <PATH>`
        // form. Without this, callers using clap's `--flag value`
        // collapsing (or hand-rolled commands) bypass the guard.
        let path = if arg == "--mcp-config" {
            match iter.next() {
                Some(value) => PathBuf::from(value),
                None => continue,
            }
        } else if let Some(stripped) = arg.to_str().and_then(|s| s.strip_prefix("--mcp-config=")) {
            PathBuf::from(stripped)
        } else {
            continue;
        };
        validate_mcp_config_path(&path)?;
    }
    Ok(())
}

fn validate_mcp_config_path(path: &Path) -> Result<(), PreFlightError> {
    if !path.exists() {
        return Err(PreFlightError::McpConfigPathMissing {
            path: path.to_path_buf(),
        });
    }
    let contents =
        std::fs::read_to_string(path).map_err(|e| PreFlightError::McpConfigPathInvalidJson {
            path: path.to_path_buf(),
            error: e.to_string(),
        })?;
    if let Err(e) = serde_json::from_str::<serde_json::Value>(&contents) {
        return Err(PreFlightError::McpConfigPathInvalidJson {
            path: path.to_path_buf(),
            error: e.to_string(),
        });
    }
    Ok(())
}

fn check_walkup_mcp_json(workspace_root: &Path) -> Result<(), PreFlightError> {
    let mut current = workspace_root.to_path_buf();
    for _ in 0..WALKUP_MAX_DEPTH {
        let candidate = current.join(".mcp.json");
        if candidate.exists() {
            let contents = std::fs::read_to_string(&candidate).map_err(|e| {
                PreFlightError::WalkUpMcpJsonInvalid {
                    path: candidate.clone(),
                    error: e.to_string(),
                }
            })?;
            // BUG-9 fix (v1.0.88): syntactic JSON validity is necessary
            // but NOT sufficient — a valid `.mcp.json` can still declare
            // MCP servers under `mcpServers`. Reject when the file is
            // syntactically valid AND declares a non-empty `mcpServers`
            // object. Keep the existing syntactic check for legacy
            // callers that hand-roll untyped JSON.
            let parsed: serde_json::Value = serde_json::from_str(&contents).map_err(|e| {
                PreFlightError::WalkUpMcpJsonInvalid {
                    path: candidate.clone(),
                    error: e.to_string(),
                }
            })?;
            let has_active_mcps = parsed
                .get("mcpServers")
                .and_then(|v| v.as_object())
                .map(|o| !o.is_empty())
                .unwrap_or(false);
            if has_active_mcps {
                return Err(PreFlightError::WalkUpMcpJsonInvalid {
                    path: candidate,
                    error: "mcpServers declares active entries; set CLAUDE_CONFIG_DIR to an empty directory or remove the file".to_string(),
                });
            }
            return Ok(());
        }
        match current.parent() {
            Some(p) => current = p.to_path_buf(),
            None => break,
        }
    }
    Ok(())
}

fn check_claude_config_dir() -> Result<(), PreFlightError> {
    let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") else {
        return Ok(());
    };
    let path = PathBuf::from(&dir);
    if !path.is_dir() {
        return Ok(());
    }
    // BUG-1 fix (v1.0.88): inspect `settings.json` semantically. A
    // populated directory containing `CLAUDE.md`, custom `commands/`,
    // or skills is harmless — Claude Code will not auto-load MCP
    // servers or hooks unless `settings.json` declares them. The
    // previous implementation rejected any non-empty directory, which
    // broke every dev install that points `CLAUDE_CONFIG_DIR` at the
    // real Claude Code configuration home.
    let settings = path.join("settings.json");
    if !settings.exists() {
        // Directory populated with non-MCP files (CLAUDE.md,
        // commands/, skills/, etc.) — emit a structured warning so
        // operators can audit, but do NOT abort the spawn.
        if std::fs::read_dir(&path)
            .map(|mut i| i.next().is_some())
            .unwrap_or(false)
        {
            tracing::warn!(
                target: "preflight",
                path = %path.display(),
                "CLAUDE_CONFIG_DIR is populated but contains no settings.json; \
                 MCP servers and hooks will not be auto-loaded"
            );
        }
        return Ok(());
    }
    let contents = match std::fs::read_to_string(&settings) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                target: "preflight",
                path = %settings.display(),
                error = %e,
                "CLAUDE_CONFIG_DIR/settings.json exists but could not be read; \
                 skipping semantic validation"
            );
            return Ok(());
        }
    };
    let parsed: serde_json::Value = match serde_json::from_str(&contents) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                target: "preflight",
                path = %settings.display(),
                error = %e,
                "CLAUDE_CONFIG_DIR/settings.json is not valid JSON; \
                 skipping semantic validation"
            );
            return Ok(());
        }
    };
    // Reject when settings.json declares active MCP servers. Hooks are
    // tolerated because the spawners pass `--settings '{"hooks":{}}'`
    // which overrides the user-level hooks at the CLI boundary.
    let has_mcp_servers = parsed
        .get("mcpServers")
        .and_then(|v| v.as_object())
        .map(|o| !o.is_empty())
        .unwrap_or(false);
    if has_mcp_servers {
        return Err(PreFlightError::ClaudeConfigDirNotEmpty {
            path,
            reason: "mcpServers",
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests (GAP-META-005 test plan, 15 cases)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn dummy_argv() -> Vec<OsString> {
        vec![
            OsString::from("/usr/bin/claude"),
            OsString::from("-p"),
            OsString::from("hello"),
        ]
    }

    fn dummy_args<'a>(
        binary: &'a Path,
        argv: &'a [OsString],
        inline_json: Option<&'a str>,
    ) -> PreFlightArgs<'a> {
        // Use a dedicated empty tempdir for workspace_root so walk-up of
        // `.mcp.json` does not pick up unrelated files in the test's CWD.
        // The tempdir is leaked (kept alive for the test lifetime) via
        // `OnceLock` to keep the API simple.
        use std::sync::OnceLock;
        static WORKSPACE: OnceLock<tempfile::TempDir> = OnceLock::new();
        let workspace = WORKSPACE.get_or_init(|| tempfile::tempdir().expect("tempdir"));
        PreFlightArgs {
            binary_path: binary,
            argv,
            workspace_root: workspace.path(),
            mcp_config_inline_json: inline_json,
            expected_output_bytes: 1024,
            spawner_name: "test",
        }
    }

    #[test]
    #[serial_test::serial(env)]
    fn check_binary_exists_passes_when_path_valid() {
        // SAFETY: serial_test::serial(env) ensures no parallel mutation.
        let saved = std::env::var_os("CLAUDE_CONFIG_DIR");
        unsafe {
            std::env::remove_var("CLAUDE_CONFIG_DIR");
        }
        let binary = if cfg!(windows) {
            "C:\\Windows\\System32\\cmd.exe"
        } else {
            "/bin/sh"
        };
        let argv = dummy_argv();
        let args = dummy_args(Path::new(binary), &argv, None);
        let result = preflight_check(&args);
        if let Some(v) = saved {
            unsafe {
                std::env::set_var("CLAUDE_CONFIG_DIR", v);
            }
        }
        assert!(result.is_ok(), "preflight returned: {result:?}");
    }

    #[test]
    fn check_binary_exists_fails_when_missing() {
        let argv = dummy_argv();
        let args = dummy_args(Path::new("/does/not/exist/claude-binary"), &argv, None);
        let err = preflight_check(&args).unwrap_err();
        assert!(
            matches!(err, PreFlightError::BinaryNotFound { .. }),
            "expected BinaryNotFound, got {err:?}"
        );
    }

    #[test]
    #[serial_test::serial(env)]
    fn check_argv_size_passes_under_limit() {
        let saved = std::env::var_os("CLAUDE_CONFIG_DIR");
        unsafe {
            std::env::remove_var("CLAUDE_CONFIG_DIR");
        }
        let argv = dummy_argv();
        let args = dummy_args(Path::new("/bin/sh"), &argv, None);
        let result = preflight_check(&args);
        if let Some(v) = saved {
            unsafe {
                std::env::set_var("CLAUDE_CONFIG_DIR", v);
            }
        }
        // dummy_argv() is tiny — well under ARG_MAX.
        assert!(result.is_ok(), "preflight returned: {result:?}");
    }

    #[test]
    #[serial_test::serial(env)]
    fn check_argv_size_fails_when_exceeds_arg_max() {
        let saved = std::env::var_os("CLAUDE_CONFIG_DIR");
        unsafe {
            std::env::remove_var("CLAUDE_CONFIG_DIR");
        }
        // Synthesize an argv that exceeds ARG_MAX regardless of the
        // host value. We allocate 64 MiB to leave the 4 KiB safety
        // margin well below `getconf ARG_MAX` on every supported OS.
        let huge = "x".repeat(64 * 1024 * 1024);
        let argv = vec![OsString::from("/bin/sh"), OsString::from(huge)];
        let args = dummy_args(Path::new("/bin/sh"), &argv, None);
        let err = preflight_check(&args).unwrap_err();
        if let Some(v) = saved {
            unsafe {
                std::env::set_var("CLAUDE_CONFIG_DIR", v);
            }
        }
        assert!(
            matches!(err, PreFlightError::ArgvExceedsArgMax { .. }),
            "expected ArgvExceedsArgMax, got {err:?}"
        );
    }

    #[test]
    fn check_mcp_inline_json_detects_literal_braces() {
        // argv references /bin/sh (exists) so the binary check passes.
        let argv = dummy_argv();
        let args = dummy_args(Path::new("/bin/sh"), &argv, Some("{}"));
        let err = preflight_check(&args).unwrap_err();
        assert!(
            matches!(err, PreFlightError::McpConfigInlineJsonRejected(_)),
            "expected McpConfigInlineJsonRejected, got {err:?}"
        );
    }

    #[test]
    fn check_mcp_inline_json_writes_valid_tempfile() {
        // Round-trip: write_empty_mcp_config_tempfile produces a file
        // parseable as JSON containing `mcpServers: {}`.
        let path = write_empty_mcp_config_tempfile().expect("tempfile write");
        let contents = std::fs::read_to_string(&path).expect("tempfile read");
        let parsed: serde_json::Value =
            serde_json::from_str(&contents).expect("tempfile valid JSON");
        assert!(parsed.get("mcpServers").is_some());
        assert!(parsed["mcpServers"].as_object().unwrap().is_empty());
        // Cleanup
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn check_mcp_path_missing_returns_error() {
        // Build an argv with --mcp-config pointing at a nonexistent path.
        let argv = vec![
            OsString::from("/bin/sh"),
            OsString::from("--mcp-config"),
            OsString::from("/nonexistent/path/mcp.json"),
        ];
        let args = dummy_args(Path::new("/bin/sh"), &argv, None);
        let err = preflight_check(&args).unwrap_err();
        assert!(
            matches!(err, PreFlightError::McpConfigPathMissing { .. }),
            "expected McpConfigPathMissing, got {err:?}"
        );
    }

    #[test]
    fn check_mcp_path_invalid_json_returns_error() {
        // Write an invalid JSON tempfile then reference it.
        let tmp = tempfile::NamedTempFile::new().expect("tempfile");
        std::fs::write(tmp.path(), b"this is not json").expect("write");
        let argv = vec![
            OsString::from("/bin/sh"),
            OsString::from("--mcp-config"),
            OsString::from(tmp.path().to_string_lossy().into_owned()),
        ];
        let args = dummy_args(Path::new("/bin/sh"), &argv, None);
        let err = preflight_check(&args).unwrap_err();
        assert!(
            matches!(err, PreFlightError::McpConfigPathInvalidJson { .. }),
            "expected McpConfigPathInvalidJson, got {err:?}"
        );
    }

    #[test]
    fn check_walkup_mcp_json_passes_when_clean() {
        // Use a dedicated tempdir created for the test (guaranteed empty).
        let dir = tempfile::tempdir().expect("tempdir");
        let argv = dummy_argv();
        let args = PreFlightArgs {
            workspace_root: dir.path(),
            ..dummy_args(Path::new("/bin/sh"), &argv, None)
        };
        let result = preflight_check(&args);
        // We only assert we did NOT return WalkUpMcpJsonInvalid for a
        // clean workspace.
        if let Err(PreFlightError::WalkUpMcpJsonInvalid { .. }) = &result {
            panic!("walk-up incorrectly flagged on clean workspace");
        }
    }

    #[test]
    fn check_walkup_mcp_json_fails_on_zod_invalid() {
        // Create a temp workspace dir with an invalid .mcp.json inside.
        let dir = tempfile::tempdir().expect("tempdir");
        let bad = dir.path().join(".mcp.json");
        std::fs::write(&bad, b"{not json").expect("write bad mcp.json");
        let argv = dummy_argv();
        let args = PreFlightArgs {
            workspace_root: dir.path(),
            ..dummy_args(Path::new("/bin/sh"), &argv, None)
        };
        let err = preflight_check(&args).unwrap_err();
        assert!(
            matches!(err, PreFlightError::WalkUpMcpJsonInvalid { .. }),
            "expected WalkUpMcpJsonInvalid, got {err:?}"
        );
    }

    #[test]
    fn check_walkup_mcp_json_fails_on_active_mcp_servers() {
        // BUG-9 regression: a syntactically valid `.mcp.json` that
        // declares MCP servers under `mcpServers` must be rejected.
        let dir = tempfile::tempdir().expect("tempdir");
        let bad = dir.path().join(".mcp.json");
        std::fs::write(
            &bad,
            r#"{"mcpServers":{"github":{"command":"gh","args":["mcp"]}}}"#,
        )
        .expect("write bad mcp.json");
        let argv = dummy_argv();
        let args = PreFlightArgs {
            workspace_root: dir.path(),
            ..dummy_args(Path::new("/bin/sh"), &argv, None)
        };
        let err = preflight_check(&args).unwrap_err();
        assert!(
            matches!(err, PreFlightError::WalkUpMcpJsonInvalid { .. }),
            "expected WalkUpMcpJsonInvalid, got {err:?}"
        );
    }

    #[test]
    fn check_walkup_mcp_json_passes_with_empty_mcp_servers() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ok = dir.path().join(".mcp.json");
        std::fs::write(&ok, r#"{"mcpServers":{}}"#).expect("write");
        let argv = dummy_argv();
        let args = PreFlightArgs {
            workspace_root: dir.path(),
            ..dummy_args(Path::new("/bin/sh"), &argv, None)
        };
        let result = preflight_check(&args);
        if let Err(PreFlightError::WalkUpMcpJsonInvalid { .. }) = &result {
            panic!("empty mcpServers must pass walk-up: {result:?}");
        }
    }

    #[test]
    fn check_mcp_path_equals_form_detects_missing_file() {
        // BUG-5 regression: --mcp-config=PATH single-slot form must be
        // caught the same as the GNU --mcp-config <PATH> form.
        let argv = vec![
            OsString::from("/bin/sh"),
            OsString::from("--mcp-config=/nonexistent/path/mcp.json"),
        ];
        let args = dummy_args(Path::new("/bin/sh"), &argv, None);
        let err = preflight_check(&args).unwrap_err();
        assert!(
            matches!(err, PreFlightError::McpConfigPathMissing { .. }),
            "expected McpConfigPathMissing, got {err:?}"
        );
    }

    #[test]
    fn check_output_buffer_warns_when_oversized() {
        let argv = dummy_argv();
        let args = PreFlightArgs {
            expected_output_bytes: 100_000, // > 65536 cap
            ..dummy_args(Path::new("/bin/sh"), &argv, None)
        };
        let err = preflight_check(&args).unwrap_err();
        assert!(
            matches!(err, PreFlightError::OutputBufferTooSmall { .. }),
            "expected OutputBufferTooSmall, got {err:?}"
        );
    }

    #[test]
    #[serial_test::serial(env)]
    fn check_claude_config_dir_fails_when_settings_has_active_mcps() {
        // SAFETY: serial_test::serial(env) ensures no parallel mutation.
        let dir = tempfile::tempdir().expect("tempdir");
        let settings = dir.path().join("settings.json");
        std::fs::write(
            &settings,
            r#"{"mcpServers":{"github":{"command":"gh","args":["mcp"]}}}"#,
        )
        .expect("write settings.json");
        unsafe {
            std::env::set_var("CLAUDE_CONFIG_DIR", dir.path());
        }
        let argv = dummy_argv();
        let args = dummy_args(Path::new("/bin/sh"), &argv, None);
        let err = preflight_check(&args);
        unsafe {
            std::env::remove_var("CLAUDE_CONFIG_DIR");
        }
        if let Err(PreFlightError::ClaudeConfigDirNotEmpty { reason, .. }) = err {
            assert_eq!(reason, "mcpServers");
        } else {
            panic!("expected ClaudeConfigDirNotEmpty mcpServers, got {err:?}");
        }
    }

    #[test]
    #[serial_test::serial(env)]
    fn check_claude_config_dir_passes_when_settings_empty() {
        // SAFETY: serial_test::serial(env) ensures no parallel mutation.
        let dir = tempfile::tempdir().expect("tempdir");
        let settings = dir.path().join("settings.json");
        std::fs::write(&settings, r#"{"mcpServers":{},"hooks":{}}"#).expect("write");
        unsafe {
            std::env::set_var("CLAUDE_CONFIG_DIR", dir.path());
        }
        let argv = dummy_argv();
        let args = dummy_args(Path::new("/bin/sh"), &argv, None);
        let result = preflight_check(&args);
        unsafe {
            std::env::remove_var("CLAUDE_CONFIG_DIR");
        }
        assert!(result.is_ok(), "empty MCPs and hooks must pass: {result:?}");
    }

    #[test]
    #[serial_test::serial(env)]
    fn check_claude_config_dir_passes_when_no_settings_json() {
        // SAFETY: serial_test::serial(env) ensures no parallel mutation.
        let dir = tempfile::tempdir().expect("tempdir");
        // Populate with non-MCP files only (CLAUDE.md, commands/, etc).
        std::fs::write(dir.path().join("CLAUDE.md"), "# project notes").expect("write");
        unsafe {
            std::env::set_var("CLAUDE_CONFIG_DIR", dir.path());
        }
        let argv = dummy_argv();
        let args = dummy_args(Path::new("/bin/sh"), &argv, None);
        let result = preflight_check(&args);
        unsafe {
            std::env::remove_var("CLAUDE_CONFIG_DIR");
        }
        assert!(
            result.is_ok(),
            "populated dir without settings.json must pass: {result:?}"
        );
    }

    #[test]
    #[serial_test::serial(env)]
    fn check_claude_config_dir_passes_when_settings_has_only_hooks() {
        // Hooks are tolerated because the spawners override
        // `--settings '{"hooks":{}}'` at the CLI boundary; only MCP
        // servers are flagged as a hard error.
        let dir = tempfile::tempdir().expect("tempdir");
        let settings = dir.path().join("settings.json");
        std::fs::write(&settings, r#"{"hooks":{"PreToolUse":[]}}"#).expect("write");
        unsafe {
            std::env::set_var("CLAUDE_CONFIG_DIR", dir.path());
        }
        let argv = dummy_argv();
        let args = dummy_args(Path::new("/bin/sh"), &argv, None);
        let result = preflight_check(&args);
        unsafe {
            std::env::remove_var("CLAUDE_CONFIG_DIR");
        }
        assert!(result.is_ok(), "hooks must be tolerated: {result:?}");
    }

    #[test]
    fn preflight_check_runs_all_guards_in_order() {
        // Valid path + clean argv + clean workspace + no inline JSON.
        let dir = tempfile::tempdir().expect("tempdir");
        let argv = dummy_argv();
        let args = PreFlightArgs {
            workspace_root: dir.path(),
            ..dummy_args(Path::new("/bin/sh"), &argv, None)
        };
        assert!(preflight_check(&args).is_ok());
    }

    #[test]
    fn preflight_check_short_circuits_on_first_failure() {
        // Invalid binary + bad inline JSON — should report BinaryNotFound
        // first (cheap in-memory check) NOT the McpConfigInlineJsonRejected
        // (also cheap, but binary is checked earlier in the order).
        let argv = dummy_argv();
        let args = dummy_args(Path::new("/does/not/exist/at/all"), &argv, Some("{}"));
        let err = preflight_check(&args).unwrap_err();
        assert!(
            matches!(err, PreFlightError::BinaryNotFound { .. }),
            "expected BinaryNotFound (short-circuit), got {err:?}"
        );
    }

    #[test]
    #[serial_test::serial(env)]
    fn app_error_preflight_failed_has_exit_code_16() {
        // Cross-check the integration: AppError::PreFlightFailed maps to
        // exit code 16 (validated by this test, not by preflight itself).
        use crate::errors::AppError;
        let err: AppError = crate::spawn::preflight::PreFlightError::BinaryNotFound {
            path: "/bin/test".into(),
        }
        .into();
        assert_eq!(err.exit_code(), 16);
        assert!(err.is_permanent());
        assert!(!err.is_retryable());
    }
}
