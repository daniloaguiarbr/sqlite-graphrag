# ADR-007: Retry Policy Architecture


## Status
- Accepted (2026-05-31)


## Context
- The CLI performs operations that can transiently fail in three domains
- SQLite concurrency (SQLITE_BUSY/SQLITE_LOCKED)
- LLM rate-limiting (Claude Code / Codex subprocess returns 429)
- File-lock contention (CLI slot semaphore)
- Each domain has distinct latency characteristics requiring separate policies
- Previous implementation had retry logic duplicated across 4 files without centralization


## Decision
### Infrastructure
- Centralized `RetryConfig` struct in `src/retry.rs` with named constructors per domain
- Half-jitter formula: `delay = base/2 + fastrand::u64(0..base/2)` producing [base/2, base)
- Kill switch via `SQLITE_GRAPHRAG_DISABLE_RETRY=1` env var
- No external crate adopted

### Justification for No External Crate
- `backon` is async-only (requires tokio runtime) — CLI is synchronous
- `backoff` crate adds transitive dependencies for 3 simple retry loops
- Total implementation is ~120 LOC with full test coverage
- Per rules §16 L778: "NUNCA reimplementar quando crate resolve" — justified exception: no sync crate resolves without overhead

### Policies

| Domain | Base Delay | Max Delay | Max Attempts | Deadline | Jitter |
|--------|-----------|-----------|--------------|----------|--------|
| SQLite BUSY | 300ms | 4800ms | 5 | 30s | Half |
| LLM rate-limit | 60s | 900s | 20 | 3600s (1h) | Half |
| Cold-start | 2s | 4s | 2 | 30s | None |
| File-lock poll | 500ms | 2000ms | deadline-based | configurable | Progressive |

### Observability
- `tracing::debug` per attempt with `attempt`, `delay_ms`, `error_kind`
- `tracing::error` on exhaustion with total elapsed time
- Structured fields per §12 L619-654 of rules_rust_retry_com_backoff.md

### Error Classification
- `is_retryable()` returns true for: DbBusy, LockBusy, AllSlotsFull, LowMemory, RateLimited, Timeout
- `is_permanent()` returns true for: Validation, BinaryNotFound, Duplicate, NotFound, NamespaceError, LimitExceeded, VecExtension
- Unclassified (Database, Io, Internal, Json): neither retryable nor permanent — caller decides

### Kill Switch
- Env var `SQLITE_GRAPHRAG_DISABLE_RETRY=1` disables all retry loops immediately
- Checked at the top of each retry loop iteration
- Logs `tracing::warn` when active to ensure visibility in telemetry
- Use case: emergency incident response to prevent retry storms


## Consequences
- All retry behavior is documented and auditable
- Policy changes require only modifying `RetryConfig` constructors in `src/retry.rs`
- Kill switch allows instant disable during incidents without process restart
- Half-jitter prevents thundering herd in parallel worker scenarios
- Deadline total prevents indefinite blocking on persistent rate-limiting
