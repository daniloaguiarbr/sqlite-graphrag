//! Codex CLI version adapter (v1.0.75 — G22)

use super::compat_matrix::codex_capabilities;
use super::executor_version::ExecutorVersion;
use super::{CompatMode, ParsedOutput, VersionAdapter};
use crate::errors::AppError;
use async_trait::async_trait;
use std::process::Command;

pub struct CodexAdapter;

#[async_trait]
impl VersionAdapter for CodexAdapter {
    fn name(&self) -> &'static str {
        "codex"
    }

    async fn detect(&self) -> Result<ExecutorVersion, AppError> {
        let output = Command::new("codex").arg("--version").output();
        match output {
            Ok(out) => {
                let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if raw.is_empty() {
                    let raw = String::from_utf8_lossy(&out.stderr).trim().to_string();
                    if raw.is_empty() {
                        return Ok(ExecutorVersion::unknown());
                    }
                    return ExecutorVersion::parse(&raw);
                }
                ExecutorVersion::parse(&raw)
            }
            Err(_) => Ok(ExecutorVersion::unknown()),
        }
    }

    fn capabilities_for(&self, version: &ExecutorVersion) -> super::ExecutorCapabilities {
        codex_capabilities(version)
    }

    fn build_args(
        &self,
        prompt: &str,
        caps: &super::ExecutorCapabilities,
        _compat_mode: CompatMode,
    ) -> Vec<String> {
        let mut args = vec!["exec".to_string()];
        args.extend(caps.default_flags.clone());
        args.push("--json".to_string());
        args.push(prompt.to_string());
        args
    }

    fn parse_output(&self, raw_stdout: &str, raw_stderr: &str, exit_code: i32) -> ParsedOutput {
        let mut items = Vec::new();
        for line in raw_stdout.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
                items.push(v);
            } else if let Ok(v) =
                serde_json::from_str::<serde_json::Value>(&format!("[{trimmed}]"))
            {
                if let Some(arr) = v.as_array() {
                    for item in arr {
                        items.push(item.clone());
                    }
                }
            }
        }
        ParsedOutput {
            items,
            raw_stdout: raw_stdout.to_string(),
            raw_stderr: raw_stderr.to_string(),
            exit_code,
        }
    }
}
