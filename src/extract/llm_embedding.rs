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
use tokio::io::AsyncWriteExt;
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

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    embedding: Vec<f32>,
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
                binary: resolve_real_binary(&path),
                model: "gpt-5.4".to_string(),
            });
        }
        if let Ok(path) = which::which("claude") {
            return Ok(Self {
                flavour: EmbeddingFlavour::Claude,
                binary: resolve_real_binary(&path),
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
        let path = which::which("codex")
            .map_err(|_| AppError::Embedding("`codex` not found on PATH".to_string()))?;
        Ok(Self {
            flavour: EmbeddingFlavour::Codex,
            binary: resolve_real_binary(&path),
            model: "gpt-5.4".to_string(),
        })
    }

    pub fn with_claude() -> Result<Self, AppError> {
        Self::oauth_only_enforce()?;
        let path = which::which("claude")
            .map_err(|_| AppError::Embedding("`claude` not found on PATH".to_string()))?;
        Ok(Self {
            flavour: EmbeddingFlavour::Claude,
            binary: resolve_real_binary(&path),
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
            AppError::Embedding(format!(
                "LLM embedding response parse failed: {e}; raw={stdout}"
            ))
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
        let mut child = build_codex_embedding_command(&self.binary, &self.model, &schema_path)
            .spawn()
            .map_err(|e| AppError::Embedding(format!("codex spawn failed: {e}")))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(prompt.as_bytes())
                .await
                .map_err(|e| AppError::Embedding(format!("codex stdin write failed: {e}")))?;
        }
        let output = child
            .wait_with_output()
            .await
            .map_err(|e| AppError::Embedding(format!("codex wait failed: {e}")))?;
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

fn build_codex_embedding_command(
    binary: &std::path::Path,
    model: &str,
    schema_path: &std::path::Path,
) -> Command {
    let mut cmd = Command::new(binary);
    // v1.0.77: `-c` TOML overrides bypass the codex exec --sandbox propagation
    // bug (openai/codex#18113). CLI flags alone are insufficient — the exec
    // subcommand may not inherit --sandbox from the parent codex command.
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
    // v1.0.77: isolate codex from user config by pointing CODEX_HOME at a
    // minimal directory containing only auth.json (OAuth credentials).
    let codex_home = prepare_isolated_codex_home();
    cmd.arg("--model")
        .arg(model)
        .arg("-")
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", std::env::var("HOME").unwrap_or_default());
    if let Some(ref ch) = codex_home {
        cmd.env("CODEX_HOME", ch);
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

fn prepare_isolated_codex_home() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let real_auth = std::path::Path::new(&home).join(".codex/auth.json");
    if !real_auth.exists() {
        return None;
    }
    let isolated =
        std::env::temp_dir().join(format!("sqlite-graphrag-codex-home-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&isolated);
    let target = isolated.join("auth.json");
    if !target.exists() {
        let _ = std::fs::copy(&real_auth, &target);
    }
    Some(isolated)
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

    #[test]
    fn parse_embedding_response_accepts_claude_json() {
        let stdout = r#"{"embedding":[0.0,1.0,2.0]}"#;

        let parsed = parse_embedding_response(stdout).expect("claude JSON must parse");

        assert_eq!(parsed.embedding, vec![0.0, 1.0, 2.0]);
    }

    #[test]
    fn parse_embedding_response_accepts_codex_jsonl() {
        let stdout = r#"{"type":"thread.started","thread_id":"mock-thread-0"}
{"type":"item.completed","item":{"type":"agent_message","text":"{\"embedding\":[0.0,1.0,2.0]}"}}
{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}"#;

        let parsed = parse_embedding_response(stdout).expect("codex JSONL must parse");

        assert_eq!(parsed.embedding, vec![0.0, 1.0, 2.0]);
    }

    #[test]
    fn parse_embedding_response_rejects_jsonl_without_agent_message() {
        let stdout = r#"{"type":"thread.started","thread_id":"mock-thread-0"}"#;

        let err = parse_embedding_response(stdout).expect_err("missing agent_message must fail");

        assert!(err.contains("no agent_message"));
    }

    #[test]
    fn codex_embedding_command_reads_prompt_from_stdin() {
        let schema_path = std::env::temp_dir().join("sqlite-graphrag-embed-schema-test.json");
        let cmd = build_codex_embedding_command(
            std::path::Path::new("/bin/true"),
            "gpt-5.4",
            &schema_path,
        );
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
    fn embed_passage_sends_prompt_to_codex_stdin() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("tempdir must exist");
        let binary = temp.path().join("codex-stdin-check");
        let script = r#"#!/usr/bin/env bash
set -euo pipefail

prompt="$(cat)"
if [[ "$prompt" != "passage: codex-cli" ]]; then
  echo "unexpected stdin: $prompt" >&2
  exit 41
fi

python3 - <<'PY'
import json
payload = json.dumps({"embedding": [0.0] * 384})
print(json.dumps({
    "type": "item.completed",
    "item": {
        "type": "agent_message",
        "text": payload,
    },
}))
PY
"#;
        std::fs::write(&binary, script).expect("mock codex script must be written");
        let mut perms = std::fs::metadata(&binary)
            .expect("mock codex metadata must exist")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&binary, perms).expect("mock codex must be executable");

        let mut embedding = LlmEmbedding {
            flavour: EmbeddingFlavour::Codex,
            binary,
            model: "gpt-5.4".to_string(),
        };

        let vector = embedding
            .embed_passage("codex-cli")
            .expect("stdin-backed codex embedding must succeed");

        assert_eq!(vector.len(), EMBEDDING_DIM);
        assert!(vector.iter().all(|value| *value == 0.0));
    }
}
