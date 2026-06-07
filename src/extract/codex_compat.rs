//! Codex 0.134+ removed --ask-for-approval and -a. Detect version once
//! at startup and cache the result.
use std::sync::OnceLock;

static CODEX_SUPPORTS_ASK_FOR_APPROVAL: OnceLock<bool> = OnceLock::new();

/// Returns true if the installed `codex` binary still accepts
/// `--ask-for-approval` (versions < 0.134.0).
pub fn codex_supports_ask_for_approval() -> bool {
    *CODEX_SUPPORTS_ASK_FOR_APPROVAL.get_or_init(detect_codex_supports_ask_for_approval)
}

fn detect_codex_supports_ask_for_approval() -> bool {
    let output = std::process::Command::new("codex")
        .arg("--version")
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output();
    let Ok(out) = output else {
        return true; // assume supported on probe failure
    };
    if !out.status.success() {
        return true;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Expected output: "codex-cli 0.134.0" or "codex-cli 0.135.0-beta".
    // Parse the version string and check the minor.
    let version_str = stdout.trim();
    let semver_part = version_str
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .split('-')
        .next()
        .unwrap_or("");
    let mut parts = semver_part.split('.');
    let major: u64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor: u64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    // Codex 0.134.0 removed --ask-for-approval.
    if major == 0 && minor >= 134 {
        return false;
    }
    true
}

