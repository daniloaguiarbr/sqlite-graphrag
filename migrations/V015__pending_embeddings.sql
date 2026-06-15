-- V015__pending_embeddings.sql
-- GAP-005 (v1.0.82): fila de re-embedding para --skip-embedding-on-failure
-- Adiciona tabela `pending_embeddings` que registra memórias persistidas com embedding NULL
-- para reprocessamento posterior via `embedding retry` ou `enrich --operation re-embed`.
-- FK CASCADE garante limpeza automática quando a memória é purgada.
-- last_stderr_tail preserva informação de diagnóstico do crash original (1KB max).

CREATE TABLE IF NOT EXISTS pending_embeddings (
    pending_id        INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_id         INTEGER NOT NULL,
    namespace         TEXT    NOT NULL DEFAULT 'global',
    name              TEXT    NOT NULL,
    backend_chain     TEXT    NOT NULL,
    last_error        TEXT,
    last_exit_code    INTEGER,
    last_stderr_tail  TEXT,
    attempt_count     INTEGER NOT NULL DEFAULT 0,
    status            TEXT    NOT NULL CHECK(status IN
                        ('pending', 'in_progress', 'done', 'abandoned'))
                        DEFAULT 'pending',
    created_at        INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at        INTEGER NOT NULL DEFAULT (unixepoch()),
    FOREIGN KEY (memory_id) REFERENCES memories(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_pending_embeddings_status
    ON pending_embeddings(status, updated_at);

CREATE INDEX IF NOT EXISTS idx_pending_embeddings_memory
    ON pending_embeddings(memory_id);
