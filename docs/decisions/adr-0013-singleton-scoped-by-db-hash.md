# ADR-0013 — Singleton Escopado por `db_hash` (v1.0.69)

- **Status.** Accepted.
- **Date.** 2026-06-05.
- **Deciders.** Danilo Aguiar (operator), Claude Code (advisor).
- **Supersedes.** None.
- **Related gaps.** G30 (singleton global ignorando `--db`), G09 (sinalização de wait).

## Context

`lock::acquire_job_singleton(JobType, namespace, wait_seconds)` wrote the lock file to `ProjectDirs::cache_dir()` — a path shared by every database the user touches. Two concurrent `enrich` invocations against DIFFERENT databases (`SQLITE_GRAPHRAG_DB_PATH=/tmp/a.sqlite` and `/tmp/b.sqlite`) collided, returning `AppError::JobSingletonLocked` even though the two processes were operating on disjoint resources. The error message cited a `--wait-job-singleton` flag that did not exist on the CLI, leading operators to `pkill` the lock file by hand.

## Decision

1. The lock file path gains a `db_hash` suffix: `job-singleton-{tag}-{namespace_slug}-{db_hash}.lock`. The `db_hash` is the first 12 hex characters of `blake3(canonicalize(db_path))`.
2. `db_path_hash` is `pub` so callers can compute the hash without acquiring the lock.
3. `acquire_job_singleton` gains `db_path: &Path` and `force: bool` parameters. `force: true` breaks a stale lock from a previously crashed invocation.
4. The CLI exposes `--wait-job-singleton <SECONDS>` (poll for the lock) and `--force-job-singleton` (break a stale lock) on `enrich` and `ingest`. The error message now references the real flag.
5. `--wait-lock` (already present) is kept for `--max-concurrency` (semaphore slots), distinct from `--wait-job-singleton` (lock-file wait). The `after_long_help` table lists both with one-line descriptions.

## Consequences

- Two concurrent `enrich` invocations against different databases no longer collide. The same database still serialises.
- The `db_hash` is deterministic for a given canonical path. Renaming a database file invalidates the lock automatically.
- 6 unit tests cover namespace sanitisation, second-invocation blocking, per-namespace isolation, db_hash determinism, db_hash divergence, and force flag behaviour.
- Operators recovering from a crashed invocation use `--force-job-singleton` instead of `flock -u` or `pkill`.

## Alternatives Considered

- Use a per-PID lock in `XDG_RUNTIME_DIR`. REJECTED. PIDs are not stable across crashes and do not survive reboots.
- Use a SQLite table for the lock. REJECTED. The database being locked is exactly the resource we cannot reliably access.
- Hash the canonical path with SHA-256 instead of BLAKE3. REJECTED. BLAKE3 is already a project dependency and is faster.

## References

- `src/lock.rs:74-86` (legacy `cache_dir()`).
- `src/lock.rs:92` (`db_path_hash`).
- `src/lock.rs:93-129` (`job_singleton_path`).
- `src/lock.rs:204-280` (`acquire_job_singleton`).
- `src/commands/enrich.rs:986`, `ingest_claude.rs:580`, `ingest_codex.rs:621` (call-sites pass `args.db`).
- `src/commands/ingest.rs:262-269` (`--wait-job-singleton` and `--force-job-singleton` flags).
- gaps.md G30 lines 1325-1441.
