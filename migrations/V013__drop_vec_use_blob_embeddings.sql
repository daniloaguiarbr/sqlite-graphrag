-- v1.0.76: drop sqlite-vec virtual tables and replace them with a
-- regular BLOB-backed `memory_embeddings` table. Cosine similarity is
-- computed in pure Rust on demand (see src/embedder.rs and
-- src/recall_engine.rs). This migration is a NO-OP for fresh databases
-- (the vec tables never existed) and a one-way DESTRUCTIVE migration
-- for databases upgraded from v1.0.74 or earlier.
--
-- WARNING: existing vectors are LOST. The CLI re-embeds lazily on the
-- next remember / ingest / edit, but the old KNN results are
-- unavailable until the new embeddings are written. Operators who want
-- to preserve old vectors can run `migrate --to-llm-only --keep-vec`
-- first, dump the vec tables, and re-import after rebuild.

DROP TABLE IF EXISTS vec_memories;
DROP TABLE IF EXISTS vec_entities;
DROP TABLE IF EXISTS vec_chunks;

CREATE TABLE IF NOT EXISTS memory_embeddings (
    memory_id    INTEGER PRIMARY KEY REFERENCES memories(id) ON DELETE CASCADE,
    namespace    TEXT NOT NULL,
    embedding    BLOB NOT NULL,            -- 384 * 4 = 1536 bytes little-endian f32
    source       TEXT NOT NULL,            -- "llm-claude" / "llm-codex" / "legacy-fastembed"
    model        TEXT NOT NULL,            -- "claude-sonnet-4-6" / "gpt-5.4" / "multilingual-e5-small"
    dim          INTEGER NOT NULL DEFAULT 384,
    created_at   TEXT NOT NULL DEFAULT (CAST(unixepoch() AS TEXT)),
    updated_at   TEXT NOT NULL DEFAULT (CAST(unixepoch() AS TEXT))
);

CREATE INDEX IF NOT EXISTS idx_memory_embeddings_ns ON memory_embeddings(namespace);
CREATE INDEX IF NOT EXISTS idx_memory_embeddings_source ON memory_embeddings(source);

CREATE TABLE IF NOT EXISTS entity_embeddings (
    entity_id    INTEGER PRIMARY KEY REFERENCES entities(id) ON DELETE CASCADE,
    namespace    TEXT NOT NULL,
    embedding    BLOB NOT NULL,
    source       TEXT NOT NULL,
    model        TEXT NOT NULL,
    dim          INTEGER NOT NULL DEFAULT 384,
    created_at   TEXT NOT NULL DEFAULT (CAST(unixepoch() AS TEXT)),
    updated_at   TEXT NOT NULL DEFAULT (CAST(unixepoch() AS TEXT))
);

CREATE INDEX IF NOT EXISTS idx_entity_embeddings_ns ON entity_embeddings(namespace);

CREATE TABLE IF NOT EXISTS chunk_embeddings (
    chunk_id     INTEGER PRIMARY KEY REFERENCES memory_chunks(id) ON DELETE CASCADE,
    memory_id    INTEGER NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    embedding    BLOB NOT NULL,
    source       TEXT NOT NULL,
    model        TEXT NOT NULL,
    dim          INTEGER NOT NULL DEFAULT 384,
    created_at   TEXT NOT NULL DEFAULT (CAST(unixepoch() AS TEXT))
);

CREATE INDEX IF NOT EXISTS idx_chunk_embeddings_memory ON chunk_embeddings(memory_id);

-- FTS5 stays the workhorse for exact-match and prefix search; the
-- `embeddings_*` tables are scanned in Rust for vector similarity.
INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('vec_engine', 'rust-cosine');
INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('embedding_default_dim', '384');
