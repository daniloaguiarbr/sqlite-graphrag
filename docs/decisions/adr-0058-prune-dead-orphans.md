# ADR-0058: `enrich --prune-dead-orphans` — clean orphaned dead-letter rows (v1.0.97)

- **Status**: Accepted
- **Date**: 2026-06-29
- **Version**: v1.0.97 (closes GAP-SG-66)

## Context

A Claude Code hook audit against v1.0.97 found `lib/graphrag-recover-dead.sh`
broken: it called `sqlite-graphrag pending list --namespace <ns> --filter-status
dead`, which v1.0.97 rejects with exit 2 — `pending list` does not accept
`--namespace`, and `dead` is not a `--filter-status` value
(`[validated, embedding_in_progress, embedding_done, committed, abandoned,
failed]`). It also targeted the wrong table: dead-letter lives in the enrich
queue sidecar (`.enrich-queue.sqlite`), inspected by `enrich --list-dead`
(GAP-SG-23), not the `pending` embedding table.

Fixing the hook exposed GAP-SG-66. The project DB held 110 dead rows, all
`error_class=permanent` with `error="not found: memory 'X' not found"` — orphans
left by the legacy CWD-relative queue (ADR-0057): the memory was renamed or
purged after being enqueued, so the dead row points at a name that no longer
exists.

Cause -> effect: the queue indexes by `item_key`/`memory_id`; when the memory is
gone, the dead row is orphaned, and `cleanup_queue_entry` (GAP-SG-13) only fires
on `forget`/`purge` of an EXISTING memory. No command drops orphan dead:
`--requeue-dead` just re-fails them (permanent not-found goes straight back to
`dead`), so `queue_dead` grows monotonically and the hooks' dead-letter warnings
become permanent noise.

## Decision

1. Add `enrich --prune-dead-orphans`: a read-only inspector (no LLM, no
   singleton) in the `required_unless_present_any` group, so `--operation` and
   `--mode` are optional (like `--list-dead`/`--requeue-dead`).

2. `queue::prune_dead_orphans(queue_conn, main_conn, operation, namespace)`
   deletes only `status='dead' AND item_type='memory'` rows whose `item_key`
   (the memory name) is absent from the main DB, reusing the existence query from
   `enqueue_candidate`: `SELECT id FROM memories WHERE namespace=?1 AND name=?2
   AND deleted_at IS NULL`. Entity-keyed dead rows (`item_type='entity'`) are
   left untouched — their key is an entity name, not a memory name. Read-only on
   the main DB; only the sidecar is mutated.

3. `DeadSummary` gains a `pruned: i64` field. It is NOT a schema-dumped struct
   (only `EnrichSummary`/`EnrichStatus` are in `docs/schemas/`), so the addition
   is schema-neutral.

4. Hooks rewired:
   - `lib/graphrag-recover-dead.sh` (GAP-A) now iterates `GR_OPS_GATE` per
     namespace, prunes orphans via `--prune-dead-orphans`, then recovers the
     remaining dead (real body) via `forget`+`purge`+`remember`.
   - `lib/graphrag-enrich-worker.sh` (GAP-B) residual now emits db-scoped
     `total_dead` — reliable since ADR-0057 scoped the queue to `--db`; the
     prior "queue_dead not scoped by --db" comment is obsolete. This un-breaks
     the `auto-enrich.sh`/`memory-guardian.sh` consumers, which read `total_dead`
     from a producer that never wrote it.
   - `lib/graphrag-common.sh` centralises `GR_OPS_GATE`, `gr_dead_total` and
     `gr_prune_orphans` (DRY).

### Why prune, not requeue

`--requeue-dead` re-fails a permanent not-found item; prune is the only
terminal-safe cleanup. It deletes ONLY rows confirmed orphan by the same
existence check the worker uses, which had zero recoverable value.

## Consequences

- `queue_dead` becomes honest; `recover-dead.sh` closes the loop (prune orphans +
  recover the rest); the worker's `total_dead` un-breaks the hook dead-letter
  warnings (GAP-B).
- New unit test `prune_dead_orphans_removes_only_orphan_memory_rows`; real smoke
  on the project DB pruned 110 orphans (`dead_total` 110->0, `pruned:110`).
- CLI output schema unchanged; `installed_binary_smoke` stays 26/0 after
  `cargo install --path . --locked --force`.

## Sibling

Follows ADR-0057 — the orphan dead rows are its legacy residue; this ADR adds the
cleanup path the queue-scoping fix could not retroactively apply.
