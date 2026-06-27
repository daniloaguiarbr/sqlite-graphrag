# ADR-0055 — `enrich` dead-letter convergence and OpenRouter REST embedding concurrency

**Status**: Accepted
**Date**: 2026-06-27
**Context**: sqlite-graphrag v1.0.96 — GAP-ENRICH-BACKLOG-CONVERGE, GAP-OPENROUTER-REST-CONCURRENCY

## Problem

Two independent gaps surfaced after the v1.0.95 OpenRouter chat transport
landed (ADR-0054).

### GAP-ENRICH-BACKLOG-CONVERGE

`enrich` drives a SCAN→JUDGE→PERSIST pipeline backed by the
`.enrich-queue.sqlite` work queue. A queue item that failed — a rate
limit, a timeout, a 5xx, or a hard validation/parse error from the JUDGE —
was left in its queued state with no terminal status and no retry
schedule. Every subsequent run re-scanned the same unprocessable items,
so the backlog never provably shrank to empty. Operators worked around
this with an external bash loop that re-invoked `enrich` until the count
"looked" stable, which is not a convergence guarantee and races the
enrich singleton.

### GAP-OPENROUTER-REST-CONCURRENCY

Embedding via OpenRouter (`embed_passages_parallel_with_embedding_choice`,
`src/embedder.rs`) issued one batch REST call at a time. On a multi-batch
corpus the network sat idle between round-trips: batch N+1 only started
after batch N fully returned. The name said "parallel" but the per-batch
HTTP calls were serial, leaving most of the wall-clock to round-trip
latency rather than throughput.

## Decision

### Dead-letter convergence (GAP-ENRICH-BACKLOG-CONVERGE)

Give the enrich queue a dead-letter discipline so the live set strictly
shrinks.

- The `.enrich-queue.sqlite` schema gains two columns via idempotent
  `ALTER TABLE` (`error_class`, `next_retry_at`) and a new terminal status
  `dead`. The ALTER is idempotent so existing queues upgrade in place
  without a migration step.
- Per-item failures are classified by reusing `AttemptOutcome` and
  `compute_delay` from `src/retry.rs` — the same backoff policy the rest
  of the codebase already uses, no new retry logic. Transient
  (rate-limit / timeout / 5xx) sets `next_retry_at` to now + backoff;
  HardFailure (validation / parse) is terminal immediately.
- An item becomes `dead` after `--max-attempts` (default 5) Transient
  retries, or on the first HardFailure. Dequeue is changed to honour
  `next_retry_at` (skip items not yet due) and exclude `dead`. The live
  set therefore shrinks monotonically: every pass either persists an item
  or moves it toward `dead`, and `dead` items never re-enter.
- New flags: `--until-empty` runs an internal scan→drain loop to
  convergence (replacing the external bash loop), `--max-runtime <SECS>`
  is a wall-clock ceiling that stops the loop cleanly, `--max-attempts
  <N>` is the Transient retry budget, and `--status` is a read-only report
  of backlog/queue/dead counts that neither calls the LLM nor acquires the
  enrich singleton.

### Bounded REST fan-out (GAP-OPENROUTER-REST-CONCURRENCY)

Make OpenRouter embedding overlap its round-trips without a new
dependency or a new failure surface.

- `embed_passages_parallel_with_embedding_choice` now fans out the
  per-batch REST calls with a bounded `tokio::task::JoinSet`. Results are
  re-assembled by chunk index so output order is identical to the serial
  path — callers see no ordering change.
- In-flight requests are clamped to `1..16`, the Cloudflare-safe range
  observed for the OpenRouter REST endpoint. `enrich` gains
  `--rest-concurrency` (default 8 for `--mode openrouter`, clamp `1..16`).
- `tokio::task::JoinSet` is already available (tokio is a current
  dependency); no crate is added.

## Alternatives Considered / Deliberate Deviations

### A. Convert the enrich thread-pool to tokio tasks

Not done (deliberate). The enrich worker pool stays a thread-pool. The
concurrency win for embedding is in overlapping network round-trips, which
the bounded JoinSet delivers locally inside the embedding call; rewriting
the enrich orchestration onto tokio tasks would be a large, orthogonal
change with no additional throughput, because the real serialization point
is the SQLite single writer, not the worker model.

### B. Add an mpsc writer task to serialize DB writes

Not done (deliberate). Writes are already serial through WAL plus an
atomic claim, so a dedicated mpsc writer would add a channel and a task
without removing any contention — the single-writer invariant is already
enforced at the SQLite layer.

### C. Drop the subprocess guardrails now that OpenRouter is REST

Not done (deliberate). The preflight/spawn guardrails are preserved
because they still protect the `claude-code` / `codex` / `opencode` modes;
`--mode openrouter` simply does not exercise them. Removing them would
regress the three CLI transports for no benefit to the REST path.

### D. Unbounded fan-out for maximum embedding throughput

Rejected. Unbounded concurrency against the OpenRouter REST endpoint trips
Cloudflare rate limiting; the `1..16` clamp is the safe operating range,
and 8 is a conservative default.

### E. A separate retry/backoff implementation for the queue

Rejected (DRY). `AttemptOutcome` and `compute_delay` in `src/retry.rs`
already encode the Transient-vs-HardFailure classification and the
exponential backoff; the queue reuses them verbatim rather than forking a
parallel policy.

## Consequences

- Positive: the enrich backlog provably converges — `--until-empty` drives
  it to an empty live set in one invocation, with `--max-runtime` as a
  safety ceiling and `--status` for read-only observability that never
  touches the LLM or the singleton.
- Positive: permanently unprocessable items land in `dead` instead of
  being retried forever, and transient failures back off on a schedule
  instead of hot-looping.
- Positive: OpenRouter embedding overlaps its REST round-trips, cutting
  wall-clock on multi-batch corpora, with order preserved and the
  Cloudflare-safe `1..16` clamp.
- Neutral: the enrich thread-pool, the WAL-serialized writer, and the
  subprocess guardrails are intentionally unchanged (see Deviations A–C).
- Negative: a `dead` item requires operator inspection (via `--status`)
  to diagnose; it is not auto-resurrected. This is the intended trade-off
  — convergence over indefinite retry.

## Validation

- Build/clippy/test results to be confirmed by lead verification (Fase 7).

## Cross-references

- `gaps.md` — GAP-ENRICH-BACKLOG-CONVERGE and GAP-OPENROUTER-REST-CONCURRENCY marked RESOLVIDO em v1.0.96
- ADR-0054 (OpenRouter chat transport for `enrich`) — the v1.0.95 change this builds on
- ADR-0053 (v1.0.94 four-gap remediation) — made `enrich --mode` required
- `src/commands/enrich.rs` (`--until-empty`, `--max-runtime`, `--max-attempts`, `--status`, `--rest-concurrency`, dead-letter dequeue), `src/embedder.rs` (`embed_passages_parallel_with_embedding_choice` JoinSet fan-out), `src/retry.rs` (`AttemptOutcome`, `compute_delay` reused by the queue), `.enrich-queue.sqlite` (`error_class`, `next_retry_at`, status `dead`)
