# ADR-0022: BLOB-Backed Embeddings (v1.0.76)

- Status: Accepted (2026-06-07)
- Deciders: Danilo Aguiar
- Scope: migrations/V013, src/storage/connection.rs, src/storage/memories.rs, src/storage/entities.rs, src/storage/chunks.rs

## Context

In v1.0.74, the embedding vectors were stored in three vec0 virtual
tables (`vec_memories`, `vec_entities`, `vec_chunks`) provided by the
`sqlite-vec` extension. These tables:

- Required a C extension to be loaded at startup via
  `sqlite3_auto_extension`.
- Did not support `INSERT OR REPLACE` (forced the codebase to do
  `DELETE` + `INSERT` for every embedding write).
- Lacked FK CASCADE, forcing explicit cleanup in the storage layer
  (`vec0 lacks FK CASCADE — clean vec_entities explicitly` was a
  comment in the codebase).
- Stored the embedding as a `float[384]` opaque blob, with no
  metadata about which model produced it.

## Decision

Migration V013 (`migrations/V013__drop_vec_use_blob_embeddings.sql`)
drops all three vec tables and creates three regular BLOB-backed
tables:

- `memory_embeddings(memory_id PK, namespace, embedding BLOB, source,
   model, dim, created_at, updated_at)`
- `entity_embeddings(entity_id PK, namespace, embedding BLOB, source,
   model, dim, created_at, updated_at)`
- `chunk_embeddings(chunk_id PK, memory_id, embedding BLOB, source,
   model, dim, created_at)`

The `embedding` column is a 384 × 4 = 1536 byte little-endian f32
sequence, produced by `embedder::f32_to_bytes` and consumed by
`embedder::bytes_to_f32`. The `source` column is one of
`"llm-claude"`, `"llm-codex"`, or `"legacy-fastembed"` (the last only
when the `embedding-legacy` feature is enabled); the `model` column
stores the LLM model name (e.g. `claude-sonnet-4-6`).

The `source` and `model` columns enable the operator to audit which
LLM produced each embedding. This was impossible with vec0 because
the float array was opaque.

## Consequences

### Positive

- No external extension. The CLI no longer requires `sqlite-vec` to
  be loadable at runtime; the embedding tables are plain SQLite.
- FK CASCADE works. Deleting a memory via `DELETE FROM memories`
  automatically cleans up `memory_embeddings` via the
  `ON DELETE CASCADE` clause.
- INSERT OR REPLACE works. The single-line
  `INSERT OR REPLACE INTO memory_embeddings (...) VALUES (...)` is
  atomic and idempotent.
- `INSERT OR REPLACE INTO chunk_embeddings(chunk_id, memory_id, embedding, source, model, dim)`
  is the new canonical write path, with all metadata in one row.
- The `source` column lets the operator inspect the corpus for
  LLM-version skew (e.g. "all embeddings before 2026-05-01 used
  claude-sonnet-4-5, all after used claude-sonnet-4-6").

### Negative

- The KNN search is O(N × D) per call (see ADR-0020). For namespace
  sizes above 100k memories, the operator should partition by
  namespace or by date and rely on FTS5 for coarse filtering.
- Existing v1.0.74 databases lose their vec-table embeddings on
  migration. The re-embedding is lazy (the next `remember` /
  `edit` / `ingest` re-embeds the affected memory), but operators
  with millions of pre-existing memories should plan a batch
  re-ingest or use the `embedding-legacy` feature for the
  transition window.

## Verification

- `cargo test --lib`: 711 tests pass.
- `cargo test --lib storage::memories::tests::upsert_vec_and_delete_vec_work`:
  green — the new `memory_embeddings` table accepts upsert and
  delete correctly.
- `cargo test --lib storage::entities::tests::upsert_entity_vec_replaces`:
  green — the new `entity_embeddings` table behaves the same.
- `cargo test --lib storage::chunks::tests::test_upsert_chunk_vec_and_knn_search`:
  green — the new `chunk_embeddings` table round-trips.
