-- PRAGMA auto_vacuum aplicado em pragmas.rs ANTES deste ponto

CREATE TABLE IF NOT EXISTS schema_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS memories (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    namespace   TEXT    NOT NULL DEFAULT 'global',
    name        TEXT    NOT NULL,
    type        TEXT    NOT NULL CHECK(type IN ('user','feedback','project','reference','decision','incident','skill')),
    description TEXT    NOT NULL CHECK(length(description) <= 500),
    body        TEXT    NOT NULL CHECK(length(CAST(body AS BLOB)) <= 512000),
    body_hash   TEXT    NOT NULL,
    session_id  TEXT,
    source      TEXT    NOT NULL DEFAULT 'agent' CHECK(source IN ('agent','user','system','import','sync')),
    metadata    TEXT    NOT NULL DEFAULT '{}' CHECK(json_valid(metadata)),
    deleted_at  INTEGER,
    created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE(namespace, name)
);

CREATE INDEX IF NOT EXISTS idx_memories_ns_type  ON memories(namespace, type);
CREATE INDEX IF NOT EXISTS idx_memories_ns_live  ON memories(namespace) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_memories_body_hash ON memories(body_hash);

CREATE TRIGGER IF NOT EXISTS trg_memories_updated_at
AFTER UPDATE ON memories FOR EACH ROW
BEGIN
    UPDATE memories SET updated_at = unixepoch() WHERE id = OLD.id;
END;

CREATE TABLE IF NOT EXISTS memory_versions (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_id     INTEGER NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    version       INTEGER NOT NULL,
    name          TEXT    NOT NULL,
    type          TEXT    NOT NULL,
    description   TEXT    NOT NULL,
    body          TEXT    NOT NULL,
    metadata      TEXT    NOT NULL DEFAULT '{}',
    changed_by    TEXT,
    change_reason TEXT    NOT NULL DEFAULT 'create' CHECK(change_reason IN ('create','edit','rename','dedup_merge','restore','import_merge')),
    created_at    INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE(memory_id, version)
);

CREATE TABLE IF NOT EXISTS memory_chunks (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_id    INTEGER NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    chunk_idx    INTEGER NOT NULL,
    chunk_text   TEXT    NOT NULL,
    start_offset INTEGER NOT NULL,
    end_offset   INTEGER NOT NULL,
    token_count  INTEGER NOT NULL,
    UNIQUE(memory_id, chunk_idx)
);

CREATE TABLE IF NOT EXISTS entities (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    namespace   TEXT    NOT NULL,
    name        TEXT    NOT NULL,
    type        TEXT    NOT NULL CHECK(type IN ('project','tool','person','file','concept','incident','decision','memory','dashboard','issue_tracker')),
    description TEXT,
    aliases     TEXT    NOT NULL DEFAULT '[]' CHECK(json_valid(aliases)),
    degree      INTEGER NOT NULL DEFAULT 0,
    metadata    TEXT    NOT NULL DEFAULT '{}',
    created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE(namespace, name)
);

CREATE INDEX IF NOT EXISTS idx_entities_ns ON entities(namespace);

CREATE TABLE IF NOT EXISTS relationships (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    namespace   TEXT    NOT NULL,
    source_id   INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    target_id   INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    relation    TEXT    NOT NULL CHECK(relation IN ('applies_to','uses','depends_on','causes','fixes','contradicts','supports','follows','related','mentions','replaces','tracked_in')),
    weight      REAL    NOT NULL DEFAULT 0.5 CHECK(weight BETWEEN 0.0 AND 1.0),
    description TEXT,
    metadata    TEXT    NOT NULL DEFAULT '{}',
    UNIQUE(source_id, target_id, relation)
);

CREATE TABLE IF NOT EXISTS memory_entities (
    memory_id  INTEGER NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    entity_id  INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    PRIMARY KEY (memory_id, entity_id)
);

CREATE INDEX IF NOT EXISTS idx_me_entity ON memory_entities(entity_id);

CREATE TABLE IF NOT EXISTS memory_relationships (
    memory_id       INTEGER NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    relationship_id INTEGER NOT NULL REFERENCES relationships(id) ON DELETE CASCADE,
    PRIMARY KEY (memory_id, relationship_id)
);
