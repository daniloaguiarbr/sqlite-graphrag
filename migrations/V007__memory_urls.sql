-- Dedicated table for URLs extracted from memory bodies.
--
-- URLs were previously inserted as entities with entity_type='concept', polluting
-- the graph (26.3% of nodes in v1.0.23). This migration creates separate, idempotent
-- storage for URLs, preserving source offset and per-memory deduplication.

CREATE TABLE IF NOT EXISTS memory_urls (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_id   INTEGER NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    url         TEXT    NOT NULL CHECK(length(url) >= 10 AND length(url) <= 4096),
    url_offset  INTEGER,
    created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE(memory_id, url)
);

CREATE INDEX IF NOT EXISTS idx_memory_urls_memory_id
    ON memory_urls(memory_id);

CREATE INDEX IF NOT EXISTS idx_memory_urls_url
    ON memory_urls(url);
