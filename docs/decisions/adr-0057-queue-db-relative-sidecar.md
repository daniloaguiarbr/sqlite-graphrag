# ADR-0057: Enrich + Ingest queue sidecar derived from `--db`, not the CWD (v1.0.97)

- **Status**: Accepted
- **Date**: 2026-06-29
- **Version**: v1.0.97 (closes GAP-SG-64 and the newly-found GAP-SG-65)

## Context

The v1.0.97 end-to-end audit surfaced a class of bug: worklist sidecar
databases were resolved against the process CWD instead of the directory of
the `--db` database.

- GAP-SG-64 (enrich): `const DEFAULT_QUEUE_DB: &str = ".enrich-queue.sqlite"`
  (`src/commands/enrich/mod.rs`) was opened via `Connection::open` with a
  relative literal, so `enrich --status --db X` reported the CWD queue, not X's.
- GAP-SG-65 (ingest, found while fixing GAP-SG-64): the clap flag
  `#[arg(long, default_value = ".ingest-queue.sqlite")]` (`IngestArgs.queue_db`,
  consumed by both `ingest_claude.rs` and `ingest_codex.rs`) had the same
  CWD-relative default, so `--resume`/`--retry-failed` lost the queue whenever
  the CWD changed between runs.

Cause -> effect: a static/relative queue path (a const, or a clap
`default_value`) resolves against the CWD rather than
`AppPaths::resolve(--db).db`. Two sources of truth diverge: the scan honours
`--db` (correct `unbound_backlog`) while the queue is CWD-fixed, producing a
misleading `--status` and a cross-processing risk by `memory_id` collision when
draining a queue planted for a different database.

Empirical evidence: with the same `--db` and `--namespace`, `enrich --status`
reported `queue_pending=111` from the project CWD versus `0` from
`/tmp/e2e-cwd-test`; the only variable was the CWD.

Verified siblings that are already safe (no change): `slots_dir()` resolves
`XDG_RUNTIME_DIR -> SQLITE_GRAPHRAG_CACHE_DIR -> HOME/.local/share -> /tmp`;
`lock.rs` uses `cache_dir()`; codex `schema_path` uses `trusted_schema_path()`
(cache dir) or a tempfile. The only live members of the class were the two
queues above.

## Decision

1. Add `paths::sidecar_path(db_path: &Path, filename: &str) -> PathBuf` next to
   the existing `parent_or_err`. It derives the sidecar in the database's parent
   directory and falls back gracefully to the bare filename (CWD) when
   `db_path` has no parent — preserving the legacy default-DB layout.

2. Enrich (GAP-SG-64): widen `open_queue_db` to `P: AsRef<Path>`
   (`rusqlite::Connection::open` is already generic over `AsRef<Path>`), remove
   the relative `DEFAULT_QUEUE_DB` const, and derive `queue_path` from
   `paths.db` in all four branches of `run` (list-dead/requeue, status, the main
   drain, and the worker closure via a `&queue_path` re-borrow). The public
   `cleanup_queue_entry` gains a leading `db_path: &Path` parameter; its three
   callers (`forget`, `purge`, `remember`) pass their resolved `paths.db`
   (`purge` threads it through `execute_purge`).

3. Ingest (GAP-SG-65): `IngestArgs.queue_db` becomes `Option<String>` with no
   clap default; `run_claude_ingest`/`run_codex_ingest` resolve
   `queue_path = args.queue_db.as_deref().map(PathBuf::from).unwrap_or_else(|| sidecar_path(&early_paths.db, ".ingest-queue.sqlite"))`.
   An explicit `--queue-db` still overrides.

4. Remove the dead `constants::CLI_LOCK_FILE` (zero uses).

### Why no legacy migration

`AppPaths::resolve(None)` without `SQLITE_GRAPHRAG_HOME` returns
`current_dir().join("graphrag.sqlite")` (absolute), so the derived sidecar
COINCIDES with the legacy `./.enrich-queue.sqlite` when run from the project
directory — the canonical workflow keeps its backlog with no file move. When
`--db` points elsewhere, the CWD queue rightfully belongs to the CWD database,
so leaving it behind is correct; migrating it would mis-bind `memory_id`s.
A SQLite-with-WAL file move was therefore deliberately avoided.

## Consequences

- `enrich --status` and the ingest `--resume` follow `--db`; the queue is
  isolated per database directory; cross-processing by `memory_id` between
  databases sharing a CWD is eliminated.
- Public API change: `cleanup_queue_entry` and `IngestArgs.queue_db` signatures
  changed (internal, not a published API; CLI output schema unchanged —
  `schema_contract_strict` stays 38/0).
- New regression test `tests/enrich_queue_db_isolation.rs` plants a queue next
  to `db_a` and proves `--status` reads it from an unrelated CWD (the divergence
  the prior suite never exercised, since integration tests ran where CWD == --db).

## Sibling

Design sibling of GAP-SG-63 (slots_dir CWD/XDG isolation), resolved in v1.0.97.
