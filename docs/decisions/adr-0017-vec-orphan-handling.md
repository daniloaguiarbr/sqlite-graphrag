# ADR-0017 — vec Orphan Handling (v1.0.69)

- **Status.** Accepted.
- **Date.** 2026-06-05.
- **Deciders.** Danilo Aguiar (operator), Claude Code (advisor).
- **Supersedes.** None.
- **Related gaps.** G39 (vec_memories_orphaned sem diagnóstico ou purga).

## Context

`health` reports `vec_memories_orphaned: N` (rows in `vec_memories` whose `memory_id` no longer exists in `memories`) but provides no path to remediation. The orphans accumulate across `forget` (soft-delete) and `purge` (hard-delete) operations because neither removes the corresponding `vec_memories` row. The 1 KB-per-vector cost is small per memory but unbounded in aggregate.

## Decision

1. Add a new `vec` subcommand family in `src/commands/vec.rs`:
   - `vec orphan-list --json` — lists each orphan with `memory_id` and `vector_hash` (BLAKE3 of the embedding blob).
   - `vec purge-orphan --yes --dry-run` — deletes orphans from `vec_memories`, `vec_entities`, and `vec_chunks` in a single transaction. The `--yes` flag is required to prevent accidental loss; `--dry-run` previews the count.
   - `vec stats --json` — reports `vec_memories_rows`, `vec_entities_rows`, `vec_chunks_rows`, and `orphaned` counts.
2. Add a hook in `src/commands/forget.rs:88-99` that calls `memories::delete_vec(memory_id)` BEFORE the soft-delete. This prevents new orphans from forming in the steady state.
3. Add a parallel hook in `purge.rs` for hard-delete.
4. The `vec purge-orphan` command purges THREE tables: `vec_memories`, `vec_entities`, and `vec_chunks`. The response reports `deleted`, `deleted_entities`, and `deleted_chunks`.

## Consequences

- The `health` warning becomes actionable: operators run `vec purge-orphan --yes` to clear the metric.
- No new orphans form in the steady state because `forget` and `purge` now remove the vectors.
- 3 unit tests cover `vec_table_exists` (renamed to avoid shadowing), `vec stats` field set, and `vec purge-orphan` transaction scope.
- Operators with no orphans can run `vec stats --json` as a routine check; the `orphaned` field should be 0.

## Alternatives Considered

- Drop `vec_memories_orphaned` from `health` instead of adding a fix. REJECTED. The metric is useful for catching bugs in `forget`/`purge`; the fix is to prevent orphans, not to hide them.
- Run `vec purge-orphan` automatically on `optimize`. REJECTED. `optimize` is read-mostly; coupling it to a destructive operation surprises operators.
- Use `FOREIGN KEY` constraints in SQLite to enforce referential integrity. REJECTED. SQLite FK enforcement is opt-in and would require a schema migration that touches every `vec_*` table.

## References

- `src/commands/vec.rs` (~430 lines, 3 tests).
- `src/commands/forget.rs:88-99` (`delete_vec` hook).
- `src/commands/mod.rs:51` (`pub mod vec`).
- gaps.md G39 lines 2179-2275.
