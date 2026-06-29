//! GAP-005 (v1.0.82): mapping of LLM subprocess exit codes to actionable diagnostics.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Immutable table of known exit codes mapped to actionable suggestions.
pub static EXIT_CODE_HINTS: OnceLock<HashMap<i32, &'static str>> = OnceLock::new();

fn exit_code_hints_map() -> &'static HashMap<i32, &'static str> {
    EXIT_CODE_HINTS.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert(1, "subprocesso retornou erro genérico; verificar logs em ~/.local/share/sqlite-graphrag/llm-backend.log");
        m.insert(2, "uso incorreto do CLI do subprocesso; rever flags passadas");
        m.insert(101, "SIGABRT do kernel; possível panic no código do subprocesso");
        m.insert(126, "binary não executável; executar chmod +x no binário");
        m.insert(127, "binary não encontrado no PATH; verificar which codex ou which claude");
        m.insert(134, "SIGABRT; abort interno do subprocesso — reportar bug upstream");
        m.insert(137, "SIGKILL do OOM killer ou externo; verificar dmesg | grep -i kill e reduzir --llm-parallelism");
        m.insert(139, "SIGSEGV; reportar bug upstream com stderr preservado");
        m.insert(143, "SIGTERM externo; hook PreToolUse ou timeout cascateou");
        m
    })
}

/// Returns an actionable diagnostic based on the exit code.
pub fn diagnose_exit_code(code: Option<i32>, signal: Option<i32>) -> String {
    if let Some(sig) = signal {
        return match sig {
            2 => "SIGINT recebido; usuário cancelou operação".to_string(),
            9 => "SIGKILL externo; OOM killer do kernel".to_string(),
            15 => "SIGTERM externo; hook PreToolUse ou timeout cascateou".to_string(),
            other => format!("signal Unix {other} não mapeado; consultar `kill -l`"),
        };
    }
    let code = code.unwrap_or(-1);
    exit_code_hints_map()
        .get(&code)
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            format!("exit code {code} desconhecido; consultar upstream docs do binary")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oom_killer_hint_contains_oom() {
        let hint = diagnose_exit_code(Some(137), None);
        assert!(hint.contains("OOM"), "expected OOM in: {hint}");
    }

    #[test]
    fn not_found_hint_contains_path() {
        let hint = diagnose_exit_code(Some(127), None);
        assert!(hint.contains("PATH"), "expected PATH in: {hint}");
    }

    #[test]
    fn sigterm_signal_hint() {
        let hint = diagnose_exit_code(None, Some(15));
        assert!(hint.contains("SIGTERM"), "expected SIGTERM in: {hint}");
    }

    #[test]
    fn unknown_code_returns_generic() {
        let hint = diagnose_exit_code(Some(42), None);
        assert!(hint.contains("42"), "expected 42 in: {hint}");
    }

    #[test]
    fn nine_exit_codes_mapped() {
        assert_eq!(exit_code_hints_map().len(), 9);
    }
}

// =============================================================================
// v1.0.82 (GAP-005): LlmBackendError — diagnostic error for LLM subprocess
// failures with captured stderr/stdout tails and an actionable hint.
// =============================================================================

/// Maximum number of bytes captured from each subprocess stream (stdout
/// and stderr) for the diagnostic tail. 1 KiB matches the limit used by
/// `tracing::log` macros and keeps the JSON envelope under 4 KiB.
pub const DIAG_TAIL_BYTES: usize = 1024;

/// Structured error for an LLM subprocess invocation that failed.
///
/// Each variant carries the information needed to diagnose the failure
/// WITHOUT re-running the subprocess: the binary, the exit code, and
/// a truncated tail of stdout/stderr so the operator can see WHY the
/// call failed (rate limit, OAuth, OOM, segfault, missing binary, ...).
///
/// Distinct from `AppError::Embedding(String)` (the legacy v1.0.81
/// shape) so the call sites can match on the failure category
/// programmatically instead of parsing the message string. The
/// `Display` impl preserves the legacy string format for back-compat
/// with `tracing` consumers and the i18n layer.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LlmBackendError {
    /// Subprocess exited with a non-zero status. The `hint` is
    /// looked up from [`EXIT_CODE_HINTS`] and tells the operator
    /// what to do next (re-auth, reduce parallelism, report upstream,
    /// etc.).
    NonZeroExit {
        /// Process exit code (`None` if killed by a signal).
        exit_code: Option<i32>,
        /// Unix signal that killed the process (2 = SIGINT, 15 = SIGTERM,
        /// 9 = SIGKILL). `None` when the process exited normally.
        signal: Option<i32>,
        /// Last 1 KiB of the subprocess stdout, UTF-8 lossy-decoded.
        stdout_tail: String,
        /// Last 1 KiB of the subprocess stderr, UTF-8 lossy-decoded.
        stderr_tail: String,
        /// Path of the binary that was spawned (e.g. `/usr/bin/codex`).
        binary: String,
        /// Human-readable diagnostic from [`EXIT_CODE_HINTS`].
        hint: String,
    },
    /// Subprocess could not be spawned at all (binary missing, no exec
    /// permission, or the OS refused to fork). Distinct from
    /// `NonZeroExit` so call sites can branch on "never started" vs
    /// "started and crashed".
    SpawnFailed {
        /// Path of the binary that was supposed to be spawned.
        binary: String,
        /// Underlying `io::Error` message (e.g. "No such file or directory").
        source: String,
    },
    /// Subprocess exceeded the per-call timeout (default 300s,
    /// override via `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS`).
    Timeout {
        /// Configured timeout in seconds.
        secs: u64,
        /// Path of the binary that was running when the timeout fired.
        binary: String,
    },
    /// All backends in the fallback chain failed AND no fallback was
    /// available. The call site should honour `--skip-embedding-on-failure`
    /// to write a `pending_embeddings` row instead of propagating this.
    NoBackendsAvailable,
}

impl LlmBackendError {
    /// Returns the human-readable diagnostic for this error.
    pub fn hint(&self) -> String {
        match self {
            Self::NonZeroExit { hint, .. } => hint.clone(),
            Self::SpawnFailed { binary, source } => {
                format!(
                    "spawn of '{binary}' failed: {source}; check that the binary exists, is executable, and required env vars (PATH, HOME, ...) are set"
                )
            }
            Self::Timeout { secs, binary } => {
                format!(
                    "subprocess '{binary}' exceeded the {secs}s timeout; \
                     override via SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS"
                )
            }
            Self::NoBackendsAvailable => "no backends succeeded and no fallback was configured; \
                 pass --llm-fallback=codex,claude or --skip-embedding-on-failure"
                .to_string(),
        }
    }

    /// Truncates the tail of a UTF-8 string to `max_bytes`, breaking
    /// on a char boundary so the result is always valid UTF-8.
    pub fn truncate_tail(raw: &[u8], max_bytes: usize) -> String {
        if raw.len() <= max_bytes {
            return String::from_utf8_lossy(raw).into_owned();
        }
        // Find the last char boundary at or before `max_bytes`.
        // `[u8]::is_char_boundary` is only on `str`, not `[u8]`, so we
        // hand-roll the boundary check: a UTF-8 continuation byte has
        // its top 2 bits set to 10, while a boundary byte has 0xxxxxxx
        // (ASCII) or 11xxxxxx (start of multi-byte).
        let mut cut = max_bytes.min(raw.len());
        while cut > 0 && (raw[cut] >= 0x80 && raw[cut] < 0xC0) {
            cut -= 1;
        }
        let mut s = String::from_utf8_lossy(&raw[..cut]).into_owned();
        s.push_str("...[truncated]");
        s
    }
}

impl std::fmt::Display for LlmBackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonZeroExit {
                exit_code,
                signal,
                stdout_tail,
                stderr_tail,
                binary,
                ..
            } => {
                let code_repr = match (exit_code, signal) {
                    (Some(c), _) => format!("exit {c}"),
                    (None, Some(s)) => format!("signal {s}"),
                    _ => "unknown status".to_string(),
                };
                write!(
                    f,
                    "{binary} subprocess failed: {code_repr}; \
                     stdout_tail={stdout_tail:?}; stderr_tail={stderr_tail:?}"
                )
            }
            Self::SpawnFailed { binary, source } => {
                write!(f, "{binary} spawn failed: {source}")
            }
            Self::Timeout { secs, binary } => {
                write!(f, "{binary} timed out after {secs}s")
            }
            Self::NoBackendsAvailable => {
                write!(f, "no LLM backends available; fallback chain exhausted")
            }
        }
    }
}

impl std::error::Error for LlmBackendError {}

/// Converts an `LlmBackendError` to a legacy `AppError::Embedding(String)`
/// so call sites that still return the old shape keep compiling during
/// the migration window. Once all call sites are migrated to return
/// `LlmBackendError` directly, this helper can be deleted.
pub fn into_legacy_embedding(err: &LlmBackendError) -> crate::errors::AppError {
    crate::errors::AppError::Embedding(err.to_string())
}

#[cfg(test)]
mod llm_backend_error_tests {
    use super::*;

    #[test]
    fn truncate_tail_short_returns_input() {
        let s = LlmBackendError::truncate_tail(b"hello", 1024);
        assert_eq!(s, "hello");
    }

    #[test]
    fn truncate_tail_long_appends_marker() {
        let raw = vec![b'a'; 2048];
        let s = LlmBackendError::truncate_tail(&raw, 1024);
        assert!(s.ends_with("...[truncated]"));
        // The prefix must be the original bytes up to the cut point.
        assert!(s.starts_with(&"a".repeat(1024)));
    }

    #[test]
    fn truncate_tail_respects_utf8_boundary() {
        // 600 'é' chars = 1200 bytes; cut at 1023 (odd byte inside a 2-byte
        // UTF-8 sequence) must back off to 1022 (boundary). 4-byte emoji
        // would also be handled: 256 emoji = 1024 bytes, cut at 1023 must
        // back off to 1020 (emoji = 4 bytes each, 1020 is boundary).
        let raw = "é".repeat(600).into_bytes(); // 2 bytes per char
        let s = LlmBackendError::truncate_tail(&raw, 1023);
        assert_eq!(s.len(), 1022 + "...[truncated]".len());
        assert!(s.ends_with("...[truncated]"));
        // Confirm the cut is a valid UTF-8 boundary by re-decoding the
        // first part (up to the cut marker).
        let cut = s.trim_end_matches("...[truncated]").len();
        let prefix = &s[..cut];
        assert!(std::str::from_utf8(prefix.as_bytes()).is_ok());
    }

    #[test]
    fn no_backends_hint_mentions_fallback() {
        let err = LlmBackendError::NoBackendsAvailable;
        assert!(err.hint().contains("--llm-fallback"));
    }

    #[test]
    fn spawn_failed_hint_mentions_binary() {
        let err = LlmBackendError::SpawnFailed {
            binary: "claude".into(),
            source: "No such file or directory".into(),
        };
        let h = err.hint();
        assert!(h.contains("claude"));
        assert!(h.contains("No such file or directory"));
    }

    #[test]
    fn timeout_hint_mentions_env_var() {
        let err = LlmBackendError::Timeout {
            secs: 300,
            binary: "codex".into(),
        };
        assert!(err.hint().contains("SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS"));
    }

    #[test]
    fn non_zero_exit_display_includes_stderr_tail() {
        let err = LlmBackendError::NonZeroExit {
            exit_code: Some(1),
            signal: None,
            stdout_tail: "out-1k".into(),
            stderr_tail: "err-1k".into(),
            binary: "codex".into(),
            hint: "diagnostic".into(),
        };
        let s = err.to_string();
        assert!(s.contains("codex"));
        assert!(s.contains("exit 1"));
        assert!(s.contains("err-1k"));
    }
}
