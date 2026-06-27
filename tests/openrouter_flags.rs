//! CLI-level validation tests for the v1.0.95 `--mode openrouter` JUDGE.
//!
//! These exercise the argument-validation layer of `enrich` end to end
//! through the real binary (via `assert_cmd`), without touching the network
//! or a database: both checks fire *before* any DB or HTTP work in `run()`.

use assert_cmd::Command;
use predicates::str::contains;

/// `--mode openrouter` without `--openrouter-model` must fail fast with exit
/// code 1 (AppError::Validation) and name the missing flag. The model check
/// runs before any API-key or DB access, so the outcome does not depend on
/// `OPENROUTER_API_KEY` being present in the environment.
#[test]
fn openrouter_mode_requires_model_flag() {
    Command::cargo_bin("sqlite-graphrag")
        .expect("binary builds")
        .args([
            "enrich",
            "--operation",
            "memory-bindings",
            "--mode",
            "openrouter",
        ])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("openrouter-model"));
}

/// Cross-provider flags (`--claude-binary`, etc.) are rejected when
/// `--mode openrouter` is selected. The G20 conflict check runs at the very
/// top of `run()` — before the model/API-key checks — so passing a model is
/// not required to trigger it, and the result is deterministic regardless of
/// environment.
#[test]
fn openrouter_mode_rejects_crossed_claude_flag() {
    Command::cargo_bin("sqlite-graphrag")
        .expect("binary builds")
        .args([
            "enrich",
            "--operation",
            "memory-bindings",
            "--mode",
            "openrouter",
            "--openrouter-model",
            "deepseek/deepseek-v4-flash",
            "--claude-binary",
            "/usr/bin/true",
        ])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("claude-binary"))
        .stderr(contains("openrouter"));
}

/// A codex flag crossed into `--mode openrouter` is likewise rejected.
#[test]
fn openrouter_mode_rejects_crossed_codex_flag() {
    Command::cargo_bin("sqlite-graphrag")
        .expect("binary builds")
        .args([
            "enrich",
            "--operation",
            "memory-bindings",
            "--mode",
            "openrouter",
            "--openrouter-model",
            "z-ai/glm-5.2",
            "--codex-model",
            "gpt-5.4-mini",
        ])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("codex-model"));
}
