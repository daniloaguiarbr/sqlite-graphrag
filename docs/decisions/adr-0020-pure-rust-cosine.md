# ADR-0020: Pure-Rust Cosine Similarity (v1.0.76)

- Status: Accepted (2026-06-07)
- Update (v1.0.79): the `embedding-legacy` escape hatch mentioned below was removed ahead of the v1.1.0 schedule; the transition window is closed
- Deciders: Danilo Aguiar
- Scope: src/similarity.rs, src/storage/memories.rs, src/storage/entities.rs, src/storage/chunks.rs

## Context

`sqlite-vec` exposed a vec0 virtual table with `MATCH` and `distance` columns
that returned pre-sorted KNN results. The extension was C, loaded at runtime
via `sqlite3_auto_extension`, and produced distance values in the
`[0.0, 2.0]` range (cosine distance: `1.0 - similarity`).

v1.0.76 removes `sqlite-vec`. The replacement is in-process cosine similarity
in `src/similarity.rs::cosine_similarity`. The function returns
`[-1.0, 1.0]`; `similarity_to_distance` inverts it to `[0.0, 2.0]` so the
rest of the codebase (which reads `distance` columns in KNN results) keeps
working unchanged.

## Decision

The KNN search in `storage::memories::knn_search` and
`storage::entities::knn_search` is now:

1. A full table scan over the relevant BLOB-backed table.
2. A pure-Rust dot product + L2 norms for every row.
3. Sort by distance ascending; truncate to `k`.

For the default embedding dimension of 384 and namespace sizes below 10k
memories, this is O(N × 384) per call, which runs in single-digit
milliseconds on modern hardware. The performance characteristics are
acceptable for the GraphRAG memory use case (recall + hybrid-search
on personal corpora, not million-scale vector search).

## Consequences

### Positive

- Zero external dependencies. The vector search no longer requires the
  `sqlite-vec` C extension to be loadable at runtime.
- Predictable performance. No more vec0 internal allocator weirdness,
  no more `SQLITE_BUSY` on vec table writes, no more KNN with weird
  ORDER BY semantics.
- Cosine similarity is trivial to test in pure Rust — see
  `src/similarity.rs::tests` for 7 unit tests covering edge cases
  (zero vector, mismatched lengths, identical, orthogonal, opposite,
  similarity_to_distance inversion, top_k ordering).

### Negative

- O(N × D) per KNN call. For namespace sizes above 100k memories, this
  becomes the bottleneck. Operators with very large namespaces should
  rely on FTS5 (`hybrid-search`) for coarse filtering before reaching
  the KNN path; see ADR-0024 for the partitioning strategy.
- No more `vec_top_k`, `vec_quantize`, `vec_quantize_binary`, or any
  of the other vec0 built-ins. If the operator needs HNSW-style
  approximate KNN, they should rebuild with `--features embedding-legacy`
  and use the previous vec0 KNN.

## Verification

- `tests/extract_backend`, `tests/spawn_version_adapter`,
  `tests/concurrency_adaptive`: 31 unit tests green.
- `cargo test --lib`: 711 tests green.
- `cargo test --test storage::memories::tests::upsert_vec_and_delete_vec_work`:
  green after the schema swap to `memory_embeddings`.
- `cargo test --test storage::chunks::tests::test_upsert_chunk_vec_and_knn_search`:
  green after the schema swap to `chunk_embeddings`.
