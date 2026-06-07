-- v1.0.76: vec tables no longer created. The V013 migration drops them
-- for existing databases and replaces them with the `memory_embeddings`
-- BLOB-backed tables. Cosine similarity is computed in pure Rust.
--
-- This file is kept (no-op) for migration numbering stability; it must
-- remain in the migrations directory to preserve the v2 -> v13 gap.

SELECT 1;
