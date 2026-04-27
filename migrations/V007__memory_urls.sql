-- Tabela dedicada para URLs extraídas de corpos de memória.
--
-- URLs eram inseridas como entidades com entity_type='concept', poluindo o grafo
-- (26.3% dos nós em v1.0.23). Esta migração cria armazenamento separado e
-- idempotente para URLs, preservando offset de origem e deduplicação por memória.

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
