//! Error propagator for subprocess invocations (v1.0.75 — G22 P16/P17)

use crate::errors::AppError;
use std::process::Output;

/// Captures the exit code, stdout and stderr of a subprocess and converts it
/// into a structured `AppError`. The previous behaviour in
/// `src/commands/codex_spawn.rs` swallowed stderr; this propagates it.
pub struct ErrorPropagator {
    pub binary: String,
    pub args: Vec<String>,
}

impl ErrorPropagator {
    pub fn new(binary: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            binary: binary.into(),
            args,
        }
    }

    /// Convert a non-zero exit into a descriptive AppError including stderr.
    pub fn propagate(&self, output: &Output) -> Result<(), AppError> {
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let code = output.status.code().unwrap_or(-1);
        let mut msg = format!(
            "{} exited with code {}",
            self.binary,
            code
        );
        if !stderr.trim().is_empty() {
            msg.push_str(&format!("\nstderr: {}", stderr.trim()));
        }
        if !stdout.trim().is_empty() {
            msg.push_str(&format!("\nstdout: {}", stdout.trim()));
        }
        msg.push_str(&format!("\nargs: {}", self.args.join(" ")));
        Err(AppError::Internal(anyhow::anyhow!(msg)))
    }

    /// Returns the parsed stdout if exit code is 0, else propagates.
    pub fn require_success(&self, output: &Output) -> Result<String, AppError> {
        self.propagate(output)?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}
