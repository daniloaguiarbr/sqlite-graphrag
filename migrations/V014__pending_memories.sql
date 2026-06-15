-- V014__pending_memories.sql
-- GAP-001 (v1.0.82): persistência por estágios com checkpoint retomável
-- Adiciona tabela `pending_memories` para suportar Estágio A (validate) → Estágio B (embed) → Estágio C (commit)
-- Permite ao `remember` retomar de onde parou após SIGTERM/OOM sem perder body validado.
-- UNIQUE(namespace, name) garante idempotência (mesma chave de memória não fica duplicada em pending).
-- ON DELETE CASCADE NÃO é usado: pending é independente de memories até commit.

CREATE TABLE IF NOT EXISTS pending_memories (
    pending_id        INTEGER PRIMARY KEY AUTOINCREMENT,
    name              TEXT    NOT NULL,
    namespace         TEXT    NOT NULL DEFAULT 'global',
    memory_type       TEXT    NOT NULL CHECK(memory_type IN
                        ('user','feedback','project','reference',
                         'decision','incident','skill','document','note')),
    description       TEXT,
    body              BLOB    NOT NULL,
    body_hash         TEXT    NOT NULL,
    entities_json     TEXT,
    relationships_json TEXT,
    status            TEXT    NOT NULL CHECK(status IN
                        ('validated', 'embedding_in_progress', 'embedding_done',
                         'committed', 'abandoned', 'failed'))
                        DEFAULT 'validated',
    embedding         BLOB,
    embedding_dim     INTEGER,
    attempt_count     INTEGER NOT NULL DEFAULT 0,
    last_error        TEXT,
    created_at        INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at        INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE (namespace, name)
);

CREATE INDEX IF NOT EXISTS idx_pending_memories_status_updated
    ON pending_memories(status, updated_at);

CREATE INDEX IF NOT EXISTS idx_pending_memories_namespace
    ON pending_memories(namespace, status);
