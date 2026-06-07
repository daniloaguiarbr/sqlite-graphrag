//! Executor compatibility matrix (v1.0.75 — G22)
//!
//! Static map of which versions support which flags, used by the adapters.

use super::ExecutorCapabilities;
use crate::spawn::executor_version::ExecutorVersion;

pub fn codex_capabilities(version: &ExecutorVersion) -> ExecutorCapabilities {
    let mut caps = ExecutorCapabilities::empty();
    caps.supports_strict_schema = true;

    if version.is_at_least(0, 137, 0) {
        caps.supports_mcp_map = true;
        caps.supports_ask_for_approval_flag = false;
        caps.default_flags
            .extend(["-a".to_string(), "never".to_string()]);
        caps.removed_flags.push("--ask-for-approval".to_string());
    } else if version.is_at_least(0, 134, 0) {
        // PATCH 2026-06-07: codex CLI 0.134.0 removed BOTH --ask-for-approval AND -a.
        // Approvals are now controlled via --dangerously-bypass-approvals-and-sandbox.
        // We skip the approval flag entirely (sandbox=read-only is already strict).
        caps.supports_mcp_map = true;
        caps.supports_ask_for_approval_flag = false;
        caps.removed_flags.push("--ask-for-approval".to_string());
        caps.removed_flags.push("-a".to_string());
    } else if version.is_at_least(0, 130, 0) {
        caps.supports_mcp_map = false;
        caps.supports_ask_for_approval_flag = true;
        caps.default_flags
            .push("--ask-for-approval=never".to_string());
    } else {
        caps.supports_ask_for_approval_flag = true;
        caps.default_flags
            .push("--ask-for-approval=never".to_string());
    }

    caps
}

pub fn claude_capabilities(version: &ExecutorVersion) -> ExecutorCapabilities {
    let mut caps = ExecutorCapabilities::empty();
    caps.supports_strict_schema = true;
    caps.supports_mcp_map = true;

    if version.is_at_least(2, 0, 0) {
        caps.supports_ask_for_approval_flag = true;
        caps.default_flags
            .extend(["--output-format".to_string(), "json".to_string()]);
    } else {
        caps.default_flags.push("--output-format=json".to_string());
    }
    caps
}

pub fn opencode_capabilities(_version: &ExecutorVersion) -> ExecutorCapabilities {
    let mut caps = ExecutorCapabilities::empty();
    caps.supports_mcp_map = true;
    caps.supports_ask_for_approval_flag = true;
    caps.supports_strict_schema = false;
    caps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_137_removed_old_flag() {
        let v = ExecutorVersion::parse("0.137.0").unwrap();
        let caps = codex_capabilities(&v);
        assert!(!caps.supports_ask_for_approval_flag);
        assert!(caps
            .removed_flags
            .contains(&"--ask-for-approval".to_string()));
    }

    #[test]
    fn codex_130_supports_old_flag() {
        let v = ExecutorVersion::parse("0.130.0").unwrap();
        let caps = codex_capabilities(&v);
        assert!(caps.supports_ask_for_approval_flag);
    }
}
