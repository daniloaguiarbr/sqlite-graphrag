# ADR-0043: Five-Gap Remediation — Typed FallbackReason, Deterministic OAuth Fallback, Rate-Limit Headers, Bilíngue Read NotFound, dim 64

- **Status**: Accepted
- **Date**: 2026-06-17
- **Version**: v1.0.85 (resolves GAP-003, G58, G45-CR5, G55 docs, G56 docs)
- **Authors**: tech-lead

## Context

GAP-002 (v1.0.84, ADR-0042) split the Claude entry point so `--llm-backend claude` actually invokes Claude. Five related gaps remained open and are now consolidated into a single v1.0.85 release.

### GAP-003 — Slot semaphore timeout

`acquire_llm_slot_for_embedding` in `src/embedder.rs:289-317` blocks up to 30s when 8+ concurrent LLM subprocesses are active. The resulting `AppError::Embedding` is mapped to `FallbackReason::EmbeddingFailed(msg)` with discriminator `"embedding_failed"`, indistinguishable from quota exhaustion or a structural bug.

Production trace from `/tmp/claude-1000/.../tasks/b6ppfly55.output` (2026-06-17):

```
WARN hybrid_search: live embedding failed; falling back to FTS5
  fallback_reason=embedding failed: lock busy: failed to acquire LLM slot within 300s (max=8 concurrent)
```

### G58 — Non-deterministic fallback under OAuth fatigue

`recall` and `hybrid-search` in `src/commands/{recall,hybrid_search}.rs` fall back to FTS5-puro on any embedding error. The operator cannot tell whether the fallback is caused by quota exhaustion (recoverable via backend swap) or a structural bug.

### G45-CR5 — `anthropic-ratelimit-*` headers discarded

`LlmEmbedding::invoke_claude` in `src/extract/llm_embedding.rs:530-588` discards 12-14 `anthropic-ratelimit-*` headers returned by the `claude -p` subprocess. The operator never sees the rate-limit countdown and only discovers the outage when the subprocess returns exit 11.

### G55 — `read NotFound` lost the identifier

`AppError::NotFound(String)` previously discarded the name or id of the missing entity. Documented in v1.0.80 as `AppError::MemoryNotFound { name, namespace }` and `AppError::MemoryNotFoundById { id }` with bilíngue Display via `pt::memory_not_found` and `pt::memory_not_found_by_id`.

### Note — GAP-003 ID Overloaded

The gap ID `GAP-003` is used by two distinct gaps across releases:

- **GAP-003 (v1.0.82)** — `docs/decisions/adr-0038-llm-backend-user-choice.md` documents the LLM backend user choice
- **GAP-003 (v1.0.85)** — this ADR documents the slot semaphore timeout refinement (the `FallbackReason::SlotExhausted` discriminator)

When citing `GAP-003` in cross-references, append the version suffix (e.g., `GAP-003@1.0.85`) to disambiguate. Future ADRs should adopt this convention.

### G56 — `dim 384` was burning OAuth quota

Embedding with `dim=384` in codex (`gpt-5.5`) consumed ~6× more output tokens than `dim=64`. Default was reduced to 64 (MRL, arXiv 2205.13147) in v1.0.79.

## Decision

### 1. `FallbackReason` extends from 3 to 7 variants

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum FallbackReason {
    EmbeddingFailed(String),
    SlotExhausted,                                   // GAP-003
    OAuthQuota { backend: &'static str },             // G58, G45-CR5
    BackendMismatch { requested: &'static str, resolved: &'static str },
    DimZero,                                         // structural bug discriminator
    Cancelled,
    Timeout { operation: String, duration_secs: u64 },
}
```

`reason_code()` returns a stable string for each variant:
`"embedding_failed" | "slot_exhausted" | "oauth_quota" | "backend_mismatch" | "dim_zero" | "cancelled" | "timeout"`.

### 2. `classify_embedding_error` (pure function, no I/O)

Located in `src/embedder.rs:436-477`. Maps `AppError` to `FallbackReason` via lexical substring match on the error message — no retries, no telemetry, deterministic and `#[serial_test::serial(env)]`-safe.

### 3. `try_embed_query_with_deterministic_fallback` (G58)

In `src/embedder.rs:478-505`. On `OAuthQuota`, retries once with the alternative backend (codex ↔ claude). On `SlotExhausted`, sleeps 750 ms and retries once. On any other reason, returns immediately.

### 4. `acquire_llm_slot_for_embedding` (GAP-003)

When `crate::llm_slots::acquire_llm_slot` returns `AppError::LockBusy` with `wait_secs > 0`, the error is rewritten as `AppError::Embedding("slot exhausted: ...")`. `classify_embedding_error` then maps the substring to `FallbackReason::SlotExhausted`.

### 5. `LlmEmbedding::invoke_claude` (G45-CR5)

After `cmd.output()`, loop over `output.headers`. For every `anthropic-ratelimit-*-remaining` header, check if the value is `0`. When yes, return `AppError::Embedding("OAuth usage quota exhausted: {name}=0")` BEFORE checking the subprocess exit status — this lets `classify_embedding_error` map it to `OAuthQuota { backend: "claude" }`.

### 6. Validation gates

- `cargo check --workspace --all-targets` exit 0
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` exit 0
- `cargo nextest run --profile ci` 830+ tests
- `cargo llvm-cov nextest --profile ci --summary-only` ≥ 80%
- `cargo test --test embedder -- --ignored` hermetic env
- `--dry-run-backend` 4 backends return JSON

## Consequences

### Positive

- Operator can distinguish quota exhaustion from slot exhaustion from structural bugs via `vec_degraded_reason` discriminator
- Sessions under OAuth fatigue on codex transparently swap to claude before falling back to FTS5
- Rate-limit headers are now first-class signal — quota exhaustion is detected proactively, not after a non-zero exit
- Slot exhaustion has a 750 ms ceiling (was 30 s) before degrading to FTS5
- Backwards compatible: `FallbackReason::EmbeddingFailed(msg)` still works for unrecognized messages
- Bilíngue `read NotFound` messages preserved from v1.0.80
- `dim 64` default preserved from v1.0.79

### Negative

- Five call sites of `try_embed_query_with_choice` updated in hybrid_search.rs and recall.rs — patched atomically
- `classify_embedding_error` depends on substring matching of error messages — must be updated when error wording changes
- `try_embed_query_with_deterministic_fallback` adds up to 750 ms latency on `SlotExhausted` path
- Test count grows (5 new regression tests) — maintenance overhead

## Alternatives Considered

1. **Telemetry-only approach (rejected)**: add metrics fields to envelopes without changing `FallbackReason`. Rejected — does not solve the underlying problem of indistinguishability.
2. **Circuit breaker with rolling window (rejected for v1.0.85)**: `AtomicU64` with global counter. Rejected — added complexity without proportional benefit at v1.0.85 scope.
3. **Skip OAuth headers entirely (rejected)**: keep current behavior. Rejected — quota exhaustion happens without warning.
4. **Single big-bang refactor (rejected)**: merge all 5 gaps into one massive PR. Rejected — violates surgical-scope rule.

## References

- `src/embedder.rs:289-317` — `acquire_llm_slot_for_embedding`
- `src/embedder.rs:425-477` — `try_embed_query_with_fallback` + `classify_embedding_error`
- `src/embedder.rs:478-505` — `try_embed_query_with_deterministic_fallback`
- `src/commands/hybrid_search.rs:218-241` — call site update
- `src/commands/recall.rs:172-184` — call site update
- `src/extract/llm_embedding.rs:530-619` — `invoke_claude` with rate-limit headers
- `src/errors.rs:64-73` — `AppError::MemoryNotFound` / `MemoryNotFoundById`
- `src/errors.rs:355-365` — bilíngue Display
- `src/constants.rs:22` — `DEFAULT_EMBEDDING_DIM = 64`

## Related Decisions

- **ADR-0042 (v1.0.84)**: split Claude entry point — GAP-003 inherits from this architecture
- **ADR-0041 (v1.0.83)**: preserve custom provider env — enables G45-CR5 path for Anthropic-compatible gateways
- **ADR-0038 (v1.0.76)**: codex as default backend — G58 swap target
- **ADR-0034 (v1.0.80)**: SHUTDOWN resilience — GAP-003 cross-ref (slot exhaustion can mask SHUTDOWN)
