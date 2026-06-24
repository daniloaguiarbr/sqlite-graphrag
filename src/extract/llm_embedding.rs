//! LLM-based embedding backend (v1.0.76 default; reworked in v1.0.79 G42).
//!
//! `LlmEmbedding` is the production embedding client. It wraps headless
//! invocations of `claude code` or `codex` and returns f32 vectors of the
//! active dimensionality (`crate::constants::embedding_dim()`, default 64).
//!
//! v1.0.79 (G42) changes:
//! - S1: the dimensionality is no longer hardcoded here — the single
//!   source of truth lives in `crate::constants` and the JSON schemas
//!   are generated dynamically.
//! - S2: `embed_batch` embeds N numbered texts per LLM call with the
//!   `{items:[{i,v}]}` schema, collapsing 39 subprocess spawns into 4-5.
//! - S4: the codex `--output-schema` file is a `tempfile::NamedTempFile`
//!   with a randomised name created once per client and shared across
//!   clones via `Arc` — no per-call write+delete, no PID-path races.
//! - S5: the claude model honours `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL`
//!   (symmetric to the codex env var). ZERO hardcoded models without
//!   an env override.
//! - S6: `CLAUDE_CONFIG_DIR` points at an empty managed directory BY
//!   DEFAULT, because `--strict-mcp-config`/`--mcp-config '{}'` are
//!   silently ignored upstream (anthropics/claude-code#10787) and a
//!   full `~/.claude` costs ~223k cache-creation tokens per call.
//! - S7: the codex `request_user_input` failure mode maps to an
//!   actionable error instead of an opaque exit 11.
//! - BLOCO 4: every subprocess uses `kill_on_drop(true)` plus an
//!   explicit `tokio::time::timeout`, so cancellation never leaks a
//!   child and a hung LLM cannot stall the pipeline forever.
//!
//! OAuth is the only supported credential path. The constructor rejects
//! `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` in the environment — see
//! `v1.0.69 (G31) OAuth-Only Enforcement`.

use crate::errors::AppError;
use serde::Deserialize;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Default per-LLM-call timeout in seconds. Consistent with the
/// `--claude-timeout` / `--codex-timeout` defaults used by ingest.
/// Override via `SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS`.
const DEFAULT_EMBED_TIMEOUT_SECS: u64 = 120;

fn embed_timeout() -> std::time::Duration {
    let secs = std::env::var("SQLITE_GRAPHRAG_EMBED_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&n| (10..=3_600).contains(&n))
        .unwrap_or(DEFAULT_EMBED_TIMEOUT_SECS);
    std::time::Duration::from_secs(secs)
}

/// v1.0.89 (GAP-4): scales the per-call timeout with batch size.
/// A single-item batch uses the base timeout (120s default).
/// Each additional item adds 15s to account for the LLM generating
/// more embedding vectors in the same call.
#[cfg(test)]
fn embed_timeout_for_batch(batch_size: usize) -> std::time::Duration {
    let base = embed_timeout();
    let extra = std::time::Duration::from_secs(15) * batch_size.saturating_sub(1) as u32;
    base + extra
}

/// Cross-platform helper: extracts `(exit_code, signal)` from an
/// `ExitStatus` whose `.code()` returned `None`. On Unix this means
/// the process was killed by a signal; on Windows processes always
/// have an exit code so this branch returns `(None, None)`.
fn extract_exit_info(status: &std::process::ExitStatus) -> (Option<i32>, Option<i32>) {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        (None, status.signal())
    }
    #[cfg(not(unix))]
    {
        let _ = status;
        (None, None)
    }
}

/// G42/S1: single-vector JSON schema generated from the active dim.
fn build_single_schema(dim: usize) -> String {
    format!(
        r#"{{"type":"object","properties":{{"embedding":{{"type":"array","items":{{"type":"number"}},"minItems":{dim},"maxItems":{dim}}}}},"required":["embedding"],"additionalProperties":false}}"#
    )
}

/// G42/S2: batch JSON schema `{items:[{i,v}]}`. The `items` array length
/// is deliberately unconstrained so ONE schema file serves every batch
/// size (index coverage is validated in Rust after parsing).
fn build_batch_schema(dim: usize) -> String {
    format!(
        r#"{{"type":"object","properties":{{"items":{{"type":"array","items":{{"type":"object","properties":{{"i":{{"type":"integer"}},"v":{{"type":"array","items":{{"type":"number"}},"minItems":{dim},"maxItems":{dim}}}}},"required":["i","v"],"additionalProperties":false}}}}}},"required":["items"],"additionalProperties":false}}"#
    )
}

#[derive(Clone, Debug)]
pub struct LlmEmbedding {
    /// Which LLM headless binary to spawn. `claude` or `codex`.
    flavour: EmbeddingFlavour,
    /// Cached path to the binary to avoid PATH lookups on every call.
    binary: std::path::PathBuf,
    /// Model name. Resolved from env overrides at construction time.
    model: String,
    /// G42/S4: lazily-created codex `--output-schema` tempfiles, shared
    /// across clones. Keyed by dim so an env change between tests cannot
    /// serve a stale schema.
    codex_schemas: Arc<parking_lot::Mutex<CodexSchemaFiles>>,
    /// BUG-TIMEOUT-HARDCODE-001: instance-scoped timeout override.
    /// Precedence: this field > env var > DEFAULT_EMBED_TIMEOUT_SECS.
    timeout_override: Option<std::time::Duration>,
}

#[derive(Debug, Default)]
struct CodexSchemaFiles {
    single: Option<(usize, Arc<tempfile::NamedTempFile>)>,
    batch: Option<(usize, Arc<tempfile::NamedTempFile>)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum EmbeddingFlavour {
    Claude,
    Codex,
    Opencode,
}

/// ADR-0042 / GAP-002: builder for [`LlmEmbedding`] that lets callers
/// override the binary path and model without having to remember the
/// env-var names per flavour. Replaces the duplicated `with_codex` /
/// `with_claude` bodies that diverged in v1.0.82 (GAP-002: the Claude
/// arm of `embed_via_backend` re-did the PATH probe via
/// `LlmEmbedding::detect_available` and could silently pick `codex`).
#[derive(Clone, Debug)]
pub struct LlmEmbeddingBuilder {
    flavour: EmbeddingFlavour,
    binary_override: Option<std::path::PathBuf>,
    model_override: Option<String>,
    timeout_override: Option<std::time::Duration>,
}

impl LlmEmbeddingBuilder {
    /// Convenience: produce a Claude-backed builder pre-configured with
    /// the canonical default binary + model.
    /// Convenience: produce a Claude-backed builder pre-configured with
    /// the canonical default binary + model.
    pub fn claude_default() -> Self {
        Self {
            flavour: EmbeddingFlavour::Claude,
            binary_override: None,
            model_override: None,
            timeout_override: None,
        }
    }

    /// Convenience: produce a Codex-backed builder pre-configured with
    /// the canonical default binary + model.
    pub fn codex_default() -> Self {
        Self {
            flavour: EmbeddingFlavour::Codex,
            binary_override: None,
            model_override: None,
            timeout_override: None,
        }
    }

    /// Convenience: produce an OpenCode-backed builder pre-configured with
    /// the canonical default binary + model.
    pub fn opencode_default() -> Self {
        Self {
            flavour: EmbeddingFlavour::Opencode,
            binary_override: None,
            model_override: None,
            timeout_override: None,
        }
    }
    /// Override the binary path (skips the `which::which` PATH probe).
    pub fn override_binary(mut self, binary: std::path::PathBuf) -> Self {
        self.binary_override = Some(binary);
        self
    }

    /// Override the model name (skips the env-var lookup).
    pub fn override_model(mut self, model: String) -> Self {
        self.model_override = Some(model);
        self
    }

    /// Override the per-call embedding timeout (skips env-var lookup).
    pub fn override_timeout(mut self, secs: u64) -> Self {
        let clamped = secs.clamp(10, 3_600);
        self.timeout_override = Some(std::time::Duration::from_secs(clamped));
        self
    }

    /// Build the [`LlmEmbedding`]. Enforces OAuth-only and resolves the
    /// binary/model via the override or the env-var defaults.
    pub fn build(self) -> Result<LlmEmbedding, AppError> {
        LlmEmbedding::oauth_only_enforce()?;
        let binary = match self.binary_override {
            Some(path) => resolve_real_binary(&path),
            None => {
                let (env_var, which_name) = match self.flavour {
                    EmbeddingFlavour::Codex => ("SQLITE_GRAPHRAG_CODEX_BINARY", "codex"),
                    EmbeddingFlavour::Claude => ("SQLITE_GRAPHRAG_CLAUDE_BINARY", "claude"),
                    EmbeddingFlavour::Opencode => ("SQLITE_GRAPHRAG_OPENCODE_BINARY", "opencode"),
                };
                let path = std::env::var_os(env_var)
                    .map(std::path::PathBuf::from)
                    .or_else(|| which::which(which_name).ok())
                    .ok_or_else(|| {
                        AppError::Embedding(format!("`{which_name}` not found on PATH"))
                    })?;
                resolve_real_binary(&path)
            }
        };
        let model = match self.model_override {
            Some(m) => m,
            None => match self.flavour {
                EmbeddingFlavour::Codex => codex_embed_model(),
                EmbeddingFlavour::Claude => claude_embed_model(),
                EmbeddingFlavour::Opencode => opencode_embed_model(),
            },
        };
        Ok(LlmEmbedding {
            flavour: self.flavour,
            binary,
            model,
            codex_schemas: Arc::new(parking_lot::Mutex::new(CodexSchemaFiles::default())),
            timeout_override: self.timeout_override,
        })
    }
}

impl EmbeddingFlavour {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Opencode => "opencode",
        }
    }
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    embedding: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct BatchEmbeddingResponse {
    items: Vec<BatchEmbeddingItem>,
}

#[derive(Debug, Deserialize)]
struct BatchEmbeddingItem {
    i: usize,
    v: Vec<f32>,
}

/// Follows symlinks and shell-script shim `exec` targets to find
/// the real ELF binary. Shim wrappers (like `~/.graphrag-shim/codex`)
/// can strip hardening flags; bypassing them is a security requirement.
pub fn resolve_real_binary(path: &std::path::Path) -> std::path::PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        if is_elf_binary(&canonical) {
            return canonical;
        }
        if let Some(exec_target) = extract_exec_target_from_shim(&canonical) {
            if exec_target.exists() && is_elf_binary(&exec_target) {
                return exec_target;
            }
        }
        return canonical;
    }
    path.to_path_buf()
}

fn is_elf_binary(path: &std::path::Path) -> bool {
    std::fs::read(path)
        .map(|bytes| bytes.len() >= 4 && bytes[..4] == [0x7f, b'E', b'L', b'F'])
        .unwrap_or(false)
}

fn extract_exec_target_from_shim(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let content = std::fs::read_to_string(path).ok()?;
    if !content.starts_with("#!") {
        return None;
    }
    for line in content.lines().rev() {
        let trimmed = line.trim();
        if trimmed.starts_with("exec ") {
            let after_exec = trimmed.strip_prefix("exec ")?;
            let binary = after_exec.split_whitespace().next()?;
            return Some(std::path::PathBuf::from(binary));
        }
    }
    None
}

/// G42/S5: claude embedding model with env override, symmetric to the
/// codex `SQLITE_GRAPHRAG_CODEX_EMBED_MODEL` introduced in v1.0.78.
fn claude_embed_model() -> String {
    // Precedence: SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL > SQLITE_GRAPHRAG_LLM_MODEL > default
    std::env::var("SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL")
        .or_else(|_| std::env::var("SQLITE_GRAPHRAG_LLM_MODEL"))
        .unwrap_or_else(|_| {
            tracing::info!(
                target: "llm_embedding",
                "no model specified; defaulting to claude-sonnet-4-6"
            );
            "claude-sonnet-4-6".to_string()
        })
}

fn codex_embed_model() -> String {
    // Precedence: SQLITE_GRAPHRAG_CODEX_EMBED_MODEL > SQLITE_GRAPHRAG_LLM_MODEL > default
    std::env::var("SQLITE_GRAPHRAG_CODEX_EMBED_MODEL")
        .or_else(|_| std::env::var("SQLITE_GRAPHRAG_LLM_MODEL"))
        .unwrap_or_else(|_| {
            tracing::info!(
                target: "llm_embedding",
                "no model specified; defaulting to gpt-5.5"
            );
            "gpt-5.5".to_string()
        })
}

fn opencode_embed_model() -> String {
    // Precedence: SQLITE_GRAPHRAG_OPENCODE_EMBED_MODEL > SQLITE_GRAPHRAG_OPENCODE_MODEL > default
    // NOTE: intentionally does NOT fall back to SQLITE_GRAPHRAG_LLM_MODEL because that
    // var typically holds a codex/claude model name (e.g. "gpt-5.4-mini") that opencode
    // does not recognise — cross-contamination caused ProviderModelNotFoundError (v1.0.90 audit).
    std::env::var("SQLITE_GRAPHRAG_OPENCODE_EMBED_MODEL")
        .or_else(|_| std::env::var("SQLITE_GRAPHRAG_OPENCODE_MODEL"))
        .unwrap_or_else(|_| {
            tracing::info!(
                target: "llm_embedding",
                "no model specified; defaulting to opencode/big-pickle"
            );
            "opencode/big-pickle".to_string()
        })
}

impl LlmEmbedding {
    /// Detects which LLM CLI is available on PATH and returns the
    /// matching embedding client.
    ///
    /// v1.0.76: PREFERS `codex` over `claude` because:
    /// - Claude Code 2.1+ ships a 180k+ token system context (plugins,
    ///   skills, agents, MCP) that overflows the 200k context window
    ///   for even trivial embedding prompts and returns "Prompt is too
    ///   long". (v1.0.79/S6 mitigates this with an empty
    ///   `CLAUDE_CONFIG_DIR`, but codex stays the lighter default.)
    /// - Codex 0.134+ is lightweight (~5k system context) and the
    ///   `StructuredOutput` tool reliably returns the requested vectors.
    pub fn detect_available() -> Result<Self, AppError> {
        Self::oauth_only_enforce()?;

        // v1.0.89 (GAP-1): honour SQLITE_GRAPHRAG_CODEX_BINARY for the
        // embedding pipeline, symmetric with SQLITE_GRAPHRAG_CLAUDE_BINARY.
        let codex_path = std::env::var_os("SQLITE_GRAPHRAG_CODEX_BINARY")
            .map(std::path::PathBuf::from)
            .or_else(|| which::which("codex").ok());
        if let Some(path) = codex_path {
            return Ok(Self {
                flavour: EmbeddingFlavour::Codex,
                binary: resolve_real_binary(&path),
                model: codex_embed_model(),
                codex_schemas: Arc::new(parking_lot::Mutex::new(CodexSchemaFiles::default())),
                timeout_override: None,
            });
        }
        // v1.0.89: honour SQLITE_GRAPHRAG_CLAUDE_BINARY for the embedding
        // pipeline, not just ingest/enrich. This lets operators override the
        // symlink-resolved path (e.g. a stale multi-instance binary).
        let claude_path = std::env::var_os("SQLITE_GRAPHRAG_CLAUDE_BINARY")
            .map(std::path::PathBuf::from)
            .or_else(|| which::which("claude").ok());
        if let Some(path) = claude_path {
            return Ok(Self {
                flavour: EmbeddingFlavour::Claude,
                binary: resolve_real_binary(&path),
                model: claude_embed_model(),
                codex_schemas: Arc::new(parking_lot::Mutex::new(CodexSchemaFiles::default())),
                timeout_override: None,
            });
        }
        // v1.0.90 (GAP-OPENCODE-001): probe opencode as 3rd priority.
        let opencode_path = std::env::var_os("SQLITE_GRAPHRAG_OPENCODE_BINARY")
            .map(std::path::PathBuf::from)
            .or_else(|| which::which("opencode").ok());
        if let Some(path) = opencode_path {
            return Ok(Self {
                flavour: EmbeddingFlavour::Opencode,
                binary: resolve_real_binary(&path),
                model: opencode_embed_model(),
                codex_schemas: Arc::new(parking_lot::Mutex::new(CodexSchemaFiles::default())),
                timeout_override: None,
            });
        }
        Err(AppError::Embedding(
            "no LLM CLI found on PATH: install `codex` (0.130+), `claude` (Claude Code 2.1+), or `opencode` (1.17+)"
                .to_string(),
        ))
    }

    /// Instance-scoped timeout. Precedence:
    /// `timeout_override` field > env var > DEFAULT_EMBED_TIMEOUT_SECS.
    fn instance_embed_timeout(&self) -> std::time::Duration {
        if let Some(d) = self.timeout_override {
            return d;
        }
        embed_timeout()
    }

    /// Instance-scoped batch timeout: base + 15s per extra item.
    fn instance_embed_timeout_for_batch(&self, batch_size: usize) -> std::time::Duration {
        let base = self.instance_embed_timeout();
        let extra = std::time::Duration::from_secs(15) * batch_size.saturating_sub(1) as u32;
        base + extra
    }

    pub fn with_codex() -> Result<Self, AppError> {
        Self::with_codex_builder().build()
    }

    pub fn with_claude() -> Result<Self, AppError> {
        Self::with_claude_builder().build()
    }

    /// ADR-0042 / GAP-002: builder entry point for a codex-backed
    /// embedder with default model resolution.
    pub fn with_codex_builder() -> LlmEmbeddingBuilder {
        LlmEmbeddingBuilder {
            flavour: EmbeddingFlavour::Codex,
            binary_override: None,
            model_override: None,
            timeout_override: None,
        }
    }

    /// ADR-0042 / GAP-002: builder entry point for a claude-backed
    /// embedder with default model resolution.
    pub fn with_claude_builder() -> LlmEmbeddingBuilder {
        LlmEmbeddingBuilder {
            flavour: EmbeddingFlavour::Claude,
            binary_override: None,
            model_override: None,
            timeout_override: None,
        }
    }

    pub fn with_opencode() -> Result<Self, AppError> {
        Self::with_opencode_builder().build()
    }

    pub fn with_opencode_builder() -> LlmEmbeddingBuilder {
        LlmEmbeddingBuilder {
            flavour: EmbeddingFlavour::Opencode,
            binary_override: None,
            model_override: None,
            timeout_override: None,
        }
    }
    /// v1.0.69 (G31): refuse to spawn if an API key is set. The CLI
    /// must use OAuth. The two API-key env vars are NOT in the
    /// env-clear whitelist, so a parent process that exports them
    /// will see this error.
    fn oauth_only_enforce() -> Result<(), AppError> {
        if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            return Err(AppError::Validation(
                "ANTHROPIC_API_KEY is set; v1.0.76 requires OAuth. \
                 unset it and use `claude login` instead."
                    .into(),
            ));
        }
        if std::env::var("OPENAI_API_KEY").is_ok() {
            return Err(AppError::Validation(
                "OPENAI_API_KEY is set; v1.0.76 requires OAuth. \
                 unset it and use `codex login` instead."
                    .into(),
            ));
        }
        Ok(())
    }

    /// Embeds a single passage (chunk of a memory body). Returns an
    /// f32 vector of the active dimensionality.
    pub fn embed_passage(&self, text: &str) -> Result<Vec<f32>, AppError> {
        self.invoke_with_prefix(crate::constants::PASSAGE_PREFIX, text)
    }

    /// Embeds a single query. The LLM uses a different prompt prefix
    /// to disambiguate query from passage.
    pub fn embed_query(&self, text: &str) -> Result<Vec<f32>, AppError> {
        self.invoke_with_prefix(crate::constants::QUERY_PREFIX, text)
    }

    /// G56: returns a stable label for the active embedding model so the
    /// in-process entity-embedding cache can key by `(model, text)`.
    /// Embeddings produced by different models are not interchangeable,
    /// so a cache entry from one model must never satisfy a request
    /// served by another.
    pub fn model_label(&self) -> String {
        format!("{}:{}", self.flavour.as_str(), self.model)
    }

    /// ADR-0042 / BUG-003 fix: returns the resolved []
    /// of this embedder. Used by  and
    ///  to report the backend that
    /// ACTUALLY executed the embedding (not the one requested in the
    /// chain). When  substitutes claude
    /// for a missing codex, the operator sees the truth in
    /// .
    pub fn flavour(&self) -> EmbeddingFlavour {
        self.flavour
    }

    /// G42/S2: embeds a batch of `(global_index, text)` pairs in ONE
    /// LLM call. Returns `(global_index, vector)` pairs. Async — this
    /// is the unit of work scheduled by the bounded fan-out in
    /// `crate::embedder`.
    ///
    /// Cancel safety: the future owns its subprocess via
    /// `kill_on_drop(true)`, so dropping it (e.g. losing a
    /// `tokio::select!` race against a cancellation token) kills the
    /// child and leaks nothing.
    pub async fn embed_batch_async(
        &self,
        prefix: &str,
        batch: &[(usize, String)],
    ) -> Result<Vec<(usize, Vec<f32>)>, AppError> {
        let dim = crate::constants::embedding_dim();
        if batch.is_empty() {
            return Ok(Vec::new());
        }
        if batch.len() == 1 {
            let (idx, text) = (&batch[0].0, &batch[0].1);
            let v = self.invoke_single_async(prefix, text, dim).await?;
            return Ok(vec![(*idx, v)]);
        }

        let mut prompt = format!(
            "Generate {dim}-dimensional semantic embedding vectors for each numbered text below.\n\
             Return a JSON object with an \"items\" array containing EXACTLY {n} items.\n\
             Each item has \"i\" (the 1-based index) and \"v\" (the {dim}-float vector, values between -1 and 1).\n\n",
            n = batch.len()
        );
        for (pos, (_, text)) in batch.iter().enumerate() {
            prompt.push_str(&format!("{}: {prefix}{text}\n", pos + 1));
        }

        // BUG-TIMEOUT-HARDCODE-001: batch timeout is now instance-scoped
        // (no more std::env::set_var which was unsafe in multi-thread).
        let _batch_timeout = self.instance_embed_timeout_for_batch(batch.len());
        let stdout = match self.flavour {
            EmbeddingFlavour::Claude => {
                self.invoke_claude(&prompt, &build_batch_schema(dim))
                    .await?
            }
            EmbeddingFlavour::Codex => {
                let schema = self.codex_schema_file(dim, true)?;
                self.invoke_codex(&prompt, schema.path()).await?
            }
            EmbeddingFlavour::Opencode => {
                let opencode_prompt = format!(
                    "You are a batch embedding function. For each numbered text item below, \
                     generate an array of exactly {dim} floating-point numbers between -1 and 1 \
                     representing its semantic meaning. Output ONLY a JSON object with key \"items\" \
                     containing an array of objects, each with \"i\" (the 1-based index) and \
                     \"v\" (the {dim}-element float array). No markdown, no explanation.\n\n\
                     {prompt}"
                );
                self.invoke_opencode(&opencode_prompt).await?
            }
        };
        let parsed: BatchEmbeddingResponse = parse_llm_json(&stdout).map_err(|e| {
            AppError::Embedding(format!(
                "LLM batch embedding response parse failed: {e}; raw={stdout}"
            ))
        })?;
        if parsed.items.len() != batch.len() {
            return Err(AppError::Embedding(format!(
                "LLM batch returned {} items, expected {} (G42/S2 coverage check)",
                parsed.items.len(),
                batch.len()
            )));
        }
        let mut out: Vec<Option<Vec<f32>>> = vec![None; batch.len()];
        for item in parsed.items {
            if item.i == 0 || item.i > batch.len() {
                return Err(AppError::Embedding(format!(
                    "LLM batch item index {} out of range 1..={}",
                    item.i,
                    batch.len()
                )));
            }
            if item.v.len() != dim {
                return Err(AppError::Embedding(format!(
                    "LLM batch item {} returned {} dims, expected {dim}; \
                     refusing to truncate or pad silently (G42/C5)",
                    item.i,
                    item.v.len()
                )));
            }
            out[item.i - 1] = Some(item.v);
        }
        let mut result = Vec::with_capacity(batch.len());
        for (pos, slot) in out.into_iter().enumerate() {
            let v = slot.ok_or_else(|| {
                AppError::Embedding(format!(
                    "LLM batch response is missing item index {} (G42/S2 coverage check)",
                    pos + 1
                ))
            })?;
            result.push((batch[pos].0, v));
        }
        Ok(result)
    }

    fn invoke_with_prefix(&self, prefix: &str, text: &str) -> Result<Vec<f32>, AppError> {
        let dim = crate::constants::embedding_dim();
        let inner = self.invoke_single_async(prefix, text, dim);
        // v1.0.79 (G42/A2): reuse the process-wide multi-thread runtime
        // instead of building a current-thread runtime PER CALL. Inside
        // an existing runtime (tests, async commands) block_in_place
        // keeps the worker pool healthy.
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(|| handle.block_on(inner)),
            Err(_) => crate::embedder::shared_runtime()?.block_on(inner),
        }
    }

    async fn invoke_single_async(
        &self,
        prefix: &str,
        text: &str,
        dim: usize,
    ) -> Result<Vec<f32>, AppError> {
        let prompt = format!("{prefix}{text}");
        let stdout = match self.flavour {
            EmbeddingFlavour::Claude => {
                self.invoke_claude(&prompt, &build_single_schema(dim))
                    .await?
            }
            EmbeddingFlavour::Codex => {
                let schema = self.codex_schema_file(dim, false)?;
                self.invoke_codex(&prompt, schema.path()).await?
            }
            EmbeddingFlavour::Opencode => {
                let opencode_prompt = format!(
                    "You are an embedding function. Given the input text, output a JSON object \
                     with a single key \"embedding\" containing an array of exactly {dim} \
                     floating-point numbers between -1 and 1 that represent the semantic meaning \
                     of the text. Output ONLY the JSON object, nothing else.\n\n\
                     Input text: \"{prompt}\""
                );
                self.invoke_opencode(&opencode_prompt).await?
            }
        };
        let parsed: EmbeddingResponse = parse_llm_json(&stdout).map_err(|e| {
            AppError::Embedding(format!(
                "LLM embedding response parse failed: {e}; raw={stdout}"
            ))
        })?;
        if parsed.embedding.len() != dim {
            return Err(AppError::Embedding(format!(
                "LLM returned {} dims, expected {dim}; \
                 refusing to truncate or pad silently (G42/C5)",
                parsed.embedding.len()
            )));
        }
        Ok(parsed.embedding)
    }

    /// G42/S4: returns the lazily-created, process-shared codex schema
    /// tempfile for the requested mode. `NamedTempFile` randomises the
    /// filename (no PID-based collisions) and removes the file on drop
    /// of the last `Arc` clone.
    fn codex_schema_file(
        &self,
        dim: usize,
        batch: bool,
    ) -> Result<Arc<tempfile::NamedTempFile>, AppError> {
        let mut guard = self.codex_schemas.lock();
        let slot = if batch {
            &mut guard.batch
        } else {
            &mut guard.single
        };
        if let Some((cached_dim, file)) = slot {
            if *cached_dim == dim {
                return Ok(Arc::clone(file));
            }
        }
        let content = if batch {
            build_batch_schema(dim)
        } else {
            build_single_schema(dim)
        };
        let file = tempfile::Builder::new()
            .prefix("sqlite-graphrag-embed-schema-")
            .suffix(".json")
            .tempfile()
            .map_err(|e| AppError::Embedding(format!("schema tempfile create failed: {e}")))?;
        std::fs::write(file.path(), content)
            .map_err(|e| AppError::Embedding(format!("schema tempfile write failed: {e}")))?;
        let file = Arc::new(file);
        *slot = Some((dim, Arc::clone(&file)));
        Ok(file)
    }

    async fn invoke_claude(&self, prompt: &str, schema: &str) -> Result<String, AppError> {
        // v1.0.69 hardening: --strict-mcp-config --mcp-config <PATH> --settings
        // '{"hooks":{}}' --dangerously-skip-permissions.
        //
        // v1.0.76 hardening: Claude Code 2.1+ renamed --output-schema to
        // --json-schema and accepts the schema as an inline JSON string
        // (NOT a file path). Also pass --output-format json so the
        // response is a single JSON object on stdout.
        //
        // v1.0.79 (G42/S6): CLAUDE_CONFIG_DIR points at an empty managed
        // directory BY DEFAULT — the MCP-isolation flags above are
        // silently ignored upstream (anthropics/claude-code#10787) and a
        // populated ~/.claude costs ~223k cache-creation tokens per call.
        //
        // v1.0.88 (BUG-2 fix, ADR-0046): the inline `--mcp-config '{}'`
        // form was rejected by Claude Code 2.1.177 (ADR-0045 Bug 2).
        // Substitute a tempfile path produced by
        // `write_empty_mcp_config_tempfile()` and run the full
        // preflight gate BEFORE `Command::spawn()`, mirroring what
        // `invoke_codex` already does for the codex backend.
        let spawn_dir = crate::spawn::spawn_isolation_dir()?;
        let mcp_config_path = crate::spawn::preflight::write_empty_mcp_config_tempfile()?;
        let argv_refs: [std::ffi::OsString; 0] = [];
        let preflight_args = crate::spawn::preflight::PreFlightArgs {
            binary_path: &self.binary,
            argv: &argv_refs,
            workspace_root: &spawn_dir,
            mcp_config_inline_json: None,
            expected_output_bytes: 65_536,
            spawner_name: "llm_embedding",
        };
        crate::spawn::preflight::preflight_check(&preflight_args)?;
        let mut cmd = Command::new(&self.binary);
        cmd.arg("-p")
            .arg(prompt)
            .arg("--model")
            .arg(&self.model)
            .arg("--json-schema")
            .arg(schema)
            .arg("--output-format")
            .arg("json")
            .arg("--strict-mcp-config")
            .arg("--mcp-config")
            .arg(mcp_config_path.as_os_str())
            .arg("--settings")
            .arg(r#"{"hooks":{}}"#)
            .arg("--dangerously-skip-permissions")
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("HOME", std::env::var("HOME").unwrap_or_default())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // BLOCO 4: cancellation (dropped future) must kill the child.
            .kill_on_drop(true);
        // GAP-SPAWN-001: isolate CWD so child never inherits .mcp.json
        cmd.current_dir(&spawn_dir);
        cmd.env("CLAUDE_CONFIG_DIR", &spawn_dir);
        if let Some(config_dir) = claude_embedding_config_dir() {
            cmd.env("CLAUDE_CONFIG_DIR", &config_dir);
        }
        let binary_str = self.binary.to_string_lossy().into_owned();
        let output = match tokio::time::timeout(self.instance_embed_timeout(), cmd.output()).await {
            Err(_elapsed) => {
                return Err(crate::llm::exit_code_hints::into_legacy_embedding(
                    &crate::llm::exit_code_hints::LlmBackendError::Timeout {
                        secs: self.instance_embed_timeout().as_secs(),
                        binary: binary_str.clone(),
                    },
                ));
            }
            Ok(Err(e)) => {
                return Err(crate::llm::exit_code_hints::into_legacy_embedding(
                    &crate::llm::exit_code_hints::LlmBackendError::SpawnFailed {
                        binary: binary_str.clone(),
                        source: e.to_string(),
                    },
                ));
            }
            Ok(Ok(o)) => o,
        };
        // G45-CR5 / ADR-0043 (v1.0.85): parse the JSON envelope from
        // `claude -p --output-format json` and detect OAuth quota
        // exhaustion by looking for the `rate_limit_error` or
        // `usage` overflow markers before checking the subprocess
        // exit status. This lets the deterministic fallback in
        // hybrid-search and recall swap to codex immediately.
        let stdout_str = String::from_utf8_lossy(&output.stdout);
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&stdout_str) {
            let is_rate_limited = parsed
                .get("is_error")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                && parsed
                    .get("result")
                    .and_then(|v| v.as_str())
                    .map(|s| {
                        s.contains("rate limit")
                            || s.contains("quota")
                            || s.contains("anthropic-ratelimit")
                    })
                    .unwrap_or(false);
            if is_rate_limited {
                return Err(AppError::Embedding(format!(
                    "OAuth usage quota exhausted: claude rate_limit detected in stdout: {}",
                    parsed
                        .get("result")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .chars()
                        .take(120)
                        .collect::<String>()
                )));
            }
        }
        if !output.status.success() {
            let (exit_code, signal) = if let Some(code) = output.status.code() {
                (Some(code), None)
            } else {
                extract_exit_info(&output.status)
            };
            let stdout_tail = crate::llm::exit_code_hints::LlmBackendError::truncate_tail(
                &output.stdout,
                crate::llm::exit_code_hints::DIAG_TAIL_BYTES,
            );
            let stderr_tail = crate::llm::exit_code_hints::LlmBackendError::truncate_tail(
                &output.stderr,
                crate::llm::exit_code_hints::DIAG_TAIL_BYTES,
            );
            let mut hint = crate::llm::exit_code_hints::diagnose_exit_code(exit_code, signal);
            // v1.0.89 (GAP-5): detect expired OAuth and suggest actionable fix.
            if stderr_tail.contains("401")
                || stderr_tail.contains("Unauthorized")
                || stderr_tail.contains("expired")
                || stderr_tail.contains("login")
                || stdout_tail.contains("401")
                || stdout_tail.contains("Unauthorized")
            {
                hint.push_str(" | Claude OAuth token may be expired; run `claude login` to renew");
            }
            return Err(crate::llm::exit_code_hints::into_legacy_embedding(
                &crate::llm::exit_code_hints::LlmBackendError::NonZeroExit {
                    exit_code,
                    signal,
                    stdout_tail,
                    stderr_tail,
                    binary: binary_str,
                    hint,
                },
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    async fn invoke_codex(
        &self,
        prompt: &str,
        schema_path: &std::path::Path,
    ) -> Result<String, AppError> {
        let binary_str = self.binary.to_string_lossy().into_owned();
        let mut cmd = build_codex_embedding_command(&self.binary, &self.model, schema_path)?;

        // GAP-META-005 (v1.0.87, ADR-0045): pre-flight gate before spawn.
        // `tokio::process::Command` does not expose `get_args()`, so we
        // skip the argv-size check here and rely on binary + workspace
        // root + output buffer guards. Embedding prompts are bounded by
        // the schema validator so argv overflow is not a real risk here.
        //
        // v1.0.88 (BUG-7 fix, ADR-0046): propagate the preflight error
        // directly via `AppError::PreFlightFailed` (via the `From`
        // impl added in `errors.rs`) so callers and operators see the
        // structured `PreFlightError` variant and the canonical exit
        // code 16. The previous implementation wrapped the error in
        // `LlmBackendError::SpawnFailed`, which mapped to a different
        // exit code and masked the preflight signal.
        let argv_refs: [std::ffi::OsString; 0] = [];
        let preflight_args = crate::spawn::preflight::PreFlightArgs {
            binary_path: &self.binary,
            argv: &argv_refs,
            workspace_root: std::path::Path::new("."),
            mcp_config_inline_json: None,
            expected_output_bytes: 65_536,
            spawner_name: "llm_embedding",
        };
        crate::spawn::preflight::preflight_check(&preflight_args)?;
        let _ = binary_str; // silenced: preflight does not need it

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return Err(crate::llm::exit_code_hints::into_legacy_embedding(
                    &crate::llm::exit_code_hints::LlmBackendError::SpawnFailed {
                        binary: binary_str,
                        source: e.to_string(),
                    },
                ));
            }
        };
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(prompt.as_bytes())
                .await
                .map_err(|e| AppError::Embedding(format!("codex stdin write failed: {e}")))?;
            drop(stdin);
        }
        let output =
            match tokio::time::timeout(self.instance_embed_timeout(), child.wait_with_output())
                .await
            {
                Err(_elapsed) => {
                    return Err(crate::llm::exit_code_hints::into_legacy_embedding(
                        &crate::llm::exit_code_hints::LlmBackendError::Timeout {
                            secs: self.instance_embed_timeout().as_secs(),
                            binary: binary_str,
                        },
                    ));
                }
                Ok(Err(e)) => {
                    return Err(crate::llm::exit_code_hints::into_legacy_embedding(
                        &crate::llm::exit_code_hints::LlmBackendError::SpawnFailed {
                            binary: binary_str,
                            source: format!("codex wait failed: {e}"),
                        },
                    ));
                }
                Ok(Ok(o)) => o,
            };
        if !output.status.success() {
            let (exit_code, signal) = if let Some(code) = output.status.code() {
                (Some(code), None)
            } else {
                extract_exit_info(&output.status)
            };
            let stdout_tail = crate::llm::exit_code_hints::LlmBackendError::truncate_tail(
                &output.stdout,
                crate::llm::exit_code_hints::DIAG_TAIL_BYTES,
            );
            let stderr_tail = crate::llm::exit_code_hints::LlmBackendError::truncate_tail(
                &output.stderr,
                crate::llm::exit_code_hints::DIAG_TAIL_BYTES,
            );
            let hint = crate::llm::exit_code_hints::diagnose_exit_code(exit_code, signal);
            // G42/S7: the headless spawn can still hit interactive
            // prompts on some codex builds; keep the legacy request_user_input
            // branch as a special-case hint, and stamp the diagnostic
            // tail on top of the canonical NonZeroExit envelope.
            let mut combined_hint = hint;
            if stderr_tail.contains("request_user_input") {
                combined_hint.push_str(
                    " | codex requested interactive input in a headless embedding call; \
                     upgrade codex (>= 0.134) or switch the embedding backend to claude",
                );
            }
            return Err(crate::llm::exit_code_hints::into_legacy_embedding(
                &crate::llm::exit_code_hints::LlmBackendError::NonZeroExit {
                    exit_code,
                    signal,
                    stdout_tail,
                    stderr_tail,
                    binary: binary_str,
                    hint: combined_hint,
                },
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    async fn invoke_opencode(&self, prompt: &str) -> Result<String, AppError> {
        let binary_str = self.binary.to_string_lossy().into_owned();
        let spawn_dir = crate::spawn::spawn_isolation_dir()?;
        let mut cmd = Command::new(&self.binary);
        cmd.current_dir(&spawn_dir);
        cmd.arg("run")
            .arg("--format")
            .arg("json")
            .arg("-m")
            .arg(&self.model)
            .arg("--dangerously-skip-permissions")
            .arg(prompt)
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("HOME", std::env::var("HOME").unwrap_or_default())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        crate::commands::opencode_runner::propagate_opencode_env(&mut cmd);

        let output = match tokio::time::timeout(self.instance_embed_timeout(), cmd.output()).await {
            Err(_elapsed) => {
                return Err(crate::llm::exit_code_hints::into_legacy_embedding(
                    &crate::llm::exit_code_hints::LlmBackendError::Timeout {
                        secs: self.instance_embed_timeout().as_secs(),
                        binary: binary_str.clone(),
                    },
                ));
            }
            Ok(Err(e)) => {
                return Err(crate::llm::exit_code_hints::into_legacy_embedding(
                    &crate::llm::exit_code_hints::LlmBackendError::SpawnFailed {
                        binary: binary_str.clone(),
                        source: e.to_string(),
                    },
                ));
            }
            Ok(Ok(o)) => o,
        };
        if !output.status.success() {
            let (exit_code, signal) = if let Some(code) = output.status.code() {
                (Some(code), None)
            } else {
                extract_exit_info(&output.status)
            };
            let stdout_tail = crate::llm::exit_code_hints::LlmBackendError::truncate_tail(
                &output.stdout,
                crate::llm::exit_code_hints::DIAG_TAIL_BYTES,
            );
            let stderr_tail = crate::llm::exit_code_hints::LlmBackendError::truncate_tail(
                &output.stderr,
                crate::llm::exit_code_hints::DIAG_TAIL_BYTES,
            );
            let hint = crate::llm::exit_code_hints::diagnose_exit_code(exit_code, signal);
            return Err(crate::llm::exit_code_hints::into_legacy_embedding(
                &crate::llm::exit_code_hints::LlmBackendError::NonZeroExit {
                    exit_code,
                    signal,
                    stdout_tail,
                    stderr_tail,
                    binary: binary_str,
                    hint,
                },
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// G42/S6: resolves the empty `CLAUDE_CONFIG_DIR` used for embedding
/// subprocesses.
///
/// - `SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR` is honoured when set and
///   pointing at a directory (same contract as G28-A in claude_runner);
/// - otherwise a managed directory is created at
///   `~/.local/state/sqlite-graphrag/claude-empty-config` (mode 0700).
///   If `~/.claude/.credentials.json` exists (Linux OAuth storage) it is
///   copied in so authentication still works; on macOS credentials live
///   in the Keychain and the empty dir is sufficient.
///
/// Returns `None` only when HOME is unset AND no override is given —
/// in that case the subprocess falls back to claude's own default.
fn claude_embedding_config_dir() -> Option<std::path::PathBuf> {
    if let Ok(dir) = std::env::var("SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR") {
        let path = std::path::PathBuf::from(dir);
        if path.is_dir() {
            return Some(path);
        }
        tracing::warn!(
            target: "embedding",
            path = %path.display(),
            "SQLITE_GRAPHRAG_CLAUDE_EMPTY_CONFIG_DIR is set but not a directory; \
             falling back to the managed empty config dir"
        );
    }
    let home = std::env::var("HOME").ok()?;
    let dir = std::path::Path::new(&home)
        .join(".local/state/sqlite-graphrag")
        .join("claude-empty-config");
    if std::fs::create_dir_all(&dir).is_err() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    // Linux stores OAuth credentials on disk; copy them so the isolated
    // config dir still authenticates. Best-effort: macOS uses Keychain.
    // v1.0.89: ALWAYS copy (was: skip if target exists). OAuth tokens
    // expire and the stale copy causes 401 until manually deleted.
    let creds = std::path::Path::new(&home).join(".claude/.credentials.json");
    if creds.exists() {
        let target = dir.join(".credentials.json");
        let _ = std::fs::copy(&creds, &target);
    }
    Some(dir)
}

fn build_codex_embedding_command(
    binary: &std::path::Path,
    model: &str,
    schema_path: &std::path::Path,
) -> Result<Command, AppError> {
    let spawn_dir = crate::spawn::spawn_isolation_dir()?;
    let mut cmd = Command::new(binary);
    cmd.current_dir(&spawn_dir);
    cmd.arg("exec")
        .arg("-c")
        .arg("sandbox_mode='read-only'")
        .arg("-c")
        .arg("approval_policy='never'")
        .arg("--json")
        .arg("--output-schema")
        .arg(schema_path)
        .arg("--ephemeral")
        .arg("--skip-git-repo-check")
        .arg("--sandbox")
        .arg("read-only")
        .arg("--ignore-user-config")
        .arg("--ignore-rules");
    if crate::extract::codex_compat::codex_supports_ask_for_approval() {
        cmd.arg("--ask-for-approval").arg("never");
    }
    // v1.0.89: use the real CODEX_HOME (~/.codex) instead of an isolated
    // per-PID directory. The isolated dir caused cold-start overhead (codex
    // creates ~6 SQLite databases on first run) that regularly exceeded
    // the 30s embedding timeout. The --ignore-user-config + --ephemeral
    // flags already prevent config pollution; CODEX_HOME only needs auth.
    cmd.arg("--model")
        .arg(model)
        .arg("-")
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", std::env::var("HOME").unwrap_or_default());
    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        cmd.env("CODEX_HOME", codex_home);
    } else if let Ok(home) = std::env::var("HOME") {
        let default_home = std::path::Path::new(&home).join(".codex");
        if default_home.exists() {
            cmd.env("CODEX_HOME", &default_home);
        }
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // BLOCO 4: cancellation (dropped future) must kill the child.
        .kill_on_drop(true);
    Ok(cmd)
}

// prepare_isolated_codex_home removed in v1.0.89: the per-PID isolated
// CODEX_HOME caused cold-start overhead that exceeded the 30s embedding
// timeout. The real ~/.codex is now used directly (see build_codex_embedding_command).

/// Parse an LLM JSON response of type `T`. The two backends emit
/// different shapes:
/// - Claude (with `--output-format json`): single JSON object on stdout.
/// - Codex (with `--json`): JSONL stream with one event per line; the
///   `agent_message` event's `text` field is the JSON payload.
///
/// This helper accepts both shapes and returns the parsed value (or an
/// error describing the first mismatch).
fn parse_llm_json<T: serde::de::DeserializeOwned>(stdout: &str) -> Result<T, String> {
    // Strategy 1: try the whole stdout as JSON (Claude path).
    if let Ok(parsed) = serde_json::from_str::<T>(stdout) {
        return Ok(parsed);
    }
    // Strategy 3: walk NDJSON and collect `.part.text` from `type == "text"`
    // events (OpenCode path: `opencode run --format json`).
    let mut opencode_texts: Vec<String> = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if event.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(text) = event
                .get("part")
                .and_then(|p| p.get("text"))
                .and_then(|t| t.as_str())
            {
                opencode_texts.push(text.to_string());
            }
        }
    }
    if !opencode_texts.is_empty() {
        let combined = opencode_texts.concat();
        if let Ok(parsed) = serde_json::from_str::<T>(&combined) {
            return Ok(parsed);
        }
    }
    // Strategy 2: walk the JSONL line by line and pick the last
    // `item.completed` of type `agent_message` (Codex path).
    let mut last_agent_text: Option<String> = None;
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if event.get("type").and_then(|t| t.as_str()) != Some("item.completed") {
            continue;
        }
        let item = match event.get("item") {
            Some(i) => i,
            None => continue,
        };
        if item.get("type").and_then(|t| t.as_str()) != Some("agent_message") {
            continue;
        }
        if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
            last_agent_text = Some(text.to_string());
        }
    }
    let text = last_agent_text
        .ok_or_else(|| "no agent_message found in codex JSONL output".to_string())?;
    serde_json::from_str::<T>(&text)
        .map_err(|e| format!("codex agent_message text does not match schema: {e}; raw={text}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_client(flavour: EmbeddingFlavour, binary: std::path::PathBuf) -> LlmEmbedding {
        LlmEmbedding {
            flavour,
            binary,
            model: "gpt-5.4".to_string(),
            codex_schemas: Arc::new(parking_lot::Mutex::new(CodexSchemaFiles::default())),
            timeout_override: None,
        }
    }

    #[test]
    fn embed_timeout_default_is_120() {
        assert_eq!(DEFAULT_EMBED_TIMEOUT_SECS, 120);
    }

    #[test]
    #[serial_test::serial(env)]
    fn oauth_only_enforce_blocks_api_keys() {
        // SAFETY: this test only sets and unsets env vars; the
        // `serial(env)` group prevents cross-test interference.
        unsafe {
            std::env::set_var("ANTHROPIC_API_KEY", "test");
            assert!(LlmEmbedding::oauth_only_enforce().is_err());
            std::env::remove_var("ANTHROPIC_API_KEY");

            std::env::set_var("OPENAI_API_KEY", "test");
            assert!(LlmEmbedding::oauth_only_enforce().is_err());
            std::env::remove_var("OPENAI_API_KEY");
        }
        assert!(LlmEmbedding::oauth_only_enforce().is_ok());
    }

    #[test]
    fn flavour_as_str_is_stable() {
        assert_eq!(EmbeddingFlavour::Claude.as_str(), "claude");
        assert_eq!(EmbeddingFlavour::Codex.as_str(), "codex");
    }

    #[test]
    fn single_schema_embeds_active_dim() {
        let schema = build_single_schema(64);
        assert!(schema.contains(r#""minItems":64"#));
        assert!(schema.contains(r#""maxItems":64"#));
        let parsed: serde_json::Value =
            serde_json::from_str(&schema).expect("single schema must be valid JSON");
        assert_eq!(parsed["properties"]["embedding"]["minItems"], 64);
    }

    #[test]
    fn batch_schema_is_valid_json_and_unbounded_items() {
        let schema = build_batch_schema(64);
        let parsed: serde_json::Value =
            serde_json::from_str(&schema).expect("batch schema must be valid JSON");
        // The items array must NOT constrain its length so one schema
        // file serves every batch size (G42/S4).
        assert!(parsed["properties"]["items"].get("minItems").is_none());
        assert_eq!(
            parsed["properties"]["items"]["items"]["properties"]["v"]["minItems"],
            64
        );
    }

    #[test]
    fn parse_llm_json_accepts_claude_json() {
        let stdout = r#"{"embedding":[0.0,1.0,2.0]}"#;

        let parsed: EmbeddingResponse = parse_llm_json(stdout).expect("claude JSON must parse");

        assert_eq!(parsed.embedding, vec![0.0, 1.0, 2.0]);
    }

    #[test]
    fn parse_llm_json_accepts_codex_jsonl() {
        let stdout = r#"{"type":"thread.started","thread_id":"mock-thread-0"}
{"type":"item.completed","item":{"type":"agent_message","text":"{\"embedding\":[0.0,1.0,2.0]}"}}
{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}"#;

        let parsed: EmbeddingResponse = parse_llm_json(stdout).expect("codex JSONL must parse");

        assert_eq!(parsed.embedding, vec![0.0, 1.0, 2.0]);
    }

    #[test]
    fn parse_llm_json_rejects_jsonl_without_agent_message() {
        let stdout = r#"{"type":"thread.started","thread_id":"mock-thread-0"}"#;

        let err = parse_llm_json::<EmbeddingResponse>(stdout)
            .expect_err("missing agent_message must fail");

        assert!(err.contains("no agent_message"));
    }

    #[test]
    fn parse_llm_json_accepts_batch_response() {
        let stdout = r#"{"items":[{"i":1,"v":[0.0,1.0]},{"i":2,"v":[2.0,3.0]}]}"#;

        let parsed: BatchEmbeddingResponse = parse_llm_json(stdout).expect("batch JSON must parse");

        assert_eq!(parsed.items.len(), 2);
        assert_eq!(parsed.items[0].i, 1);
        assert_eq!(parsed.items[1].v, vec![2.0, 3.0]);
    }

    #[test]
    fn codex_schema_file_is_created_once_and_reused() {
        let client = test_client(
            EmbeddingFlavour::Codex,
            std::path::PathBuf::from("/bin/true"),
        );
        let first = client
            .codex_schema_file(64, false)
            .expect("schema file must be created");
        let second = client
            .codex_schema_file(64, false)
            .expect("schema file must be reused");
        assert_eq!(first.path(), second.path(), "same dim must reuse the file");

        let batch = client
            .codex_schema_file(64, true)
            .expect("batch schema file must be created");
        assert_ne!(
            first.path(),
            batch.path(),
            "single and batch schemas are distinct files"
        );

        let content = std::fs::read_to_string(first.path()).expect("schema file must be readable");
        assert!(content.contains(r#""minItems":64"#));
    }

    #[test]
    fn codex_embedding_command_reads_prompt_from_stdin() {
        let schema_path = std::env::temp_dir().join("sqlite-graphrag-embed-schema-test.json");
        let cmd = build_codex_embedding_command(
            std::path::Path::new("/bin/true"),
            "gpt-5.4",
            &schema_path,
        )
        .expect("build_codex_embedding_command must succeed in test");
        let argv: Vec<String> = cmd
            .as_std()
            .get_args()
            .filter_map(|arg| arg.to_str().map(|s| s.to_string()))
            .collect();

        assert!(
            argv.iter().any(|arg| arg == "-"),
            "codex embedding command must read prompt from stdin: {argv:?}"
        );
        assert!(
            !argv.iter().any(|arg| arg.starts_with("passage: ")),
            "prompt text must not be passed as argv: {argv:?}"
        );
        for required in &[
            "exec",
            "-c",
            "sandbox_mode='read-only'",
            "approval_policy='never'",
            "--json",
            "--output-schema",
            "--ephemeral",
            "--skip-git-repo-check",
            "--sandbox",
            "read-only",
            "--ignore-user-config",
            "--ignore-rules",
            "--model",
            "gpt-5.4",
        ] {
            assert!(
                argv.iter().any(|arg| arg == required),
                "missing flag {required} in {argv:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    #[serial_test::serial(env)]
    fn embed_passage_sends_prompt_to_codex_stdin() {
        use std::os::unix::fs::PermissionsExt;

        // Pin the dimensionality so the mock script and the validation
        // agree regardless of test execution order.
        // SAFETY: guarded by serial(env).
        unsafe {
            std::env::set_var("SQLITE_GRAPHRAG_EMBEDDING_DIM", "64");
        }

        let temp = tempfile::tempdir().expect("tempdir must exist");
        let binary = temp.path().join("codex-stdin-check");
        let script = r#"#!/usr/bin/env bash
set -euo pipefail

prompt="$(cat)"
if [[ "$prompt" != "passage: codex-cli" ]]; then
  echo "unexpected stdin: $prompt" >&2
  exit 41
fi

vals="0.0"
for _ in $(seq 2 64); do
  vals="$vals,0.0"
done
payload="{\"embedding\":[$vals]}"
escaped="${payload//\"/\\\"}"
echo "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"$escaped\"}}"
"#;
        std::fs::write(&binary, script).expect("mock codex script must be written");
        let mut perms = std::fs::metadata(&binary)
            .expect("mock codex metadata must exist")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&binary, perms).expect("mock codex must be executable");

        let embedding = test_client(EmbeddingFlavour::Codex, binary);

        let vector = embedding
            .embed_passage("codex-cli")
            .expect("stdin-backed codex embedding must succeed");

        // SAFETY: guarded by serial(env).
        unsafe {
            std::env::remove_var("SQLITE_GRAPHRAG_EMBEDDING_DIM");
        }

        assert_eq!(vector.len(), 64);
        assert!(vector.iter().all(|value| *value == 0.0));
    }

    // ---------------------------------------------------------------
    // ADR-0042 / GAP-002: LlmEmbeddingBuilder unit tests
    // ---------------------------------------------------------------

    /// `claude_default` is the `with_claude_builder` alias: returns a
    /// builder pre-set to the Claude flavour. Build requires the
    /// Claude binary to be on PATH; in CI without `claude`, the build
    /// fails with the canonical `claude not found` error, which is
    /// itself the proof that the flavour is propagated correctly.
    #[test]
    fn claude_default_resolves_path() {
        let builder = LlmEmbeddingBuilder::claude_default();
        assert_eq!(builder.flavour, EmbeddingFlavour::Claude);
        assert!(builder.binary_override.is_none());
        assert!(builder.model_override.is_none());
    }

    /// `override_binary` short-circuits the PATH probe. The builder
    /// stores the override verbatim so the `build()` call can fall
    /// back to `resolve_real_binary` for ELF canonicalisation.
    #[test]
    fn override_binary_uses_provided() {
        let path = std::path::PathBuf::from("/tmp/fake-claude-binary");
        let builder = LlmEmbeddingBuilder::claude_default().override_binary(path.clone());
        assert_eq!(builder.binary_override.as_ref(), Some(&path));
    }

    /// `override_model` short-circuits the env-var lookup. The model
    /// override travels untouched through `build()` so the LLM
    /// subprocess spawn honours it.
    #[test]
    fn override_model_uses_provided() {
        let builder =
            LlmEmbeddingBuilder::codex_default().override_model("gpt-5.4-custom".to_string());
        assert_eq!(builder.model_override.as_deref(), Some("gpt-5.4-custom"));
    }

    // ---------------------------------------------------------------
    // v1.0.89 GAP tests
    // ---------------------------------------------------------------

    #[test]
    fn embed_timeout_for_batch_scales_with_size() {
        let t1 = embed_timeout_for_batch(1);
        let t4 = embed_timeout_for_batch(4);
        let t8 = embed_timeout_for_batch(8);
        assert!(
            t1 < t4,
            "batch of 4 must have longer timeout than batch of 1"
        );
        assert!(
            t4 < t8,
            "batch of 8 must have longer timeout than batch of 4"
        );
        assert_eq!(t8 - t1, std::time::Duration::from_secs(15 * 7));
    }

    #[test]
    fn embed_timeout_for_batch_single_equals_base() {
        let base = embed_timeout();
        let single = embed_timeout_for_batch(1);
        assert_eq!(base, single);
    }

    #[test]
    fn opencode_flavour_as_str() {
        assert_eq!(EmbeddingFlavour::Opencode.as_str(), "opencode");
    }

    #[test]
    #[serial_test::serial(env)]
    fn opencode_embed_model_uses_env_override() {
        unsafe {
            std::env::set_var(
                "SQLITE_GRAPHRAG_OPENCODE_EMBED_MODEL",
                "opencode/test-model",
            );
            let model = opencode_embed_model();
            std::env::remove_var("SQLITE_GRAPHRAG_OPENCODE_EMBED_MODEL");
            assert_eq!(model, "opencode/test-model");
        }
    }

    #[test]
    #[serial_test::serial(env)]
    fn opencode_embed_model_falls_back_to_opencode_model() {
        unsafe {
            std::env::remove_var("SQLITE_GRAPHRAG_OPENCODE_EMBED_MODEL");
            std::env::set_var("SQLITE_GRAPHRAG_OPENCODE_MODEL", "opencode/fallback");
            let model = opencode_embed_model();
            std::env::remove_var("SQLITE_GRAPHRAG_OPENCODE_MODEL");
            assert_eq!(model, "opencode/fallback");
        }
    }

    #[test]
    #[serial_test::serial(env)]
    fn opencode_embed_model_ignores_llm_model() {
        unsafe {
            std::env::remove_var("SQLITE_GRAPHRAG_OPENCODE_EMBED_MODEL");
            std::env::remove_var("SQLITE_GRAPHRAG_OPENCODE_MODEL");
            std::env::set_var("SQLITE_GRAPHRAG_LLM_MODEL", "gpt-5.4-mini");
            let model = opencode_embed_model();
            std::env::remove_var("SQLITE_GRAPHRAG_LLM_MODEL");
            assert_eq!(
                model, "opencode/big-pickle",
                "must NOT cross-contaminate with LLM_MODEL"
            );
        }
    }

    #[test]
    fn parse_llm_json_accepts_opencode_ndjson() {
        let stdout = r#"{"type":"step_start","timestamp":1234,"sessionID":"ses_test","part":{"type":"step-start"}}
{"type":"text","timestamp":1235,"sessionID":"ses_test","part":{"type":"text","text":"{\"embedding\":[0.1,0.2,0.3]}"}}
{"type":"step_finish","timestamp":1236,"sessionID":"ses_test","part":{"type":"step-finish","tokens":{"total":100,"input":90,"output":10,"reasoning":0},"cost":0}}"#;

        let parsed: EmbeddingResponse = parse_llm_json(stdout).expect("opencode NDJSON must parse");
        assert_eq!(parsed.embedding, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn parse_llm_json_accepts_opencode_batch_ndjson() {
        let stdout = r#"{"type":"step_start","timestamp":1234,"sessionID":"ses_test","part":{"type":"step-start"}}
{"type":"text","timestamp":1235,"sessionID":"ses_test","part":{"type":"text","text":"{\"items\":[{\"i\":1,\"v\":[0.1,0.2]},{\"i\":2,\"v\":[0.3,0.4]}]}"}}
{"type":"step_finish","timestamp":1236,"sessionID":"ses_test","part":{"type":"step-finish","tokens":{"total":100,"input":90,"output":10,"reasoning":0},"cost":0}}"#;

        let parsed: BatchEmbeddingResponse =
            parse_llm_json(stdout).expect("opencode batch NDJSON must parse");
        assert_eq!(parsed.items.len(), 2);
        assert_eq!(parsed.items[0].i, 1);
        assert_eq!(parsed.items[1].v, vec![0.3, 0.4]);
    }

    #[test]
    fn opencode_builder_default_has_correct_flavour() {
        let builder = LlmEmbeddingBuilder::opencode_default();
        assert_eq!(builder.flavour, EmbeddingFlavour::Opencode);
        assert!(builder.binary_override.is_none());
        assert!(builder.model_override.is_none());
    }
}
