//! Env whitelist for LLM subprocess spawners (v1.0.83, ADR-0041).
//!
//! Unifies the duplicated `env_clear()` + re-injection logic that previously
//! lived in `src/commands/{claude_runner,codex_spawn,ingest_claude}.rs`.
//!
//! ## OAuth-only mandate preserved
//!
//! `ANTHROPIC_API_KEY` and `OPENAI_API_KEY` are INTENTIONALLY ABSENT —
//! rejected by upstream guards in `claude_runner.rs`, `codex_spawn.rs`,
//! `ingest_claude.rs` and `extract/llm_embedding.rs` per ADR-0011, ADR-0025
//! and ADR-0041. The guards reject these vars regardless of whether they
//! reach the subprocess; the env whitelist is the SECOND line of defence.
//!
//! ## Custom provider support (v1.0.83)
//!
//! `ANTHROPIC_AUTH_TOKEN` and `ANTHROPIC_BASE_URL` are preserved so that
//! Claude Code can authenticate against a custom Anthropic-compatible
//! endpoint (MiniMax/api.minimax.io, OpenRouter, corporate gateways). The
//! `--bare` flag remains PROHIBITED — these vars only flow to the
//! subprocess when the user opts into a custom provider via env vars.
//!
//! ## Strict mode (compliance)
//!
//! When `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR=1` (or `--strict-env-clear` flag)
//! is active, only `PATH` is preserved. This covers environments that
//! forbid credential forwarding via env vars entirely.

use std::process::Command;

/// Environment variables preserved when spawning Claude/Codex subprocesses.
///
/// Order is purely cosmetic — `env_clear()` followed by per-var `env()` is
/// independent of iteration order.
pub const PRESERVED_ENV_VARS: &[&str] = &[
    // Standard POSIX / XDG base directory
    "PATH",
    "HOME",
    "USER",
    "SHELL",
    "TERM",
    "LANG",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_RUNTIME_DIR",
    "XDG_CACHE_HOME",
    // Temporary directories
    "TMPDIR",
    "TMP",
    "TEMP",
    // macOS dynamic linker fallback path
    "DYLD_FALLBACK_LIBRARY_PATH",
    // Claude Code specific
    "CLAUDE_CONFIG_DIR",
    // v1.0.83 (ADR-0041): custom provider credentials for Claude Code
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "CLAUDE_CODE_ENTRYPOINT",
    // v1.0.83 (ADR-0041): custom provider credentials for Codex CLI
    "CODEX_ACCESS_TOKEN",
    "OPENAI_BASE_URL",
    // v1.0.83 (ADR-0041): telemetry opt-out and observability override
    "DISABLE_TELEMETRY",
    "OTEL_EXPORTER_OTLP_ENDPOINT",
];

/// Windows-only environment variables preserved alongside the POSIX set.
#[cfg(windows)]
pub const PRESERVED_ENV_VARS_WINDOWS: &[&str] = &[
    "LOCALAPPDATA",
    "APPDATA",
    "USERPROFILE",
    "SystemRoot",
    "COMSPEC",
    "PATHEXT",
    "HOMEPATH",
    "HOMEDRIVE",
];

/// Apply the v1.0.83 env whitelist to a `Command`.
///
/// In strict mode, only `PATH` is preserved (compliance environments).
/// In default mode, the full `PRESERVED_ENV_VARS` set is applied.
pub fn apply_env_whitelist(cmd: &mut Command, strict: bool) {
    cmd.env_clear();
    if strict {
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", path);
        }
        return;
    }
    for var in PRESERVED_ENV_VARS {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }
    #[cfg(windows)]
    for var in PRESERVED_ENV_VARS_WINDOWS {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }
}

/// Detect whether strict env-clear mode is requested.
///
/// Returns true when `SQLITE_GRAPHRAG_STRICT_ENV_CLEAR` is `1`, `true`,
/// `TRUE` or `yes` (case-insensitive for `true`/`yes`).
pub fn is_strict_env_clear() -> bool {
    matches!(
        std::env::var("SQLITE_GRAPHRAG_STRICT_ENV_CLEAR")
            .ok()
            .as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("True") | Some("yes") | Some("YES")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper that records the env vars set on a Command without spawning it.
    fn captured_env(cmd: &Command) -> Vec<(String, String)> {
        cmd.get_envs()
            .filter_map(|(k, v)| {
                let k = k.to_str()?.to_string();
                let v = v?.to_str()?.to_string();
                Some((k, v))
            })
            .collect()
    }

    #[test]
    #[serial_test::serial(env)]
    fn whitelist_includes_custom_provider_vars() {
        // SAFETY: serial_test::serial(env) ensures no parallel mutation.
        unsafe {
            std::env::set_var("ANTHROPIC_AUTH_TOKEN", "sk-cp-test");
            std::env::set_var("ANTHROPIC_BASE_URL", "https://api.minimax.io/anthropic");
            std::env::set_var("OPENAI_BASE_URL", "https://api.openrouter.ai/v1");
        }
        let mut cmd = std::process::Command::new("/usr/bin/false");
        apply_env_whitelist(&mut cmd, false);
        let envs = captured_env(&cmd);
        let has_token = envs
            .iter()
            .any(|(k, v)| k == "ANTHROPIC_AUTH_TOKEN" && v == "sk-cp-test");
        let has_anthropic_url = envs
            .iter()
            .any(|(k, v)| k == "ANTHROPIC_BASE_URL" && v == "https://api.minimax.io/anthropic");
        let has_openai_url = envs
            .iter()
            .any(|(k, v)| k == "OPENAI_BASE_URL" && v == "https://api.openrouter.ai/v1");
        unsafe {
            std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
            std::env::remove_var("ANTHROPIC_BASE_URL");
            std::env::remove_var("OPENAI_BASE_URL");
        }
        assert!(has_token, "ANTHROPIC_AUTH_TOKEN not preserved");
        assert!(has_anthropic_url, "ANTHROPIC_BASE_URL not preserved");
        assert!(has_openai_url, "OPENAI_BASE_URL not preserved");
    }

    #[test]
    #[serial_test::serial(env)]
    fn whitelist_excludes_api_key_vars() {
        // SAFETY: serial_test::serial(env) ensures no parallel mutation.
        unsafe {
            std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-violation");
            std::env::set_var("OPENAI_API_KEY", "sk-violation");
        }
        let mut cmd = std::process::Command::new("/usr/bin/false");
        apply_env_whitelist(&mut cmd, false);
        let envs = captured_env(&cmd);
        let has_anthropic_key = envs.iter().any(|(k, _)| k == "ANTHROPIC_API_KEY");
        let has_openai_key = envs.iter().any(|(k, _)| k == "OPENAI_API_KEY");
        unsafe {
            std::env::remove_var("ANTHROPIC_API_KEY");
            std::env::remove_var("OPENAI_API_KEY");
        }
        assert!(
            !has_anthropic_key,
            "ANTHROPIC_API_KEY must NEVER reach subprocess"
        );
        assert!(
            !has_openai_key,
            "OPENAI_API_KEY must NEVER reach subprocess"
        );
    }

    #[test]
    #[serial_test::serial(env)]
    fn strict_mode_drops_credentials() {
        // SAFETY: serial_test::serial(env) ensures no parallel mutation.
        unsafe {
            std::env::set_var("ANTHROPIC_AUTH_TOKEN", "sk-cp-strict-test");
            std::env::set_var("PATH", "/usr/bin:/bin");
        }
        let mut cmd = std::process::Command::new("/usr/bin/false");
        apply_env_whitelist(&mut cmd, true);
        let envs = captured_env(&cmd);
        let has_token = envs.iter().any(|(k, _)| k == "ANTHROPIC_AUTH_TOKEN");
        let has_path = envs
            .iter()
            .any(|(k, v)| k == "PATH" && v == "/usr/bin:/bin");
        unsafe {
            std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
        }
        assert!(!has_token, "strict mode must drop credentials");
        assert!(has_path, "strict mode preserves PATH only");
    }
}
