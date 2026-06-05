# ADR-0015 — Preservation Gate Jaccard (v1.0.69)

- **Status.** Accepted.
- **Date.** 2026-06-05.
- **Deciders.** Danilo Aguiar (operator), Claude Code (advisor).
- **Supersedes.** None.
- **Related gaps.** G29 Passo 4 (validação de preservação), G29 Passo 5 (idempotência blake3).

## Context

`enrich --operation body-enrich` calls an LLM to expand a short memory body. The LLM may invent facts, drop critical tokens, or return a body that drifts from the original. Without a gate, every `body-enrich` is a roll of the dice: a hallucination is silently persisted and `restore --version N` is the only escape valve. Reprocessing the same memory is also unsafe because `persist_enriched_body` always re-inserts a new version even when the LLM produced a byte-for-byte identical body (rare but possible).

## Decision

1. Create `src/preservation.rs` with `jaccard_similarity(a: &str, b: &str) -> f64` that operates on character trigrams (UTF-8 safe) and an enum `PreservationVerdict` with `Preserved { score, threshold }`, `Rejected { score, threshold }`, and `Unchanged { byte_len }` variants. 10 unit tests cover boundary conditions (0.0, 0.5, 0.7, 1.0), trigrams, empty strings, and Unicode.
2. Add `--preserve-threshold <FLOAT>` to `EnrichArgs` with default 0.7. The threshold is the minimum Jaccard similarity between the original and enriched bodies required to persist.
3. In `call_body_enrich`, AFTER the LLM call, compute the Jaccard similarity. If `score < threshold`, return `EnrichItemResult::PreservationFailed { score, threshold, chars_before, chars_after }` and do NOT call `memories::update`.
4. Add idempotency via `blake3::hash`. Compute `old_hash = blake3(body)` and `new_hash = blake3(enriched_body)`. If the hashes are equal, return `EnrichItemResult::Skipped { reason: "enriched body hash matches original (blake3:{hash}); idempotency skip" }` BEFORE the Jaccard check.
5. The verification order is: (a) blake3 idempotency, (b) Jaccard preservation, (c) `chars_after <= chars_before` length sanity, (d) `memories::update`. A failure at any step emits an `EnrichItemResult` variant and skips persistence.

## Consequences

- Hallucinated bodies with low token overlap are rejected at the gate, not at `history --name <X>` after the fact.
- Reprocessing the same memory is safe: identical hashes return `Skipped`, divergent hashes that fail the Jaccard test return `PreservationFailed`, and divergent hashes that pass the Jaccard test persist normally.
- The NDJSON stream includes `preservation_failed` events with the Jaccard score, so operators can audit rejections.
- The threshold is configurable per-invocation, so CI can lower it for fast tests and operators can raise it for high-precision corpora.
- 10 + 0 = 10 new tests (the gate logic is exercised in the existing 745 tests).

## Alternatives Considered

- Use BLEU or ROUGE instead of Jaccard. REJECTED. Jaccard on trigrams is dependency-free, fast, and adequate for short bodies.
- Use a second LLM as a judge. REJECTED. The double cost and latency are not justified for the v1.0.69 release; a future ADR may add a `--judge-model` flag.
- Skip the length sanity check (step c). REJECTED. A body that is shorter than the original is almost always a regression.

## References

- `src/preservation.rs` (10 tests).
- `src/commands/enrich.rs:2127-2158` (`EnrichItemResult::PreservationFailed` variant).
- `src/commands/enrich.rs:2488-2500` (blake3 idempotency).
- `src/commands/enrich.rs:2404-2448` (Jaccard gate).
- gaps.md G29 Passos 4-5 lines 823-851.
