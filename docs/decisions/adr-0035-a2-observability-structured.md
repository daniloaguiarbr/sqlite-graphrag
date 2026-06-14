# ADR-0035 — A2 Audit: Structured Observability for Backup and Health (v1.0.80)

## Status

Accepted (v1.0.80, 2026-06-14).

## Context

The v1.0.80 audit suite (A2 audit cycle, scope:
maintenance-command observability) verified that
`commands/backup.rs` and `commands/health.rs` emit their
non-fatal diagnostics through the structured `tracing`
subscriber rather than direct `eprintln!` or `println!`
calls. The A2 audit identified that earlier versions of
these commands used `eprintln!` for non-fatal warnings
(permission fixes, NTFS DACL notes, vec-table diagnostics)
which bypassed the global subscriber and produced
unparseable output for log aggregators.

## Decision

The two non-fatal diagnostic sites in `commands/backup.rs`
and the four diagnostic sites in `commands/health.rs`
were audited and verified to use the structured
`tracing::{info, warn, error, debug}` subscriber. Each
emission is keyed by `target = "<command>"` (e.g.,
`target: "backup"`, `target: "health"`) and includes the
relevant structured fields (`path`, `error`,
`integrity_ok`, `vec_memories_ok`, `vec_entities_ok`,
`vec_missing`, `vec_orphaned`, `fts_ok`, `fts_query_ok`,
`model_ok`).

The specific emissions are:

- `commands/backup.rs:171` — `tracing::warn!` when the
  Unix mode 0o600 `set_permissions` call fails after
  `temp.persist`. The warning carries `path` and `error`
  fields and uses `target: "backup"`. The persisted
  backup file remains in place (the warning is
  informational; the persist succeeded).
- `commands/backup.rs:181` — `tracing::debug!` on Windows
  noting that the Unix mode 0o600 step is skipped because
  the NTFS DACL default is already private-to-user. The
  debug emission is the right level: it is the expected
  behaviour on Windows, not a warning condition.
- `commands/health.rs:209` — `tracing::info!` after the
  `PRAGMA integrity_check` runs, carrying
  `integrity_ok` and the elapsed time. This is the
  primary signal for log-aggregator health checks.
- `commands/health.rs:370` — `tracing::info!` after the
  vec-table checks complete, carrying
  `vec_memories_ok`, `vec_entities_ok`, `vec_missing`,
  `vec_orphaned`. The two diagnostic counts are
  required for G66 (vec-table desync diagnosis).
- `commands/health.rs:385` — `tracing::info!` after the
  FTS5 checks complete, carrying `fts_ok` and
  `fts_query_ok`. The `fts_query_ok` field is new in
  v1.0.65 and indicates a live FTS5 query succeeded
  (in addition to schema integrity).
- `commands/health.rs:423` — `tracing::info!` after the
  LLM CLI availability check, carrying `model_ok`. This
  is the primary signal for the OAuth-only enforcement
  runtime check.

## Consequences

Positive:

- All non-fatal diagnostics from the maintenance commands
  flow through the global subscriber and are captured by
  the JSON log format (`SQLITE_GRAPHRAG_LOG_FORMAT=json`)
  for log aggregators.
- The `target: "<command>"` keys allow log filters to
  surface diagnostics from a specific command without
  grepping the message text.
- The structured fields (`integrity_ok`,
  `vec_memories_ok`, `fts_query_ok`, `model_ok`, etc.)
  are stable across v1.x.y and form a documented
  observability contract.

Negative:

- Pretty-mode human readers see one line per emission
  with structured fields; the format is more verbose
  than a single `eprintln!` line. This is the trade-off
  for machine-parseable diagnostics.
- The Windows `debug!` emission is silent at the default
  log level; operators who need to verify the Windows
  DACL behaviour must set
  `SQLITE_GRAPHRAG_LOG_LEVEL=debug`.

## References

- `src/commands/backup.rs:171` (Unix mode 0o600 warning)
- `src/commands/backup.rs:181` (Windows DACL debug)
- `src/commands/health.rs:209` (PRAGMA integrity_check)
- `src/commands/health.rs:370` (vec-table checks)
- `src/commands/health.rs:385` (FTS5 checks)
- `src/commands/health.rs:423` (LLM CLI availability)
- A2 audit cycle (v1.0.80, scope: maintenance-command
  observability)

