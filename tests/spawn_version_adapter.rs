//! Tests for the G22 VersionAdapter trait and concrete adapters.

use sqlite_graphrag::spawn::{
    claude_adapter::ClaudeAdapter, codex_adapter::CodexAdapter, compat_matrix,
    executor_version::ExecutorVersion, opencode_adapter::OpencodeAdapter, CompatMode,
    VersionAdapter,
};

#[tokio::test]
async fn codex_137_caps_removed_old_flag() {
    let v = ExecutorVersion::parse("0.137.0").unwrap();
    let caps = compat_matrix::codex_capabilities(&v);
    assert!(!caps.supports_ask_for_approval_flag);
    assert!(caps
        .removed_flags
        .contains(&"--ask-for-approval".to_string()));
}

#[tokio::test]
async fn codex_130_caps_keep_old_flag() {
    let v = ExecutorVersion::parse("0.130.0").unwrap();
    let caps = compat_matrix::codex_capabilities(&v);
    assert!(caps.supports_ask_for_approval_flag);
}

#[tokio::test]
async fn codex_adapter_name() {
    let adapter = CodexAdapter;
    assert_eq!(adapter.name(), "codex");
}

#[tokio::test]
async fn codex_adapter_capabilities_match_matrix() {
    let adapter = CodexAdapter;
    let v = ExecutorVersion::parse("0.137.0").unwrap();
    let caps = adapter.capabilities_for(&v);
    assert!(!caps.supports_ask_for_approval_flag);
}

#[tokio::test]
async fn codex_adapter_build_args_include_exec() {
    let adapter = CodexAdapter;
    let v = ExecutorVersion::parse("0.137.0").unwrap();
    let caps = adapter.capabilities_for(&v);
    let args = adapter.build_args("hello world", &caps, CompatMode::Auto);
    assert!(args.contains(&"exec".to_string()));
    assert!(args.contains(&"hello world".to_string()));
}

#[tokio::test]
async fn codex_adapter_parse_jsonl() {
    let adapter = CodexAdapter;
    let raw = r#"{"type":"message","content":"hello"}
{"type":"tool","name":"foo"}"#;
    let parsed = adapter.parse_output(raw, "", 0);
    assert_eq!(parsed.items.len(), 2);
    assert_eq!(parsed.exit_code, 0);
}

#[tokio::test]
async fn claude_adapter_name() {
    let adapter = ClaudeAdapter;
    assert_eq!(adapter.name(), "claude");
}

#[tokio::test]
async fn claude_adapter_capabilities_for_v2() {
    let adapter = ClaudeAdapter;
    let v = ExecutorVersion::parse("2.1.0").unwrap();
    let caps = adapter.capabilities_for(&v);
    assert!(caps.supports_ask_for_approval_flag);
    assert!(caps.supports_mcp_map);
}

#[tokio::test]
async fn claude_adapter_build_args_uses_dash_p() {
    let adapter = ClaudeAdapter;
    let v = ExecutorVersion::parse("2.1.0").unwrap();
    let caps = adapter.capabilities_for(&v);
    let args = adapter.build_args("test prompt", &caps, CompatMode::Auto);
    assert!(args.contains(&"-p".to_string()));
    assert!(args.contains(&"test prompt".to_string()));
}

#[tokio::test]
async fn opencode_adapter_name() {
    let adapter = OpencodeAdapter;
    assert_eq!(adapter.name(), "opencode");
}

#[tokio::test]
async fn opencode_adapter_capabilities() {
    let adapter = OpencodeAdapter;
    let v = ExecutorVersion::parse("0.5.0").unwrap();
    let caps = adapter.capabilities_for(&v);
    assert!(caps.supports_mcp_map);
    assert!(!caps.supports_strict_schema);
}

#[tokio::test]
async fn opencode_adapter_build_args() {
    let adapter = OpencodeAdapter;
    let v = ExecutorVersion::parse("0.5.0").unwrap();
    let caps = adapter.capabilities_for(&v);
    let args = adapter.build_args("hello", &caps, CompatMode::Auto);
    assert!(args.contains(&"headless".to_string()));
    assert!(args.contains(&"hello".to_string()));
}

#[test]
fn compat_mode_parse() {
    assert_eq!(CompatMode::parse("strict"), CompatMode::Strict);
    assert_eq!(CompatMode::parse("LENIENT"), CompatMode::Lenient);
    assert_eq!(CompatMode::parse("auto"), CompatMode::Auto);
    assert_eq!(CompatMode::parse("random"), CompatMode::Auto);
}
