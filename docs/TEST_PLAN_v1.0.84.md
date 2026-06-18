# TEST PLAN v1.0.84 — GAP-002 Claude Backend Split

## Scope

ADR-0042 introduces the real split of the Claude entry point in `src/embedder.rs`. Five regression tests in `tests/embedder.rs` cover:

1. `embed_via_backend_claude_does_not_invoke_codex` — `--llm-backend claude` invokes claude, not codex
2. `embed_via_backend_codex_does_not_invoke_claude` — `--llm-backend codex` invokes codex, not claude
3. `embed_via_backend_none_returns_empty_vector` — `--llm-backend none` skips embedding entirely
4. `cli_dry_run_backend_prints_resolved_path` — `--dry-run-backend` exits 0 with JSON envelope
5. `claude_invocation_uses_isolated_config_dir` — `CLAUDE_CONFIG_DIR` is empty/isolated per ADR-0010

## Test Environment

- All five tests gated by `#[serial_test::serial(env)]` to prevent PATH pollution
- Mock LLM scripts in TempDir shadow PATH: `claude` and `codex` are bash scripts that dump argv and exit 0
- `SQLITE_GRAPHRAG_DB_PATH` and `SQLITE_GRAPHRAG_CLAUDE_EMBED_MODEL` env vars per test

## Expected Results

945 tests total (818 pre-existing + 5 new v1.0.84 + 0 new v1.0.85 yet). All green via `cargo nextest -P ci`. Coverage: 100% of `embed_via_backend` arms; 90%+ on `LlmEmbeddingBuilder`.

## Cross-refs

- ADR-0042 (`docs/decisions/adr-0042-claude-backend-split.md`)
- `tests/embedder.rs` (5 new tests)
- `src/embedder.rs:190+` (`embed_via_claude_local`)
- `src/extract/llm_embedding.rs:232+` (`LlmEmbeddingBuilder`)
- `src/spawn/env_whitelist.rs` (`apply_env_whitelist_for_claude`)
