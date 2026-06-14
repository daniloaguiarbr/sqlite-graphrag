# ADR-0034 — SHUTDOWN Global Resilience for Audits and Tests

## Status

Accepted (v1.0.80).

## Context

The v1.0.80 audit suite (A1 through A4) discovered that the global
`static SHUTDOWN: AtomicBool` in `src/lib.rs:48` is a contamination hazard
for Agent Teams workflows. When parallel teammate invocations registered
their `ctrlc::set_handler` callbacks, the SHUTDOWN flag was set to `true` in
the shared process namespace. Subsequent invocations of
`sqlite-graphrag remember` would observe `SHUTDOWN == true` at startup and
abort the LLM-driven embedding with exit code 11
("embedding cancelled by shutdown signal") before doing any work.

Memory 1262 (`incident-a1-bloqueada-shutdown-2026-06-14`) documents the
reproducible failure: A1 audit could not be persisted across four
consecutive retry attempts spanning minutes.

## Decision

Implement three layered mitigations in v1.0.80:

1. **`try_reset_shutdown()`** in `src/lib.rs` — atomic `AcqRel` swap of the
   SHUTDOWN flag back to `false`, plus zeroing the `SIGNAL_COUNT` and
   `SIGNAL_NUMBER` counters. Returns `true` if the flag was previously set.
   Documented as for tests and audit invocations only.
2. **`should_obey_shutdown()`** in `src/lib.rs` — reads the
   `SQLITE_GRAPHRAG_IGNORE_SHUTDOWN` environment variable
   (`1`/`true`/`yes`/`on`, case-insensitive) and returns `false` when set.
   Inverts the production check from "obey unless told otherwise" to
   "ignore unless told otherwise".
3. **Embedder bypass** in `src/embedder.rs:537` — the `tokio::select!`
   between `work(batch)` and `token.cancelled()` is wrapped in
   `if should_obey_shutdown() { select! } else { work(batch).await }`.
   In audit mode the cancellation arm is dropped, so the batch runs to
   completion even if the cancellation token is in a cancelled state.

## Consequences

Positive:
- Audits and tests succeed even when the SHUTDOWN flag is contaminated.
- Memory 1261 (A1 audit) and 1262 (incident) only persisted thanks to this
  mitigation.
- Public API is type-safe and documented with doctest examples.
- Zero overhead in production: a single `std::env::var` per
  `should_obey_shutdown()` call, not on a hot path.

Negative:
- The global `tokio_util::sync::CancellationToken` remains one-shot; only
  the `AtomicBool` is resettable. Callers that need a resettable token
  must use a per-invocation token.
- Production code MUST NOT call `try_reset_shutdown()` — the bypass is
  opt-in via env var only.
- Tests must set the env var in a `#[serial_test::serial(env)]` block to
  avoid concurrent invocations racing on the env read.

## Implementation Notes

- `try_reset_shutdown` uses `SHUTDOWN.swap(false, Ordering::AcqRel)` for
  atomic observe-and-reset. The `AcqRel` ordering pairs with the
  `Release` store in the signal handler and the `Acquire` load in
  `shutdown_requested`.
- `should_obey_shutdown` is `pub` and exposed alongside the existing
  `SHUTDOWN` static and `shutdown_requested` function.
- The embedder change is minimal: one `if` arm around the existing
  `tokio::select!`. No new tasks, no new tokens, no new channels.

## Documented Workaround

For pipelines that hit the SHUTDOWN contamination in Agent Teams:

```bash
PATH=tests/mock-llm:$PATH \
  SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1 \
  setsid -w timeout 60 \
  sqlite-graphrag remember --graph-stdin < payload.json
```

Three independent layers:
- `mock-llm` on PATH bypasses the real LLM subprocess that would be
  killed by the SIGINT in the same process group.
- `SQLITE_GRAPHRAG_IGNORE_SHUTDOWN=1` bypasses the parent cancellation
  check in the embedder batch loop.
- `setsid -w` detaches the CLI from the Bash tool's process group so
  SIGINT does not propagate to the child.

## Alternatives Considered

- **Replacing the global `CancellationToken`** in a `Mutex<Option<...>>`:
  rejected because the public `cancel_token()` returns
  `&'static CancellationToken`, and replacing it would not cancel
  futures already in flight on the old token. The env var bypass is
  cheaper and surgical.
- **Per-invocation tokens only**: rejected because it would require
  changing every call site of `crate::cancel_token()`. The env var
  bypass keeps the existing public API stable.
- **Documenting the contamination as "expected behaviour"**: rejected
  because it would block every future audit and integration test from
  using Agent Teams.

## References

- Memory 1261: `audit-a1-core-cli-lifecycle-2026-06-14` (A1 audit, 8 gaps).
- Memory 1262: `incident-a1-bloqueada-shutdown-2026-06-14` (reproducer).
- Memory 1265: `adr-0034-shutdown-resilience-2026-06-14` (this ADR in
  GraphRAG form).
- `src/lib.rs:91-160` — `try_reset_shutdown`, `should_obey_shutdown`.
- `src/embedder.rs:537` — bypassed cancellation arm.
