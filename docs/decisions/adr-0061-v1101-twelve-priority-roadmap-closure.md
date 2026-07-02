# ADR-0061: v1.1.01 — Twelve-Priority Roadmap Closure (gaps.md Limitations 1–15)

- **Status**: Accepted
- **Date**: 2026-07-02
- **Version**: v1.1.01 (official release name; `Cargo.toml` carries `1.1.1` because SemVer rejects a leading zero in the patch component — the HTTP `User-Agent` is `sqlite-graphrag/1.1.1` via `CARGO_PKG_VERSION`)

## Context

The gaps.md block "Melhoria do GraphRAG — Limitações da CLI sqlite-graphrag"
documents fifteen limitations, each audited against the v1.1.0 source code
(file-and-line confirmations) and prioritized into a twelve-priority
implementation roadmap. The audit identified **one common architectural root
cause** behind most of them: the separation between the *write path* and the
*graph maintenance path*. The CLI was designed for incremental
memory-by-memory writing, not for mass maintenance of an already-existing
corpus — so every corrective bulk operation depended on commands that did not
exist (embedding backfill, degree recompute, ID-based disambiguation) or that
destructively normalized their input (`reclassify-relation --from`). Secondary
observability limitations compounded this: `health` tested vector-table
*existence* rather than coverage, and `embedding status` projected only the
pending-memories queue. Priorities 8 and 9 (sidecar-queue-as-truth in
`enrich --status`, transient entity absence dead-lettered on first miss) had
already been resolved in v1.1.0 as GAP-SG-77 and GAP-SG-78 (ADR-0060). This
release closes the remaining ten: Priorities 1–7 and 10–12.

## Decision

1. **Decouple entity embedding from the LLM subprocess (P1, Limitation 3).**
   Entity embedding runs through the same OpenRouter REST API used for
   memories and chunks — the `[OpenRouter]` chain applies even with
   `--llm-backend none` — so every new write receives an entity vector and the
   coverage gap no longer regenerates. Empty-vector guards are added to
   `upsert_entity_vec`, `upsert_chunk_vec` and `memories::upsert_vec`.

2. **Embedding backfill via re-embed targets (P2, Limitation 2).**
   `enrich --operation re-embed --target memories|entities|chunks|all`
   introduces new scanners in `src/commands/enrich/scan.rs` covering
   `entity_embeddings` and `chunk_embeddings`, with a per-target
   `scan_backlog` in `--status` so backfill convergence is observable.

3. **`graph recompute-degree` (P3, Limitation 4).** A new subcommand
   (implemented in `src/commands/graph_export.rs`) recomputes the stored
   `degree` from the real edges in a single transaction, supports `--dry-run`,
   and reports the envelope `{total, updated, zeroed, unchanged}`.

4. **`reclassify-relation --literal-from` (P4, Limitation 1).** The new flag
   matches the stored relation verbatim, bypassing the clap `value_parser`
   normalization at the argument edge (`src/commands/reclassify_relation.rs`),
   so legacy hyphenated edges (`applies-to`, `depends-on`) become migratable.

5. **ID-based entity disambiguation (P5, Limitation 5).**
   `merge-entities --ids/--into-id` and `rename-entity --id` operate on entity
   IDs with namespace scoping, removing the name-only ambiguity.

6. **Real vector-coverage observability (P6, Limitations 7 and 8).**
   `health --json` gains `vec_*_missing` and `vec_*_coverage_pct` with real
   orphan/coverage semantics (not mere table existence), and
   `embedding status --json` gains per-table `*_missing` counters in its
   coverage section.

7. **Typed `EntityType` deserialization (P7, Limitations 6 and 9).**
   `EntityType` implements a manual `Deserialize` whose error message lists
   the 13 canonical types, validating early with an actionable message.

8. **Priorities 8 and 9 — already closed in v1.1.0.** Per-operation
   `scan_backlog` in `enrich --status` (GAP-SG-77) and transient
   not-yet-materialized entity (GAP-SG-78); see ADR-0060.

9. **Dimension-aware re-embed predicate (P10, Limitation 13).** The
   `reembed_*_predicate` functions in `scan.rs` select rows whose stored `dim`
   diverges from the configured dimension or whose blob is empty — across all
   three vector tables — instead of only rows with a missing vector.

10. **Typed payload-limit errors (P11, Limitation 15).**
    `AppError::BodyTooLarge` and `AppError::TooManyChunks` replace the single
    indistinct `LimitExceeded`; exit 6 is preserved, but the message now names
    the specific ceiling and the measured value (512000 body bytes,
    512 chunks), so the operator knows which limit fired.

11. **`ingest --name-prefix` (P12, Limitation 14).** Ingest accepts a name
    prefix with ceiling validation and a reduced budget for the derived name,
    giving batch imports controllable naming.

The schema stays at v15 — no migration. The release binary is ~19 MiB.

## Alternatives Considered

- **Direct SQL `UPDATE` on the `.sqlite` for maintenance.** Rejected: the
  project rule forbids writing to the database outside the binary; every
  maintenance path must be a first-class CLI command.
- **`1.1.01` as the Cargo version.** Rejected: SemVer forbids a leading zero
  in the patch component, so `Cargo.toml` carries `1.1.1` while the official
  release name remains v1.1.01.
- **A separate `embedding backfill` command instead of re-embed targets.**
  Rejected: extending `enrich --operation re-embed` with `--target` reuses the
  existing queue, `--status`, `--resume` and dead-letter machinery instead of
  duplicating it.
- **An admin `relation-rename-raw` subcommand instead of `--literal-from`.**
  Rejected: a flag on the existing command keeps the surface smaller and the
  filter semantics adjacent to the normalizing default.

## Consequences

### Positive

- The root cause is addressed at both ends: the write path no longer
  regenerates the entity-vector gap (P1), and the maintenance path finally
  exists for the historical liability (P2, P3, P4, P5).
- `health` and `embedding status` become true coverage instruments — a
  converged backfill is verifiable from `vec_*_coverage_pct` and `*_missing`
  instead of inferred.
- Exit 6 becomes diagnosable: the operator can size splits by the ceiling that
  actually fired instead of guessing between bytes and chunk count.
- Legacy hyphenated relation edges have a safe, literal migration path for the
  first time.
- Dimension drift (Limitation 13) is now selectable by the re-embed scanners,
  so a corpus embedded at a legacy dimension can be re-vectorized in place.

### Negative / Notes

- The code closes the roadmap, but the **production-database liability remains
  pending operational execution**: backfill, degree recompute and the
  hyphen-edge migration must still be run with the new commands.
- The release-name/Cargo-version split (v1.1.01 vs `1.1.1`) requires care when
  comparing versions; the `User-Agent` reports `1.1.1`.
- P8/P9 are documented here only for roadmap completeness; their design record
  is ADR-0060.

## Cross-references

- gaps.md — "Melhoria do GraphRAG" block, Limitations 1–15 and the
  "Roteiro de Implementação Recomendado" (Priorities 1–12).
- CHANGELOG.md — v1.1.01 section.
- ADR-0059 — v1.0.99 (degree-cap removal, doc convergence).
- ADR-0060 — v1.1.0 (enrichment backlog convergence, GAP-SG-70..78; P8/P9).
- Code: `src/commands/enrich/scan.rs`, `src/commands/graph_export.rs`,
  `src/commands/reclassify_relation.rs`, `src/commands/health.rs`,
  `src/commands/embedding.rs`, `src/entity_type.rs`, `src/errors.rs`,
  `src/storage/entities.rs`.
