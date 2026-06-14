# ADR-0029 — A1 Audit: Main Thread Sync, Explicit Flush, and Deadlock Watchdog (v1.0.80)

## Status

Accepted (v1.0.80, 2026-06-14).

## Context

The v1.0.80 audit suite (A1 audit cycle, scope: core CLI lifecycle
and threading) identified three interacting risks in the
`main` entry point and the deadlock-detection watchdog:

- **A1/G1 — Implicit async assumption**: the codebase had
  inherited a tokio runtime from pre-v1.0.76. With the v1.0.76
  LLM-only one-shot refactor the runtime was no longer
  required, but the assumption persisted in code comments and
  influenced how shutdown signals and cancellation tokens were
  wired. The audit verified that the main thread is
  intentionally 100% synchronous: every `remember`, `ingest`,
  and `enrich` spawns a headless `claude` or `codex` subprocess
  via `std::process::Command` and waits on its exit. The
  per-subprocess concurrency cap is enforced by the
  `acquire_cli_slot` counting semaphore and the
  `MAX_CONCURRENT_CLI_*` constants; cross-process sync happens
  via SQLite WAL and `flock`. The pre-tokio design is a
  deliberate policy choice: no async runtime context to
  cancel, no `tokio::select!` arms to skip, and no `JoinSet` to
  drain on shutdown (see ADR-0034 for the SHUTDOWN global and
  the audit-mode bypass). Touching this entry point requires
  revisiting the per-subprocess cancellation policy, not just
  adding a runtime.
- **A1/G6 — Lost partial lines on signal-killed exit**:
  `std::process::ExitCode` is a transparent wrapper around a
  `u8` returned from main; on process exit, the C runtime
  flushes its own stdio buffers but does NOT know about
  Rust's internal `BufWriter` wrapping stdout/stderr. Without
  the explicit flush, the last partial line of JSON output
  (notably from `output::emit_json_compact` and
  `emit_progress`) can be lost when the process is killed by
  a signal or exits with an error code. This is a deliberate
  defensive policy: flush every error-path AND the
  success-path before returning.
- **A1/G7 — Deadlock-detection watchdog**: the
  deadlock-detection thread is intentionally process-scoped
  (it has no shutdown signal). It is a watchdog: it polls
  every 10 seconds and reports any deadlocks it finds via
  tracing, then sleeps again. When the process exits (via
  `std::process::ExitCode` return or a signal), the kernel
  tears down all threads; there is no leak because the
  thread is never joined or detached in the Rust sense. The
  10-second poll interval is a balance: short enough to catch
  deadlocks before any user-facing timeout, long enough to
  keep the watchdog overhead negligible.

## Decision

The three findings are recorded in `src/main.rs` as inline
documentation comments at the top of `fn main`. They are
NOT new code: they document the existing v1.0.80 behaviour
that the audit verified. The comments serve as the canonical
explanation for future maintainers and for the audit-trail
repositories.

Each comment is a 5-10 line block:

- `A1/G1` comment at `src/main.rs:39-49` documents the
  synchronous main thread design and its relationship to the
  per-subprocess concurrency cap and the audit-mode
  SHUTDOWN bypass.
- `A1/G6` comment at `src/main.rs:29-38` documents the
  explicit flush contract and the lost-partial-line
  pathology that motivates it.
- `A1/G7` comment at `src/main.rs:119-127` documents the
  watchdog design and the rationale for the 10-second poll.

## Consequences

Positive:

- The audit-trail is preserved in source as inline comments,
  making the rationale available to every maintainer who
  reads the file (no need to consult this ADR to understand
  the existing code).
- Future refactors that touch the main entry point have
  explicit guidance: any change must revisit the
  per-subprocess cancellation policy, not just the
  surrounding code.
- The flush contract is the single source of truth for
  "what does exit look like"; future error paths must follow
  it.

Negative:

- The inline comments are ~25 lines of code-level prose that
  must be kept in sync with the runtime behaviour; if a
  refactor changes the behaviour, the comments must be
  updated.
- The deadlock-detection watchdog is a runtime cost
  (10-second poll, `tracing::warn!` per deadlock); it is
  off by default and only enabled via the
  `deadlock-detection` feature flag.

## References

- `src/main.rs:29-38` (A1/G6 flush contract)
- `src/main.rs:39-49` (A1/G1 synchronous main thread)
- `src/main.rs:119-127` (A1/G7 deadlock-detection watchdog)
- `Cargo.toml:230` (`deadlock-detection` Cargo feature)
- ADR-0034 (SHUTDOWN global and audit-mode bypass)
- G28 (CLI process lifecycle governance)
- G30 (cross-process singleton via `flock`)

