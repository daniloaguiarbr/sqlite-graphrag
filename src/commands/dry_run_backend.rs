//! v1.0.84 (ADR-0042 / GAP-002): resolve and emit the LLM backend that
//! WOULD be invoked for embedding without actually spawning the
//! subprocess. Used by `--dry-run-backend` for CI audit and pre-flight
//! sanity-check of `--llm-backend` before long ingestion sessions.
//!
//! The output is a compact JSON envelope on stdout. stderr carries the
//! human-friendly summary so operators can run `sqlite-graphrag --dry-run-backend`
//! without piping through `jaq`.
//!
//! ## Schema (`dry-run-backend.schema.json`)
//!
//! ```json
//! {
//!   "action": "dry_run_backend",
//!   "backend": "codex|claude|none",
//!   "binary": "/usr/local/bin/codex",
//!   "model": "gpt-5.5",
//!   "flavour": "codex|claude",
//!   "chain": "claude",
//!   "strict_env_clear": false
//! }
//! ```
//!
//! ## Implementation notes
//!
//! - We deliberately do NOT depend on the private fields of
//!   `LlmEmbedding`. The struct's `binary` and `flavour` fields are
//!   private to `crate::extract::llm_embedding`, so we re-probe the
//!   PATH here (cheap, idempotent) instead of forcing the core to add
//!   `pub(crate)` getters just for this audit path.
//! - `model` comes from `LlmEmbedding::model_label()` which already
//!   exposes a stable public string of the form `<flavour>:<model>`.
//!   We strip the `<flavour>:` prefix to keep the schema flat.
//! - When `--llm-backend none` is selected the envelope still emits
//!   the same shape with empty `binary` and `model`, so downstream
//!   pipelines can parse a single schema unconditionally.

use crate::cli::{Cli, LlmBackendChoice};
use crate::errors::AppError;
use crate::extract::llm_embedding::LlmEmbedding;
use crate::output::emit_json_compact;
use crate::spawn::env_whitelist::is_strict_env_clear;
use serde::Serialize;

/// Compact JSON envelope emitted by `--dry-run-backend`.
///
/// Field order matches the documented schema. `chain` reflects
/// `--llm-fallback` so operators can audit the fallback order without
/// spawning `embedder::embed_with_fallback`.
#[derive(Serialize)]
pub struct DryRunBackendOutput {
    pub action: &'static str,
    pub backend: &'static str,
    pub binary: String,
    pub model: String,
    pub flavour: &'static str,
    pub chain: String,
    pub strict_env_clear: bool,
}

/// Resolve the LLM backend that would be used for embedding and emit
/// the JSON envelope. Returns `Err(AppError::Embedding)` when the
/// requested backend CLI is missing from PATH.
pub fn emit_dry_run_backend(cli: &Cli) -> Result<(), AppError> {
    let payload = match cli.llm_backend {
        LlmBackendChoice::None => DryRunBackendOutput {
            action: "dry_run_backend",
            backend: "none",
            binary: String::new(),
            model: String::new(),
            flavour: "none",
            chain: cli.llm_fallback.clone(),
            strict_env_clear: is_strict_env_clear(),
        },
        LlmBackendChoice::Auto => {
            // ADR-0038: codex is preferred; claude is the fallback when codex
            // is absent. Mirrors `LlmEmbedding::detect_available()` exactly
            // so the audit output never disagrees with the real spawn path.
            let resolved = LlmEmbedding::detect_available()?;
            backend_payload(&resolved, "codex-first-then-claude", cli, true)
        }
        LlmBackendChoice::Codex => {
            let resolved = LlmEmbedding::detect_available()?;
            let flavour = resolved.model_label();
            // Guard: the user explicitly asked for codex. If detect_available
            // returned a claude-backed client (no codex on PATH), we MUST
            // surface that as an error rather than silently substitute.
            // v1.0.84 (ADR-0042): claude must NOT silently replace codex
            // when the user opts in via `--llm-backend codex`.
            if flavour.starts_with("claude:") {
                return Err(AppError::Embedding(
                    "`--llm-backend codex` requested but `codex` was not found on PATH \
                     (a `claude` binary was detected; refusing silent fallback per ADR-0042). \
                     Install `codex` (>= 0.130) or pass `--llm-backend claude` explicitly."
                        .to_string(),
                ));
            }
            backend_payload(&resolved, "codex-explicit", cli, false)
        }
        LlmBackendChoice::Claude => {
            let resolved = LlmEmbedding::detect_available()?;
            let flavour = resolved.model_label();
            // Symmetric guard for `--llm-backend claude`.
            if flavour.starts_with("codex:") {
                return Err(AppError::Embedding(
                    "`--llm-backend claude` requested but `claude` was not found on PATH \
                     (a `codex` binary was detected; refusing silent fallback per ADR-0042). \
                     Install `claude` (Claude Code >= 2.1) or pass `--llm-backend codex` explicitly."
                        .to_string(),
                ));
            }
            backend_payload(&resolved, "claude-explicit", cli, false)
        }
        LlmBackendChoice::Opencode => {
            let resolved = LlmEmbedding::detect_available()?;
            let flavour = resolved.model_label();
            if !flavour.starts_with("opencode:") {
                let hint = if flavour.starts_with("codex:") || flavour.starts_with("claude:") {
                    format!(
                        "`--llm-backend opencode` requested but auto-detect resolved `{flavour}` \
                         (opencode has lower priority than codex/claude in detect_available). \
                         Pass `--llm-backend auto` or set SQLITE_GRAPHRAG_OPENCODE_BINARY explicitly."
                    )
                } else {
                    "`--llm-backend opencode` requested but `opencode` was not found on PATH. \
                     Install `opencode` (>= 1.17) or pass `--llm-backend auto` to auto-detect."
                        .to_string()
                };
                return Err(AppError::Embedding(hint));
            }
            backend_payload(&resolved, "opencode-explicit", cli, false)
        }
    };

    emit_json_compact(&payload)?;
    Ok(())
}

/// Build the envelope from a successfully-resolved `LlmEmbedding`.
///
/// `chain_label` documents which CLI knob produced this payload
/// (e.g. `codex-explicit` vs `codex-first-then-claude`) so the audit
/// output is self-describing.
fn backend_payload(
    resolved: &LlmEmbedding,
    chain_label: &str,
    cli: &Cli,
    is_auto: bool,
) -> DryRunBackendOutput {
    // `model_label()` returns `<flavour>:<model>` — split on the FIRST
    // colon so model names with colons (rare but possible) survive.
    // `flavour` must be a `&'static str` (the struct field type), so we
    // leak the slice into a `Box<str>` to obtain a `'static` reference.
    let label = resolved.model_label();
    let (flavour, model) = match label.split_once(':') {
        Some((f, m)) => (f, m.to_string()),
        None => ("unknown", label.to_string()),
    };
    let flavour: &'static str = Box::leak(flavour.to_string().into_boxed_str());

    // Re-probe PATH to surface the binary path the audit envelope
    // promises. We prefer `which::which` over the private `LlmEmbedding`
    // field so this file compiles independently of the `extract`
    // module's internal layout. The result is canonicalized when
    // possible so symlinks and shim wrappers don't leak location.
    let binary = which::which(if is_auto {
        // For Auto, prefer whichever the real spawn would pick first.
        if which::which("codex").is_ok() {
            "codex"
        } else {
            "claude"
        }
    } else {
        flavour
    })
    .ok()
    .and_then(|p| std::fs::canonicalize(&p).ok().or(Some(p)))
    .map(|p| p.display().to_string())
    .unwrap_or_default();

    // Backend string is the `LlmBackendChoice` name for clarity in CI
    // logs (operators filter on `backend == "codex"` etc.).
    let backend = match cli.llm_backend {
        LlmBackendChoice::Auto => {
            if flavour == "codex" {
                "codex"
            } else if flavour == "opencode" {
                "opencode"
            } else {
                "claude"
            }
        }
        LlmBackendChoice::Codex => "codex",
        LlmBackendChoice::Claude => "claude",
        LlmBackendChoice::Opencode => "opencode",
        LlmBackendChoice::None => "none",
    };

    DryRunBackendOutput {
        action: "dry_run_backend",
        backend,
        binary,
        model,
        flavour,
        chain: if chain_label == "codex-first-then-claude" {
            cli.llm_fallback.clone()
        } else {
            chain_label.to_string()
        },
        strict_env_clear: is_strict_env_clear(),
    }
}
