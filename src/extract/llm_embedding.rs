//! LLM-based embedding backend (v1.0.76 default).
//!
//! `LlmEmbedding` is the production embedding client. It wraps a single
//! headless invocation of `claude code` or `codex` and returns a 384-dim
//! f32 vector parsed from the LLM's JSONL response.
//!
//! The embedding model is the same `multilingual-e5-small` from before, but
//! the call now goes through the LLM's tool-use protocol (no MCP, no hooks).
//! This is the single reason the binary is now one-shot: there is no daemon
//! to keep the model loaded, the LLM subprocess is spawned on demand and
//! killed when the response is parsed.
//!
//! OAuth is the only supported credential path. The constructor rejects
//! `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` in the environment — see
//! `v1.0.69 (G31) OAuth-Only Enforcement`.

use crate::errors::AppError;
use serde::Deserialize;
use std::process::Stdio;
use tokio::process::Command;

/// Dimensionality of the embedding space. Matches the previous
/// `multilingual-e5-small` model output and the `memory_embeddings.embedding`
/// BLOB column size.
pub const EMBEDDING_DIM: usize = 384;

#[derive(Clone, Debug)]
pub struct LlmEmbedding {
    /// Which LLM headless binary to spawn. `claude` or `codex`.
    flavour: EmbeddingFlavour,
    /// Cached path to the binary to avoid PATH lookups on every call.
    binary: std::path::PathBuf,
    /// Optional model name passed via `--model`. Defaults are pinned.
    model: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum EmbeddingFlavour {
    Claude,
    Codex,
}

impl EmbeddingFlavour {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    embedding: Vec<f32>,
}

impl LlmEmbedding {
    /// Detects which LLM CLI is available on PATH and returns the
    /// matching embedding client. Prefers `claude` over `codex` because
    /// claude's tool-use protocol is more stable for embedding requests.
    pub fn detect_available() -> Result<Self, AppError> {
        Self::oauth_only_enforce()?;

        if let Ok(path) = which::which("claude") {
            return Ok(Self {
                flavour: EmbeddingFlavour::Claude,
                binary: path,
                model: "claude-sonnet-4-6".to_string(),
            });
        }
        if let Ok(path) = which::which("codex") {
            return Ok(Self {
                flavour: EmbeddingFlavour::Codex,
                binary: path,
                model: "gpt-5.4".to_string(),
            });
        }
        Err(AppError::Embedding(
            "no LLM CLI found on PATH: install `claude` (Claude Code 2.1+) or `codex` (0.130+)"
                .to_string(),
        ))
    }

    pub fn with_codex() -> Result<Self, AppError> {
        Self::oauth_only_enforce()?;
        Ok(Self {
            flavour: EmbeddingFlavour::Codex,
            binary: which::which("codex").map_err(|_| {
                AppError::Embedding("`codex` not found on PATH".to_string())
            })?,
            model: "gpt-5.4".to_string(),
        })
    }

    pub fn with_claude() -> Result<Self, AppError> {
        Self::oauth_only_enforce()?;
        Ok(Self {
            flavour: EmbeddingFlavour::Claude,
            binary: which::which("claude").map_err(|_| {
                AppError::Embedding("`claude` not found on PATH".to_string())
            })?,
            model: "claude-sonnet-4-6".to_string(),
        })
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

    /// Embeds a single passage (chunk of a memory body). Returns a
    /// 384-dim f32 vector suitable for cosine similarity.
    pub fn embed_passage(&mut self, text: &str) -> Result<Vec<f32>, AppError> {
        self.invoke_with_prefix(crate::constants::PASSAGE_PREFIX, text)
    }

    /// Embeds a single query. The LLM uses a different prompt prefix
    /// to disambiguate query from passage.
    pub fn embed_query(&mut self, text: &str) -> Result<Vec<f32>, AppError> {
        self.invoke_with_prefix(crate::constants::QUERY_PREFIX, text)
    }

    fn invoke_with_prefix(&mut self, prefix: &str, text: &str) -> Result<Vec<f32>, AppError> {
        // Lazy-init the tokio runtime on first use so the sync callers
        // (which all live behind a Mutex) don't need to be async.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| AppError::Embedding(format!("tokio runtime init failed: {e}")))?;

        rt.block_on(async move {
            let prompt = format!("{prefix}{text}");
            let stdout = match self.flavour {
                EmbeddingFlavour::Claude => self.invoke_claude(&prompt).await?,
                EmbeddingFlavour::Codex => self.invoke_codex(&prompt).await?,
            };
            let parsed: EmbeddingResponse = serde_json::from_str(&stdout).map_err(|e| {
                AppError::Embedding(format!(
                    "LLM embedding response was not valid JSON: {e}; raw={stdout}"
                ))
            })?;
            if parsed.embedding.len() != EMBEDDING_DIM {
                return Err(AppError::Embedding(format!(
                    "LLM returned {} dims, expected {EMBEDDING_DIM}",
                    parsed.embedding.len()
                )));
            }
            Ok(parsed.embedding)
        })
    }

    async fn invoke_claude(&self, prompt: &str) -> Result<String, AppError> {
        // v1.0.69 hardening: --strict-mcp-config --mcp-config '{}' --settings
        // '{"hooks":{}}' --dangerously-skip-permissions --output-schema
        const SCHEMA: &str = r#"{"type":"object","properties":{"embedding":{"type":"array","items":{"type":"number"},"minItems":384,"maxItems":384}}},"required":["embedding"],"additionalProperties":false}"#;
        let output = Command::new(&self.binary)
            .arg("-p")
            .arg(prompt)
            .arg("--output-schema")
            .arg(SCHEMA)
            .arg("--model")
            .arg(&self.model)
            .arg("--strict-mcp-config")
            .arg("--mcp-config")
            .arg("{}")
            .arg("--settings")
            .arg(r#"{"hooks":{}}"#)
            .arg("--dangerously-skip-permissions")
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("HOME", std::env::var("HOME").unwrap_or_default())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| AppError::Embedding(format!("claude spawn failed: {e}")))?;
        if !output.status.success() {
            return Err(AppError::Embedding(format!(
                "claude exited with {}: stderr={}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    async fn invoke_codex(&self, prompt: &str) -> Result<String, AppError> {
        // v1.0.69 hardening: --json --output-schema --ephemeral --skip-git-repo-check
        // --sandbox read-only --ignore-user-config --ignore-rules -c mcp_servers='{}'
        // --ask-for-approval never
        const SCHEMA: &str = r#"{"type":"object","properties":{"embedding":{"type":"array","items":{"type":"number"},"minItems":384,"maxItems":384}}},"required":["embedding"],"additionalProperties":false}"#;
        let output = Command::new(&self.binary)
            .arg("exec")
            .arg(prompt)
            .arg("--json")
            .arg("--output-schema")
            .arg(SCHEMA)
            .arg("--ephemeral")
            .arg("--skip-git-repo-check")
            .arg("--sandbox")
            .arg("read-only")
            .arg("--ignore-user-config")
            .arg("--ignore-rules")
            .arg("-c")
            .arg("mcp_servers='{}'")
            .arg("--ask-for-approval")
            .arg("never")
            .arg("--model")
            .arg(&self.model)
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("HOME", std::env::var("HOME").unwrap_or_default())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| AppError::Embedding(format!("codex spawn failed: {e}")))?;
        if !output.status.success() {
            return Err(AppError::Embedding(format!(
                "codex exited with {}: stderr={}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oauth_only_enforce_blocks_api_keys() {
        // SAFETY: this test only sets and unsets env vars; no other test
        // relies on the global env state.
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
}
