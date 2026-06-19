//! Spawn subsystem abstraction (v1.0.75 — G22 solution)
//!
//! Provides `VersionAdapter` trait that detects the version of external CLI
//! executors (claude code, codex CLI, opencode headless) and adapts flags,
//! schema and error handling accordingly.

pub mod claude_adapter;
pub mod codex_adapter;
pub mod compat_matrix;
pub mod env_whitelist;
pub mod error_propagator;
pub mod executor_version;
pub mod opencode_adapter;
pub mod preflight;

use crate::errors::AppError;
use async_trait::async_trait;
use executor_version::ExecutorVersion;
use std::collections::BTreeMap;
use std::process::Stdio;

/// Result of parsing a subprocess output stream.
#[derive(Debug, Clone)]
pub struct ParsedOutput {
    pub items: Vec<serde_json::Value>,
    pub raw_stdout: String,
    pub raw_stderr: String,
    pub exit_code: i32,
}

/// Detected capability of a given executor version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutorCapabilities {
    pub supports_mcp_map: bool,
    pub supports_ask_for_approval_flag: bool,
    pub supports_strict_schema: bool,
    pub default_flags: Vec<String>,
    pub removed_flags: Vec<String>,
}

impl ExecutorCapabilities {
    pub fn empty() -> Self {
        Self {
            supports_mcp_map: false,
            supports_ask_for_approval_flag: false,
            supports_strict_schema: false,
            default_flags: Vec::new(),
            removed_flags: Vec::new(),
        }
    }
}

/// Trait for adapting spawn invocations to a particular executor's version.
#[async_trait]
pub trait VersionAdapter: Send + Sync {
    /// Logical name of the executor (e.g. "codex", "claude", "opencode").
    fn name(&self) -> &'static str;

    /// Detect the version by invoking `<executor> --version` and parsing the output.
    async fn detect(&self) -> Result<ExecutorVersion, AppError>;

    /// Returns the capability matrix for the given version.
    fn capabilities_for(&self, version: &ExecutorVersion) -> ExecutorCapabilities;

    /// Build the CLI invocation arguments for a given prompt and capabilities.
    fn build_args(
        &self,
        prompt: &str,
        caps: &ExecutorCapabilities,
        compat_mode: CompatMode,
    ) -> Vec<String>;

    /// Parses the executor output into structured items.
    fn parse_output(&self, raw_stdout: &str, raw_stderr: &str, exit_code: i32) -> ParsedOutput;
}

/// Compatibility mode controlling how strict the adapter is with version drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatMode {
    /// Abort on unknown versions
    Strict,
    /// Try the invocation anyway
    Lenient,
    /// Auto-detect and adapt (default)
    Auto,
}

impl CompatMode {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "strict" => Self::Strict,
            "lenient" => Self::Lenient,
            _ => Self::Auto,
        }
    }
}

/// In-memory cache of `executor -> ExecutorVersion` to avoid re-spawning
/// `--version` on every command. Resettable via `--executor-version-check`.
#[derive(Debug, Default)]
pub struct VersionCache {
    inner: std::sync::Mutex<BTreeMap<String, ExecutorVersion>>,
}

impl VersionCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, name: &str) -> Option<ExecutorVersion> {
        self.inner.lock().ok().and_then(|m| m.get(name).cloned())
    }

    pub fn put(&self, name: &str, version: ExecutorVersion) {
        if let Ok(mut m) = self.inner.lock() {
            m.insert(name.to_string(), version);
        }
    }

    pub fn clear(&self) {
        if let Ok(mut m) = self.inner.lock() {
            m.clear();
        }
    }
}

static VERSION_CACHE: std::sync::OnceLock<VersionCache> = std::sync::OnceLock::new();

pub fn global_version_cache() -> &'static VersionCache {
    VERSION_CACHE.get_or_init(VersionCache::new)
}

/// Reusable tokio command builder for subprocess invocation.
pub fn base_command(binary: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new(binary);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}
