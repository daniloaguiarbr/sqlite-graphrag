-- v1.0.74 introduced sqlite-vec virtual tables for vector search.
-- v1.0.76 removes sqlite-vec and replaces these tables with the
-- BLOB-backed `memory_embeddings` / `entity_embeddings` /
-- `chunk_embeddings` tables in migration V013. The V002 DDL is kept
-- here unchanged for historical accuracy (refinery enforces
-- checksum stability on applied migrations); V013 drops the
-- tables on the next upgrade.

CREATE VIRTUAL TABLE IF NOT EXISTS vec_memories USING vec0(
    memory_id INTEGER PRIMARY KEY,
    embedding float[384] distance_metric=cosine,
    namespace TEXT PARTITION KEY,
    type TEXT PARTITION KEY
);

CREATE VIRTUAL TABLE IF NOT EXISTS vec_entities USING vec0(
    entity_id INTEGER PRIMARY KEY,
    embedding float[384] distance_metric=cosine,
    namespace TEXT PARTITION KEY,
    type TEXT PARTITION KEY
);

CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks USING vec0(
    chunk_id INTEGER PRIMARY KEY,
    memory_id INTEGER,
    embedding float[384] distance_metric=cosine
);
