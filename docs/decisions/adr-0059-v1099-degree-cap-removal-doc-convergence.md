# ADR-0059 — v1.0.99: Remove Destructive Degree-Cap Pruning; Align sort-by-degree Doc; Converge body-enrich

- **Status**: Accepted
- **Date**: 2026-06-30
- **Version**: v1.0.99 (closes GAP-SG-67, GAP-SG-68, GAP-SG-69)

## Context

A real `remember` write — one memory referencing two pre-existing super-hub
entities — silently pruned ~4856 historical edges and exited 0 with only a WARN.
The hubs (degree 2872 and 2073) were capped down because `graph::enforce_degree_cap`
trimmed the lowest-weight edges until each node was under the cap, with no
`memory_id` filter: it ranged over EVERY edge touching the hub, not just the
edges the current write introduced. Combined with `ON DELETE CASCADE`, the cap
deleted edges that belonged to other memories. The write reported success; the
relationship total dropped by thousands. This is GAP-SG-67 — a write became
destructive to historical graph state it never owned. The cap had been wired in
as an "actionable" feature in v1.0.97 (GAP-SG-49), but the global, owner-blind
deletion made it a data-loss hazard rather than a guardrail.

Two smaller defects surfaced in the same audit:

- GAP-SG-68 — `graph entities --sort-by degree` (without `--order`) sorted
  ASCENDING, contradicting the `EntitySortField::Degree` doc-comment in
  `src/commands/graph_export.rs`, which promised "descending by default". The
  `--help` text inherited the wrong promise, so users asking for "degree" got the
  least-connected entities first with no warning.

- GAP-SG-69 — `enrich --operation body-enrich --until-empty` did not converge.
  The scan re-enqueued short bodies that the trigram-Jaccard preservation guard
  had already rejected (`status='skipped'`), so each pass re-judged the same
  vetoed memories and `--until-empty` never terminated.

## Decision

1. **Remove the destructive degree-cap pruning (GAP-SG-67).** Delete
   `graph::enforce_degree_cap` and its two call sites in `remember` and `link`.
   Drop the `--max-entity-degree` flag from both `remember` and `link`
   (BREAKING — scripts that still pass it get a clap argument error, exit 2; the
   obsolete `--max-entity-degree 0` mitigation is no longer needed). Writes are
   now 100% additive: they never prune, delete edges, or emit a degree warning,
   and the total relationship count never decreases on a normal write. Schema
   stays at version 15 — no migration.

2. **Align the sort-by-degree doc to the actual behaviour (GAP-SG-68).** Rather
   than flip the sort direction (which would change a long-standing SQL contract
   exercised by the `build_order_by_*` tests), rewrite the `EntitySortField::Degree`
   doc-comment to match the ascending behaviour: "Sort by degree (total number of
   relationships). Use `--order desc` for most-connected-first." One line of
   doc-comment in `src/commands/graph_export.rs`; the 6 `build_order_by_*` tests
   stay green.

3. **Converge body-enrich (GAP-SG-69).** Add `skipped_item_keys`
   (`src/commands/enrich/queue.rs`), which reads the item_keys with
   `status='skipped'` for a given operation. The initial scan and the `BodyEnrich`
   rescan (`src/commands/enrich/mod.rs`) exclude memories already vetoed
   `skipped`, so the live set strictly shrinks. The `.enrich-queue.sqlite` sidecar
   `remove_file` runs only when `dead==0` AND `skipped==0`, preserving the veto
   verdict across passes. `cleanup_queue_entry` (called from remember/edit/forget/
   purge) clears the veto when the body changes, so an edited body is reconsidered
   automatically. Scope restricted to `BodyEnrich`.

## Consequences

### Positive

- Writes are non-destructive by default: a `remember`/`link` referencing a
  high-degree hub can no longer delete another memory's edges. Graph history is
  preserved.
- `graph entities --sort-by degree --help` no longer lies; users get the
  documented ascending order and a pointer to `--order desc`.
- `enrich --operation body-enrich --until-empty` converges: empirically
  items_total dropped 55→3 on the second pass, with the skipped verdict honoured
  between passes. Regression test
  `skipped_item_keys_excludes_only_skipped_for_operation`.

### Negative / Trade-off (GAP-SG-67)

- Without the cap, hub degree grows without bound. This is accepted: a write must
  never silently delete data it does not own. Any future degree normalization must
  be an EXPLICIT maintenance command (operator-invoked, owner-aware), never a side
  effect of a normal write.

### Schema

- No migration; schema stays v15.

## Sibling

Reverses the GAP-SG-49 wiring from ADR-0056's release line (v1.0.97), which made
`enforce_degree_cap` "actionable" without scoping the deletion to the current
write's edges.
