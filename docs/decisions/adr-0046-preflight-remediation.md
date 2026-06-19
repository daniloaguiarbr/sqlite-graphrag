# ADR-0046: Preflight Remediation — Audit Findings Fixup (v1.0.88)

- **Status**: Accepted
- **Data**: 2026-06-19
- **Versão**: v1.0.88 (closes GAP-META-005 followup)
- **Autores**: Danilo Aguiar <daniloaguiarbr@gmail.com>

## Context

ADR-0045 (v1.0.87) introduced `src/spawn/preflight.rs` exposing `preflight_check` plus 7 guards (`check_argv_size`, `check_binary_exists`, `check_output_buffer`, `check_mcp_config_inline`, `check_mcp_config_path`, `check_walkup_mcp_json`, `check_claude_config_dir`) consumed by the 4 LLM spawners (`claude_runner.rs`, `codex_spawn.rs`, `ingest_claude.rs`, `llm_embedding.rs`).

The post-release end-to-end audit (`audit-a2-graph-2026-06-18`) revealed **10 latent bugs** in the preflight layer and surrounding plumbing that ranged from CRITICAL (the dev sandbox was 100% broken by a too-aggressive config-dir guard) to LOW (variant information lost via `std::process::exit(16)` skipping structured error envelopes).

### The 10 audit findings

- **BUG-1 CRITICAL** — `check_claude_config_dir` rejected ANY non-empty directory, breaking 100% of dev calls. The fix: walk the directory and inspect `settings.json` semantically rather than checking the directory is empty. A populated `~/.claude/` with `settings.json` declaring zero MCP servers must pass the guard.

- **BUG-2 / BUG-3** — Spawners passed the literal string `--mcp-config '{}'` to `Command::arg()`. Claude Code 2.1.177+ rejected the inline JSON with "Invalid MCP configuration". The fix: introduce `write_empty_mcp_config_tempfile()` and substitute the literal across 3 spawner sites.

- **BUG-4 LOW** — `check_mcp_config_inline` only inspected `--mcp-config <PATH>` but not the `=`-form `--mcp-config=PATH`. Users running with the `=` form bypassed the path validation.

- **BUG-5 MEDIUM** — `check_claude_config_dir` short-circuited on the first non-empty entry without inspecting content. The fix: load `settings.json` (if present) and verify no MCP server declarations.

- **BUG-6 CRITICAL** — `build_claude_command` returned `Command` (infallible signature). Preflight failure called `std::process::exit(16)`, killing the CLI without surfacing a structured `AppError` envelope. The fix: introduce `From<PreFlightError> for AppError` and change `build_claude_command` to return `Result<Command, AppError>`.

- **BUG-7 HIGH** — `preflight_check` propagated `PreFlightError` directly to callers, who had no way to render it as JSON for `--json` consumers. The fix: `preflight_check` propagates `AppError::PreFlightFailed` directly via the new `From` impl.

- **BUG-9 LOW** — `check_walkup_mcp_json` accepted any JSON file named `.mcp.json`, even if its content was malformed. The fix: semantic validation that the JSON parses as `{ "mcpServers": ... }` shape.

- **BUG-10 MEDIUM** — `AppError::PreFlightFailed` was previously `String`-typed, losing the structured `PreFlightError` variant. The fix: shape changed to `Box<PreFlightError>` so the original variant is preserved through the error chain.

- **BUG-11 CRITICAL** — `src/embedder.rs` called `claude -p` without invoking `preflight_check`, bypassing all 7 guards. The fix: wrap the call site with `preflight_check` before `Command::spawn()`.

- **BUG-12 MEDIUM** — `src/output.rs:141` (`output::emit_error`) called BOTH `tracing::error!` AND `eprintln!` for the same violation, producing 2 stderr lines per OAuth-only enforcement trip.

## Decision

Consolidated remediation in v1.0.88:

1. **BUG-1 fix** — `check_claude_config_dir` now inspects `settings.json` semantically. A directory containing only `settings.json` (with zero MCP server declarations) is accepted. The directory-must-be-empty heuristic is removed.

2. **BUG-2/3 fix** — `write_empty_mcp_config_tempfile()` writes `{"mcpServers":{}}` to a tempfile via `tempfile::persist()` and returns the path. All 3 spawner sites (`claude_runner.rs`, `ingest_claude.rs`, `llm_embedding.rs`) substitute the literal `'{}'` for the tempfile path.

3. **BUG-4 fix** — `check_mcp_config_inline` now parses `--mcp-config=...` (single token, `=` form) and `--mcp-config <PATH>` (two tokens) symmetrically. Both forms route to `check_mcp_config_path` when the value is a non-empty path.

4. **BUG-5 fix** — `check_claude_config_dir` walks the directory; for each `*.json` file present, attempts `serde_json::from_str::<Settings>()` and checks `mcp_servers` is empty or absent.

5. **BUG-6 fix** — `From<PreFlightError> for AppError` added. `build_claude_command` returns `Result<Command, AppError>`. Callers receive `AppError::PreFlightFailed(_)` and render the structured envelope via `output::emit_error`.

6. **BUG-7 fix** — `preflight_check` internally maps `PreFlightError` to `AppError::PreFlightFailed` via the new `From` impl. Callers see only `AppError` variants.

7. **BUG-9 fix** — `check_walkup_mcp_json` now parses `.mcp.json` and verifies the schema is `{ "mcpServers": { ... } }`. Files with malformed shape are rejected with `WalkUpMcpJsonInvalid`.

8. **BUG-10 fix** — `AppError::PreFlightFailed` is now `Box<PreFlightError>` (the variant shape itself). Lossy `String` payload removed.

9. **BUG-11 fix** — `src/embedder.rs` now calls `preflight_check` before `Command::spawn()` in the LLM embedding pipeline. The guard invocation is identical to the other 3 spawners.

10. **BUG-12 fix** — `src/output.rs:141` (`output::emit_error`) drops the redundant `eprintln!` and keeps only `tracing::error!`. Stderr now emits exactly 1 line per error.

### Caller migration (3 spawner sites)

| Site | Before | After |
|------|--------|-------|
| `claude_runner.rs:255` | `let cmd = build_claude_command(...); preflight_check(...).unwrap_or_else(\|e\| std::process::exit(16)); cmd.spawn()` | `let cmd = build_claude_command(...)?; preflight_check(...)?; cmd.spawn()` |
| `ingest_claude.rs:297` | `cmd.arg("--mcp-config").arg("{}")` | `let path = write_empty_mcp_config_tempfile()?; cmd.arg("--mcp-config").arg(path)` |
| `llm_embedding.rs:670` | bypassed preflight entirely | `preflight_check(...)?; cmd.spawn()` |

## Consequences

### Positive

- 9 integration tests restored (previously masked by the dev-sandbox breakage): `entity_validation`, `graph_traverse`, `recall_distance`, and 6 others
- 0 regressions in `cargo test --lib` (833+ passed)
- 5 sites using `std::process::exit(16)` removed — all replaced by `?` propagation
- 3 spawner sites use `write_empty_mcp_config_tempfile()` instead of inline JSON
- `AppError::PreFlightFailed(Box<PreFlightError>)` is fully structured end-to-end
- stderr emits 1 line per OAuth-only enforcement violation (was 2)

### Negative

- `build_claude_command` signature changed from `-> Command` to `-> Result<Command, AppError>`. All 4 call sites updated.
- `AppError::PreFlightFailed` shape change is a breaking change for downstream consumers parsing the variant. Mitigation: the variant retains the same `Display` impl, only the internal payload type changed.
- 1 new tempfile per spawn in the MCP-config-inline case. Acceptable for jobs that already take seconds.

## Known Tech Debt (v1.0.89+)

- `src/commands/enrich.rs` is 4116 lines. Modularization into `enrich/queue.rs`, `enrich/extraction.rs`, `enrich/postprocess.rs` is planned for v1.0.89.
- `AppError::Embedding(String)` is stringly-typed. Subtyping into `EmbeddingBackendUnavailable`, `EmbeddingTimeout`, `EmbeddingResponseMalformed` is planned via ADR-0048 (proposed in v1.0.89).
- `tests/integration.rs` is 2367 lines. Division into suites specific to embeddings, graph traversal, recall, and CRUD is planned for v1.0.89.
- `preflight_check` returns unit; future work may return a structured report (timings per guard) for `health --json` counters.

## Cross-references

- ADR-0045 (preflight validation layer original, v1.0.87)
- ADR-0011 (OAuth-only enforcement — BUG-12 fix targets stderr deduplication)
- ADR-0040 (stderr capture fallback chain — BUG-12 is orthogonal but adjacent)
- ADR-0041 (preserve custom-provider env — preflight must not clear `CLAUDE_CONFIG_DIR` for legitimate providers)
- ADR-0042 (claude backend split — 3 of the spawner sites live in modules split by this ADR)
- `audit-a2-graph-2026-06-18` (the audit that surfaced BUG-1..12)
- `gaps.md` (GAP-META-005 closed via this remediation)
- `src/spawn/preflight.rs:1` (the helper consumed by all 4 spawners)
- `src/error.rs` (`AppError::PreFlightFailed(Box<PreFlightError>)`)

## Non-goals (YAGNI)

- NO refactor of the 4 spawners into a single abstraction beyond the preflight hook
- NO introduction of async preflight (synchronous 1ms cost is acceptable)
- NO change to `SQLITE_GRAPHRAG_SKIP_PREFLIGHT=1` semantics
- NO new exit codes (16 remains the only preflight exit, but now reached via `AppError` rendering)

## Next steps

- v1.0.89: modularize `src/commands/enrich.rs` (4116 lines)
- v1.0.89: ADR-0048 (EmbeddingErrorKind subtyping)
- v1.0.89: split `tests/integration.rs` (2367 lines) into focused suites
- v1.0.90: preflight counters in `health --json`
