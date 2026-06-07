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
    /// matching embedding client.
    ///
    /// v1.0.76: PREFERS `codex` over `claude` because:
    /// - Claude Code 2.1+ ships a 180k+ token system context (plugins,
    ///   skills, agents, MCP) that overflows the 200k context window
    ///   for even trivial embedding prompts and returns "Prompt is too
    ///   long".
    /// - Codex 0.134+ is lightweight (~5k system context) and the
    ///   `StructuredOutput` tool reliably returns 384-dim vectors.
    pub fn detect_available() -> Result<Self, AppError> {
        Self::oauth_only_enforce()?;

        if let Ok(path) = which::which("codex") {
            return Ok(Self {
                flavour: EmbeddingFlavour::Codex,
                binary: path,
                model: "gpt-5.4".to_string(),
            });
        }
        if let Ok(path) = which::which("claude") {
            return Ok(Self {
                flavour: EmbeddingFlavour::Claude,
                binary: path,
                model: "claude-sonnet-4-6".to_string(),
            });
        }
        Err(AppError::Embedding(
            "no LLM CLI found on PATH: install `codex` (0.130+) or `claude` (Claude Code 2.1+)"
                .to_string(),
        ))
    }

    pub fn with_codex() -> Result<Self, AppError> {
        Self::oauth_only_enforce()?;
        Ok(Self {
            flavour: EmbeddingFlavour::Codex,
            binary: which::which("codex")
                .map_err(|_| AppError::Embedding("`codex` not found on PATH".to_string()))?,
            model: "gpt-5.4".to_string(),
        })
    }

    pub fn with_claude() -> Result<Self, AppError> {
        Self::oauth_only_enforce()?;
        Ok(Self {
            flavour: EmbeddingFlavour::Claude,
            binary: which::which("claude")
                .map_err(|_| AppError::Embedding("`claude` not found on PATH".to_string()))?,
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
        // v1.0.76: tolerate being called from inside an existing tokio
        // runtime (e.g. a test marked `#[tokio::test]`) by reusing the
        // current Handle via block_in_place. When no runtime is in scope
        // we build a one-shot current-thread runtime.
        let prompt = format!("{prefix}{text}");
        let inner = async {
            match self.flavour {
                EmbeddingFlavour::Claude => self.invoke_claude(&prompt).await,
                EmbeddingFlavour::Codex => self.invoke_codex(&prompt).await,
            }
        };
        let stdout: String = match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(|| handle.block_on(inner))?,
            Err(_) => {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| AppError::Embedding(format!("tokio runtime init failed: {e}")))?;
                rt.block_on(inner)?
            }
        };

        let parsed: EmbeddingResponse = parse_embedding_response(&stdout).map_err(|e| {
            AppError::Embedding(format!("LLM embedding response parse failed: {e}; raw={stdout}"))
        })?;
        if parsed.embedding.len() != EMBEDDING_DIM {
            return Err(AppError::Embedding(format!(
                "LLM returned {} dims, expected {EMBEDDING_DIM}",
                parsed.embedding.len()
            )));
        }
        Ok(parsed.embedding)
    }

    async fn invoke_claude(&self, prompt: &str) -> Result<String, AppError> {
        // v1.0.69 hardening: --strict-mcp-config --mcp-config '{}' --settings
        // '{"hooks":{}}' --dangerously-skip-permissions.
        //
        // v1.0.76 hardening: Claude Code 2.1+ renamed --output-schema to
        // --json-schema and accepts the schema as an inline JSON string
        // (NOT a file path). Also pass --output-format json so the
        // response is a single JSON object on stdout (the default text
        // mode returns prose which fails the `embedding` field check).
        const SCHEMA: &str = r#"{"type":"object","properties":{"embedding":{"type":"array","items":{"type":"number"},"minItems":384,"maxItems":384}},"required":["embedding"],"additionalProperties":false}"#;
        let output = Command::new(&self.binary)
            .arg("-p")
            .arg(prompt)
            .arg("--model")
            .arg(&self.model)
            .arg("--json-schema")
            .arg(SCHEMA)
            .arg("--output-format")
            .arg("json")
            .arg("--strict-mcp-config")
            .arg("--mcp-config")
            .arg(r#"{"mcpServers":{}}"#)
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
        // --sandbox read-only --ignore-user-config --ignore-rules
        //
        // v1.0.76 hardening (G31 + G33 + codex 0.134+ compat):
        // - --ask-for-approval removed in 0.134+ (Issue #26602) — gated
        //   by the codex_compat helper
        // - -c mcp_servers='{}' removed — value is parsed as string and
        //   rejected ("expected a map"). --ignore-user-config already
        //   covers the MCP isolation requirement.
        // - --output-schema is a FILE PATH (not inline JSON like
        //   claude's --json-schema). Write to a temp file in the
        //   cache dir (matches the trusted-schema-path pattern used by
        //   codex_spawn).
        const SCHEMA: &str = r#"{"type":"object","properties":{"embedding":{"type":"array","items":{"type":"number"},"minItems":384,"maxItems":384}},"required":["embedding"],"additionalProperties":false}"#;
        let schema_path = std::env::temp_dir().join(format!(
            "sqlite-graphrag-embed-schema-{}.json",
            std::process::id()
        ));
        std::fs::write(&schema_path, SCHEMA)
            .map_err(|e| AppError::Embedding(format!("failed to write schema file: {e}")))?;
        let mut cmd = Command::new(&self.binary);
        cmd.arg("exec")
            .arg(prompt)
            .arg("--json")
            .arg("--output-schema")
            .arg(&schema_path)
            .arg("--ephemeral")
            .arg("--skip-git-repo-check")
            .arg("--sandbox")
            .arg("read-only")
            .arg("--ignore-user-config")
            .arg("--ignore-rules");
        if crate::extract::codex_compat::codex_supports_ask_for_approval() {
            cmd.arg("--ask-for-approval").arg("never");
        }
        let output = cmd
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
        let _ = std::fs::remove_file(&schema_path);
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

/// Parse the LLM embedding response. The two backends emit different
/// shapes:
/// - Claude (with `--output-format json`): single JSON object on stdout.
/// - Codex (with `--json`): JSONL stream with one event per line; the
///   `agent_message` event's `text` field is the JSON payload.
///
/// This helper accepts both shapes and returns the parsed
/// `EmbeddingResponse` (or an error describing the first mismatch).
fn parse_embedding_response(stdout: &str) -> Result<EmbeddingResponse, String> {
    // Strategy 1: try the whole stdout as JSON (Claude path).
    if let Ok(parsed) = serde_json::from_str::<EmbeddingResponse>(stdout) {
        return Ok(parsed);
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
    serde_json::from_str::<EmbeddingResponse>(&text)
        .map_err(|e| format!("codex agent_message text is not EmbeddingResponse: {e}; raw={text}"))
}
