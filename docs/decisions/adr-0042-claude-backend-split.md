# ADR-0042: Real Split of the Claude Entry Point in the Embedder

- **Status**: Accepted
- **Date**: 2026-06-17
- **Version**: v1.0.84 (resolves GAP-002)
- **Authors**: tech-lead

## Context

The CLI flag `--llm-backend claude` is accepted by the parser (`src/cli.rs:149-150`) and propagated as `cli.llm_backend: LlmBackendChoice` to six commands in `src/main.rs:310-379`. The method `LlmBackendChoice::to_chain()` in `src/cli.rs:36-59` translates `Claude` into `vec![LlmBackendKind::Claude, LlmBackendKind::None]`. The chain is iterated by `embed_with_fallback` in `src/embedder.rs:368-409`, which calls `embed_via_backend` in `src/embedder.rs:427-446`. **Root cause**: the `LlmBackendKind::Claude` match arm (lines 435-444) delegates to `embed_passage_local` (lines 177-181), which calls `get_embedder` (lines 128-135), which uses `LlmEmbedding::detect_available` (lines 184-207) — that function performs a PATH-probe **preferring `codex` FIRST** (line 187) and only falls back to `claude` when `codex` is absent. The code comment (lines 441-443) explicitly states "future v1.0.83 will split the entry points", but the split was never implemented.

### Root cause matrix

| Factor | Location | Line range |
|---|---|---|
| `LlmBackendKind::Claude` arm delegates to `embed_passage_local` | `src/embedder.rs` | 435-444 |
| `embed_passage_local` re-runs PATH-probe | `src/embedder.rs` | 177-181 |
| `detect_available` prefers `codex` over `claude` | `src/extract/llm_embedding.rs` | 184-207 |
| `with_claude` exists but is never invoked by the chain | `src/extract/llm_embedding.rs` | 221-231 |
| Stale comment "future v1.0.83 will split" | `src/embedder.rs` | 441-443 |

### Production impact (2026-06-17)

When an operator passes `--llm-backend claude` expecting a deterministic bypass of `codex`:

1. `embed_with_fallback` reaches the `LlmBackendKind::Claude` arm
2. The arm delegates to `embed_passage_local` instead of a Claude-only path
3. `get_embedder` invokes `LlmEmbedding::detect_available`
4. PATH-probe prefers `codex` because it sits first on PATH
5. `codex` OAuth quota exhausted → exit 11 (`AppError::Embedding`)
6. `remember`/`edit` abort before SQLite persistence completes
7. Partial memory row remains in `memories` without a vector in `memory_embeddings`
8. Orphan entry grows in `pending_embeddings` on every retry
9. `recall` and `hybrid-search` lose semantic precision

### Cross-references

- `gap-g58-recall-sem-fallback-deterministic-2026-06-13` — recall and hybrid-search under OAuth fatigue
- `incident-codex-oauth-refresh-token-reused-2026-06-14` — refresh-token 401 chain
- ADR-0038 — codex as default backend since v1.0.76
- ADR-0041 — preservation of custom-provider env (ADR-0042 is the symmetric fix for the Claude entry point)

## Decision

Split the Claude entry point in the embedder so `--llm-backend claude` invokes `claude` and never `codex`. The split has four concrete pieces.

### 1. New builder `LlmEmbeddingBuilder` in `src/extract/llm_embedding.rs`

Expose `with_claude_builder()` and `with_codex_builder()` constructors that return a builder with `override_binary(PathBuf)` and `override_model(String)` setters. The existing `with_claude` and `with_codex` constructors become thin wrappers that call `.build()` on the builder, eliminating duplication.

### 2. New `get_claude_embedder` and `embed_via_claude_local` in `src/embedder.rs`

`get_claude_embedder` caches a `OnceLock<Mutex<LlmEmbedding>>` that is built only via `LlmEmbedding::with_claude_builder()`. The cache never touches `detect_available`, so codex cannot enter the resolution path.

`embed_via_claude_local` is the public function called by the new match arm. It acquires the LLM slot, calls `get_claude_embedder`, and runs `embed_passage`. It honors `claude_binary` and `claude_model` overrides from CLI flags.

### 3. Match-arm swap in `embed_via_backend` at `src/embedder.rs:435-444`

Replace the delegation to `embed_passage_local` with a direct call to `embed_via_claude_local`. Remove the stale "synonym for codex" comment. The arm now logs a `tracing::debug!` event with `backend = "claude"` so operators can confirm the fix in production logs.

### 4. Observability via `backend_invoked` in seven envelopes

Add a `backend_invoked: enum [claude, codex, none]` field to seven response envelopes: `embedding status`, `remember`, `edit`, `ingest` (summary), `recall`, `hybrid-search`, `enrich` (summary). The field is omitted when the operation did not invoke any backend.

For `recall` and `hybrid-search`, also add `vec_degraded_reason: enum [embedding_failed, cancelled, timeout]` so consumers can disambiguate why live embedding fell back to FTS5.

### 5. New `--dry-run-backend` global flag

Resolve and print the backend that would be invoked (binary path, model, flavour, env-clear mode) without spawning the subprocess. Returns exit 0. Honors env var `SQLITE_GRAPHRAG_DRY_RUN_BACKEND=1`.

### Serialization helpers

- `LlmBackendKind::as_str(self) -> &'static str` returns `"claude"`, `"codex"`, or `"none"`
- `FallbackReason::reason_code(&self) -> &'static str` returns `"embedding_failed"`, `"cancelled"`, or `"timeout"`

## Consequences

### Positive

- `--llm-backend claude` honors its UX promise: binary `claude` is invoked, never `codex`
- Codex OAuth exhaustion no longer blocks sessions that explicitly opt into Claude
- `backend_invoked` field gives operators and CI pipelines per-call observability
- `--dry-run-backend` enables pre-flight audit before long ingestion runs
- `vec_degraded_reason` replaces the free-form `vec_error` with an enum, enabling structured alerting
- 5 new regression tests in `tests/embedder.rs` lock the contract

### Negative

- `embed_passage_with_choice` changes signature from `Vec<f32>` to `(Vec<f32>, LlmBackendKind)` — patch-additive per library API policy, six call sites updated atomically
- Seven JSON schemas updated; consumers must tolerate the new optional fields
- ADR-0042 introduces a conceptual dependency on ADR-0034 (SHUTDOWN), ADR-0037 (locale rename), ADR-0038 (backend default), and ADR-0041 (custom env)

### No new telemetry

The fix is silent. No `tracing::info!` is added for custom-provider usage. The only new log event is the `tracing::debug!` inside the new Claude match arm, gated by default log level. This is a deliberate decision to minimize observability surface on sensitive paths.

## Alternatives Considered

1. **Keep the "synonym for codex" shortcut** — rejected, the comment itself documents this as a future fix and the bypass is the exact GAP-002 defect
2. **Reverse `detect_available` to prefer `claude`** — rejected, breaks the `Auto` resolution order codified by ADR-0038
3. **Add a separate `--force-claude` flag** — rejected, creates two ways to ask for the same thing and confuses operators
4. **Document the workaround via external `claude -p` headless** — rejected, offloads operational burden to users and breaks the deterministic CLI contract

## References

- `src/embedder.rs:435-444` — `LlmBackendKind::Claude` match arm, now calling `embed_via_claude_local`
- `src/embedder.rs:177-181` — `embed_passage_local` (no longer reached for Claude backend)
- `src/embedder.rs:128-135` — `get_embedder` (no longer reached for Claude backend)
- `src/extract/llm_embedding.rs:184-207` — `detect_available` codex-first PATH-probe
- `src/extract/llm_embedding.rs:221-231` — `with_claude` refactored via `LlmEmbeddingBuilder`
- `src/spawn/env_whitelist.rs` — `apply_env_whitelist_for_claude` shared by `invoke_claude` and `embed_via_claude_local`
- ADR-0038 — codex as default backend since v1.0.76
- ADR-0041 — preservation of `ANTHROPIC_AUTH_TOKEN` and custom-provider env vars
- `gap-g58-recall-sem-fallback-deterministic-2026-06-13` — OAuth fatigue on read path
- `incident-codex-oauth-refresh-token-reused-2026-06-14` — refresh-token 401 chain

## Related Decisions

- **ADR-0034 (v1.0.80)** — SHUTDOWN resilience; GAP-002 inherits the A1 incident surface
- **ADR-0037 (v1.0.81)** — locale rename; orthogonal to backend split
- **ADR-0038 (v1.0.76)** — codex as default backend; ADR-0042 is the symmetric Claude entry-point split
- **ADR-0041 (v1.0.83)** — custom-provider env preservation; enables Claude OAuth on non-Anthropic endpoints that GAP-002 sessions depend on
