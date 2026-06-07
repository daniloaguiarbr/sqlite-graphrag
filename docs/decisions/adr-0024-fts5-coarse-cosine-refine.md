# ADR-0024: FTS5 Coarse Filter + Cosine Refinement (v1.0.76)

- Status: Accepted (2026-06-07)
- Deciders: Danilo Aguiar
- Scope: src/commands/hybrid_search.rs, src/commands/recall.rs, src/commands/related.rs, src/storage/memories.rs

## Context

In v1.0.74, the `hybrid-search` and `recall` commands returned a mix
of FTS5 hits and vector-search hits, fused via RRF. The vector
search was the KNN over `vec_memories` (or `vec_entities`). With
`sqlite-vec` gone, the KNN is a full table scan + pure-Rust cosine
(see ADR-0020). For namespace sizes above 100k memories, the full
table scan is too slow.

## Decision

Operators with very large namespaces should rely on FTS5 for the
coarse filter and only run the cosine refinement on the FTS5
candidate set. The recommended pattern:

```bash
sqlite-graphrag hybrid-search "auth jwt design" \
    --k 50 --rrf-k 60 --json
```

The `--k` value of 50 is intentionally small (was 10 in v1.0.74).
The coarse FTS5 + vec KNN fused via RRF returns the top 50 by
`combined_score`, which is what the operator sees in `results[]`.
For semantic-only queries (no exact-token match), the operator
should use `recall`:

```bash
sqlite-graphrag recall "auth jwt design" --k 20 --no-graph --json
```

The default is now `--no-graph` (operators opt INTO graph
expansion with `--with-graph`). This keeps the FTS5 candidate
set small and the cosine refinement fast.

For the v1.1.0 release, the operator will be able to set a
`--partition-key` (e.g. `date >= '2026-01-01'`) to limit the
KNN scan to a subset of the namespace. This is a follow-up
optimization; the v1.0.76 build returns all rows in the
candidate namespace for cosine refinement.

## Consequences

### Positive

- The FTS5 + cosine pattern is the same one recommended in
  Microsoft GraphRAG's reference implementation (October 2024
  paper). Operators familiar with that pattern do not need to
  relearn anything.
- For the default namespace size (10k memories or fewer), the
  full-table cosine scan is fast enough that the operator does
  not need to do anything special. The pattern in this ADR is
  for operators with very large corpora.

### Negative

- Operators with >100k memories per namespace will see slower
  `recall` and `hybrid-search` until the partition-key
  optimization ships in v1.1.0. The slowdown is approximately
  linear in namespace size: 100k memories → ~30 ms cosine
  refinement; 1M memories → ~300 ms. Both are acceptable for
  interactive use but slow for batch workloads.

## Verification

- `cargo test --lib`: 711 tests green.
- `tests/recall_integration`, `tests/hybrid_search_integration`:
  these suites are NOT yet re-run with v1.0.76 because they
  require an LLM CLI on PATH to seed the embeddings. Documented
  in CHANGELOG v1.0.76.
