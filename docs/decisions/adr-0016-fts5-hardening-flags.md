# ADR-0016 — FTS5 Hardening Flags (v1.0.69)

- **Status.** Accepted.
- **Date.** 2026-06-05.
- **Deciders.** Danilo Aguiar (operator), Claude Code (advisor).
- **Supersedes.** None.
- **Related gaps.** G36 (`optimize` rebuilda FTS5 sem checar, sem progresso, sem dry-run).

## Context

`optimize` rebuilt the FTS5 index unconditionally with `INSERT INTO fts_memories(fts_memories) VALUES('rebuild')`. On a 4.3 GB database the rebuild takes ~10 minutes of wall-clock time even when the index is already healthy. Operators had no way to know whether a rebuild was needed, no progress indicator during the rebuild, and no dry-run mode for CI validation.

The FTS5 rebuild command is synchronous and does not call the SQLite progress handler, so the only observability available is a background poll of the `fts_memories` row count.

## Decision

1. Pre-check FTS5 before rebuilding. `check_fts_functional` (already `pub` in `src/commands/fts.rs`) reports whether the index is healthy. The default behaviour is to skip the rebuild when the index passes integrity-check.
2. Add `--no-fts-skip-when-functional` to force a rebuild even when the index is healthy.
3. Add `--fts-dry-run`. When set, `optimize` runs `check_fts_functional` + `fts stats` and emits a `OptimizeResponse` with `status: "rebuild_recommended"` or `"ok"`, then exits with code 1 if a rebuild is recommended.
4. Add `--fts-progress <SECONDS>`. When set to a positive integer, a background thread opens a SEPARATE read-only connection (because `rusqlite::Connection` is not `Send`) and emits a `tracing::info!` line with the current `fts_memories` row count every N seconds. Default 30, 0 disables.
5. Add `--yes` to skip any future interactive prompt (currently reserved for forward compatibility — no interactive prompts exist yet).
6. The `OptimizeResponse` exposes `fts_rebuilt`, `fts_skipped_functional`, `fts_unhealthy`, and `fts_rows_indexed` (observed row count) for observability.

## Consequences

- A healthy FTS5 index is no longer rebuilt on every `optimize` call. The 10-minute wait becomes a 0.5-second skip.
- Operators can validate the FTS5 state in CI with `--fts-dry-run` and exit code 1 as a non-zero signal.
- Long-running rebuilds emit at least one progress line per `--fts-progress` interval, so the operator can see the wall-clock work happening.
- 2 new tests cover the dry-run and the response fields; the existing `fts::check_fts_functional` tests are unchanged.

## Alternatives Considered

- Use `sqlite3_progress_handler` for in-line progress. REJECTED. The FTS5 rebuild command does not invoke the progress handler (confirmed by duckduckgo-search-cli research and SQLite documentation).
- Skip the rebuild when the timestamp is recent. REJECTED. The `fts check` query is the authoritative answer; a timestamp heuristic would be wrong in edge cases.
- Use `fts5_test()` instead of `fts check`. REJECTED. `fts check` is a higher-level wrapper that reports a structured result; `fts5_test()` is a lower-level C-API hook.

## References

- `src/commands/optimize.rs:36-67` (new flag definitions).
- `src/commands/optimize.rs:105-110` (`--fts-dry-run` branch).
- `src/commands/optimize.rs:154-170` (`--fts-progress` background thread with `open_ro`).
- `src/commands/fts.rs:245-265` (`check_fts_functional`).
- gaps.md G36 lines 1914-2010.
