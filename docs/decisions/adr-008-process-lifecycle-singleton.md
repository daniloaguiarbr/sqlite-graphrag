# ADR-008: Per-Namespace Job Singleton for Heavy LLM-Driven Commands

## Status
- Accepted (2026-06-03, v1.0.68)

## Context
- The `enrich`, `ingest --mode claude-code`, and `ingest --mode codex` commands each spawn a `claude -p` (or `codex exec`) subprocess per item being processed.
- The previous design shared a 4-slot counting semaphore (`MAX_CONCURRENT_CLI_INSTANCES = 4` in `src/constants.rs:341`) across all CLI commands, meaning that two parallel `enrich` invocations on the same database would both succeed in acquiring slots.
- Combined with `--llm-parallelism` (default 1, max 32) and the typical 8-10 MCP servers configured per user, a single `enrich` invocation could spawn 16-20 child processes; four parallel invocations × 2 workers × 10 MCP servers = ~160-192 processes, saturating a 10-CPU host to load average 276 (real incident 2026-06-03).
- The existing `try_acquire_slot` / `try_lock_exclusive` infrastructure on `cli-slot-{N}.lock` files was already in place and battle-tested; extending it for a different lock type was straightforward.

## Decision
### Architecture
- Introduce `JobType` enum in `src/lock.rs:43` with three variants: `Enrich`, `IngestClaudeCode`, `IngestCodex`.  Light commands (`recall`, `stats`, `read`, `list`) intentionally do NOT have variants — they continue using the existing counting semaphore.
- New `acquire_job_singleton(job_type, namespace, wait_seconds)` function acquires a `job-singleton-{tag}-{namespace}.lock` file (NOT one of the 4 counting slots).  The lock is per-`(job_type, namespace)` so two namespaces can run independent jobs.
- The returned `File` MUST be kept alive for the entire command duration; dropping it releases the singleton for the next invocation.
- When the singleton is held by another invocation, return `AppError::JobSingletonLocked { job_type, namespace }` (exit 75, classified as retryable) immediately, OR poll every `JOB_SINGLETON_POLL_INTERVAL_MS` (1000ms) until the wait deadline expires.

### Caller Integration
- `enrich::run` (`src/commands/enrich.rs:986`) acquires `JobType::Enrich` immediately after namespace resolution.
- `ingest_claude::run_claude_ingest` (`src/commands/ingest_claude.rs:580`) acquires `JobType::IngestClaudeCode`.
- `ingest_codex::run_codex_ingest` (`src/commands/ingest_codex.rs:621`) acquires `JobType::IngestCodex`.
- All three acquisitions are the FIRST operation after namespace resolution, so the singleton is held before any expensive I/O (model loading, queue DB scans).

### Error Schema
- New `AppError::JobSingletonLocked { job_type, namespace }` variant in `src/errors.rs:127`.
- Mapped to exit code 75 (`CLI_LOCK_EXIT_CODE`) — same code used by the existing counting semaphore's `AllSlotsFull` variant, so error-handling code that already special-cases 75 keeps working.
- Classified as retryable in `is_retryable()`.
- Localised in `src/i18n.rs` with `pt::job_singleton_locked(job_type, namespace)`.

### Namespace Sanitisation
- The lock file path uses a kebab-case slug of the namespace (`a-z`, `0-9`, `-`, `_`); any other character is replaced with `-` and the result is lowercased.  Empty namespaces default to `default`.
- This prevents path injection from a namespace containing `/` or `..`.

## Consequences
- Two parallel `enrich` invocations on the same namespace now fail fast with exit 75 instead of stacking.
- A long-running `enrich` (e.g. 2,321 entities × 12.5s = 8 hours serial) cannot be duplicated accidentally by an operator re-running the command.
- The CI does not need to enforce single-instance behaviour — the binary does it at runtime.
- Operators who want to parallelise across different databases (or different namespaces of the same database) can still do so via the `--namespace` flag.
- The singleton is per-`job_type`, so `enrich` and `ingest --mode claude-code` can run in parallel against the same database without interfering (different process trees, different cost budgets).

## Alternatives Considered
- **Limit `--llm-parallelism` to 1 by default** — considered, but doesn't address the cross-invocation problem and would silently slow down operators who want to use the parallelism.
- **Global process lock** — would block ALL commands, not just heavy ones, breaking the existing CLI semaphore.
- **Database-level SQLite write lock** — would block `remember` and other write commands too; the singleton is more targeted.
- **Reusing the counting semaphore with higher cost weight** — would be confusing; users would have to know that "1 enrich = 4 slots" without an obvious signal.

## References
- Gap report: `gaps.md#G28`
- Implementation: `src/lock.rs:43` (JobType), `src/lock.rs:204` (acquire_job_singleton), `src/commands/enrich.rs:986`, `src/commands/ingest_claude.rs:580`, `src/commands/ingest_codex.rs:621`
- Test coverage: 3 unit tests in `src/lock.rs::tests` (path sanitisation, second-invocation blocking, per-namespace isolation)
- Documentation: `docs/AGENTS.md#new-in-v1.0.68`, `docs/HOW_TO_USE.md#capping-process-proliferation`, `docs/COOKBOOK.md#how-to-cap-process-proliferation`
