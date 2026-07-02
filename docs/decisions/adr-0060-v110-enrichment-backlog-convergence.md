# ADR-0060: v1.1.0 — Enrichment Backlog Convergence at the Root (GAP-SG-70..78)

- **Status**: Accepted
- **Date**: 2026-07-01
- **Version**: v1.1.0 (closes GAP-SG-70, GAP-SG-71, GAP-SG-72, GAP-SG-73, GAP-SG-74, GAP-SG-75, GAP-SG-76, GAP-SG-77, GAP-SG-78)

## Context

After v1.0.96/97/99, the enrichment dead-letter and observability surface still
had nine sharp edges that let a healthy backlog masquerade as either a dead-letter
graveyard or a false "empty queue". Truncated OpenRouter completions were re-emitted
with the *same* `max_tokens`, so every retry truncated identically and eventually
dead-lettered — a self-reinforcing loop the operator could not break. Retry
classification matched on error-message substrings, so an exhausted-internal-retry
("max retries exceeded") was mislabelled permanent and dropped straight to
dead-letter instead of taking the queue backoff. The dequeue loop collapsed
`SQLITE_BUSY` into a false empty backlog via `.ok()`, silently under-processing
under lock contention. `enrich --status` read only the memory-bindings sidecar,
reporting a false `pending=0` for `entity-descriptions`, `body-enrich` and
`re-embed`. And a transient, not-yet-materialized entity went to dead-letter on its
very first miss. Each of these turned a recoverable condition into a terminal one,
and the reported state could not be trusted as a convergence signal.

## Decision

1. **Retry truncated completions with a grown budget (GAP-SG-70).** `chat_api`
   deserializes `choices[].finish_reason`; on `"length"` it re-emits the request
   with a grown `max_tokens` — bounded by `ENRICH_MAX_LENGTH_RETRIES` — *before*
   attempting JSON repair, breaking the loop where a retry reused the same budget
   and truncated identically.

2. **Adaptive `max_tokens` constants (GAP-SG-71).** Named constants
   (`ENRICH_INITIAL_MAX_TOKENS`, `ENRICH_MAX_TOKENS_GROWTH_FACTOR`,
   `ENRICH_MAX_TOKENS_CEILING`, `ENRICH_MAX_LENGTH_RETRIES`) size the initial budget
   and its growth per retry, replacing the previous unbounded provider default.

3. **Dead-letter diagnostic columns (GAP-SG-72).** The enrich sidecar queue gains
   `finish_reason`, `input_tokens`, `output_tokens` columns via an idempotent
   `ALTER`; `complete()` returns a `ChatCompletion`/`ChatError` carrying these, and
   `--list-dead --json` exposes them so the operator can see *why* an item died.

4. **Typed retry classification, never substring (GAP-SG-73).**
   `classify_enrich_outcome` decides purely by `AppError` variant; OpenRouter
   failures carry a `retry_class` computed at the origin (exact HTTP status /
   structured provider code). The key false-permanent fix: an
   exhausted-internal-retry failure ("max retries exceeded") is now `Transient`
   (eligible for the queue `--max-attempts` backoff) instead of an immediate
   dead-letter.

5. **Shared `openrouter_http` module (GAP-SG-74, DRY).** The duplicated `ApiError`,
   `code_string`, `MAX_RETRIES`, and `backoff` shared by the chat and embedding
   clients are extracted into a new `openrouter_http` module, which also hosts the
   `status_retry_class` / `provider_error_retry_class` classifiers — one source of
   truth for retry semantics across chat and embedding.

6. **Version-stamped User-Agent (GAP-SG-75).** The OpenRouter HTTP `User-Agent` is
   bumped to `sqlite-graphrag/1.1.0` (it had drifted at 1.0.95/1.0.96).

7. **Bounded dequeue under contention (GAP-SG-76).** `open_queue_db` sets
   `busy_timeout`, and the dequeue reuses the bounded `with_busy_retry` (capped,
   exponential backoff + jitter, kill-switch-aware), failing loud with exit 15 on
   sustained contention instead of `.ok()`-collapsing `SQLITE_BUSY` into a false
   "empty backlog".

8. **Real per-operation `scan_backlog` in `--status` (GAP-SG-77).** `enrich
   --status` reports a real per-operation `scan_backlog` — the database candidates a
   scan would actually enqueue — instead of only the memory-bindings
   `unbound_backlog`, eliminating the false `pending=0` for `entity-descriptions`,
   `body-enrich` and `re-embed`. A new count-only `count_operation_backlog` shares
   the exact WHERE predicates with the scanners, so the reported backlog can never
   diverge from a real scan; the `state` field derives its `pending-scan` verdict
   from the current operation's `scan_backlog`.

9. **Transient not-yet-materialized entity (GAP-SG-78).** A not-yet-materialized
   entity is classified `Transient` (retried, not dead-lettered on first miss) via a
   typed `AppError::EntityNotYetMaterialized { name, namespace }` (`exit_code` 4,
   `is_retryable` true), replacing the string `NotFound` at the two entity call sites
   (`entity-descriptions`, `entity-type-validate`); the namespace-blind lookup in
   `call_entity_type_validate` (which ignored `_namespace` and matched on `name`
   alone) is corrected to `WHERE namespace = ?1 AND name = ?2`.

## Alternatives Considered

- **Keep substring-based retry classification.** Rejected: fragile, order-dependent
  on provider message wording, and it violates the project's typed-retry-with-backoff
  rule. Any provider copy change would silently re-break the classification.
- **Swap the enrichment LLM model.** Rejected by policy: `deepseek/deepseek-v4-flash:nitro`
  is the fixed enrichment model; the truncation loop is a budget/retry defect, not a
  model defect, and is fixed at the request layer.
- **Schema migration for the diagnostic columns.** Unnecessary: the idempotent
  `ALTER` keeps the schema at version 15, so no migration step and no
  forward/backward compatibility break.

## Consequences

### Positive

- Dead-letter becomes trustworthy: recoverable conditions (truncation, exhausted
  internal retry, transient entity miss) take the backoff path, so a converged
  `queue_dead == 0` is actually reachable.
- `enrich --status` becomes the source of truth: the per-operation `scan_backlog`
  matches what a real scan would enqueue, so `pending=0` no longer lies for
  `entity-descriptions`, `body-enrich` or `re-embed`.
- No more truncation loop: a `"length"` completion grows its budget and either
  succeeds or terminates at `ENRICH_MAX_LENGTH_RETRIES`, never re-truncating
  identically.
- DRY: chat and embedding clients share one `openrouter_http` retry/classification
  module, so retry semantics can no longer drift between the two paths.
- Operators can diagnose deaths from `--list-dead --json` (`finish_reason`,
  `input_tokens`, `output_tokens`) instead of guessing.

### Negative / Notes

- Items **already** marked `dead` in real databases before v1.1.0 stay dead; the new
  classification only governs future outcomes. Operational recovery is via
  `--requeue-dead` (optionally `--ignore-backoff`) and `--prune-dead-orphans`.
- An entity that **never** materializes is not retried forever: it is terminated by
  `--max-attempts`, so a genuinely absent entity still reaches dead-letter after the
  bounded backoff — the fix only prevents dead-lettering on the *first* transient
  miss.
- Schema stays v15; the diagnostic columns arrive via idempotent `ALTER`, so an older
  binary reading a newer sidecar simply ignores the extra columns.

## Cross-references

- ADR-0054 — OpenRouter chat enrichment.
- ADR-0055 — enrich dead-letter + REST concurrency.
- ADR-0057 — enrich queue sidecar.
- ADR-0058 — `prune-dead-orphans`.
- ADR-0059 — v1.0.99 (degree-cap removal, doc convergence).
- CHANGELOG.md — section 1.1.0.
- gaps.md — GAP-SG-70..78.
