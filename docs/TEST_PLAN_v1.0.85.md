# TEST PLAN v1.0.85 — Five-Gap Remediation (ADR-0043)

## Scope

ADR-0043 introduces the 7-variant `FallbackReason` enum. Five regression tests in `tests/embedder.rs` cover:

1. `slot_exhaustion_returns_typed_error` — GAP-003: `acquire_llm_slot_for_embedding` returns `reason_code: "slot_exhausted"` after 750ms backoff ceiling
2. `oauth_quota_fallback_deterministic` — G58: `try_embed_query_with_deterministic_fallback` retries on `OAuthQuota` and propagates `reason_code` to `vec_degraded_reason`
3. `anthropic_ratelimit_headers_captured` — G45-CR5: 12-14 `anthropic-ratelimit-*-remaining` headers parsed; `0` aborts embed and triggers codex fallback
4. `read_notfound_preserves_identifier` — G55 docs: bilingual message preserves the identifier (name or id) and namespace
5. `embedding_dim_reduces_token_cost` — G56: dim=64 consumes ≤1/6 of OAuth tokens vs dim=384

## Test Environment

- All five tests gated by `#[serial_test::serial(env)]`
- Mock HTTP server for `claude -p` returning synthetic `anthropic-ratelimit-*-remaining: 0` headers
- `SQLITE_GRAPHRAG_EMBEDDING_DIM=64` env var for G56 test

## Expected Results

945 tests total (818 pre-existing + 5 new v1.0.84 + 5 new v1.0.85 + 0 new hotfixes yet). All green via `cargo nextest -P ci`. Coverage: 100% of `FallbackReason` variants; 90%+ on `try_embed_query_with_deterministic_fallback`.

## Cross-refs

- ADR-0043 (`docs/decisions/adr-0043-five-gap-remediation.md`)
- ADR-0042 (`docs/decisions/adr-0042-claude-backend-split.md`)
- `src/embedder.rs:284-298` (FallbackReason enum + reason_code)
- `src/commands/hybrid_search.rs` and `src/commands/recall.rs` (fallback path)
- `src/extract/llm_embedding.rs:460-482` (`invoke_claude` header capture)
- `src/commands/read.rs` (bilingual MemoryNotFound)
