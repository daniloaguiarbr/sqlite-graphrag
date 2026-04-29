-- v1.0.30: Expand memories.type CHECK constraint to include 'document' and 'note'.
--
-- The CLI enum (cli.rs MemoryType) accepts nine values, but V001/V006 kept the
-- CHECK constraint at seven, causing 'document' and 'note' to be rejected at
-- runtime with exit 10. This migration aligns the schema with the public CLI
-- contract used in README and clap help examples.
--
-- SQLite cannot alter CHECK constraints in place. Following the V006 pattern,
-- we recreate the memories table along with its direct child tables to keep
-- foreign keys pointing at the new table after legacy_alter_table renaming.

PRAGMA foreign_keys = OFF;
PRAGMA legacy_alter_table = ON;

DROP TRIGGER IF EXISTS trg_memories_updated_at;
DROP TRIGGER IF EXISTS trg_fts_ai;
DROP TRIGGER IF EXISTS trg_fts_ad;

ALTER TABLE memories RENAME TO memories_v008_type_check;
ALTER TABLE memory_versions RENAME TO memory_versions_v008_type_check;
ALTER TABLE memory_chunks RENAME TO memory_chunks_v008_type_check;
ALTER TABLE memory_entities RENAME TO memory_entities_v008_type_check;
ALTER TABLE memory_relationships RENAME TO memory_relationships_v008_type_check;
ALTER TABLE memory_urls RENAME TO memory_urls_v008_type_check;

CREATE TABLE memories (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    namespace   TEXT    NOT NULL DEFAULT 'global',
    name        TEXT    NOT NULL,
    type        TEXT    NOT NULL CHECK(type IN ('user','feedback','project','reference','decision','incident','skill','document','note')),
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

INSERT INTO memories (
    id,
    namespace,
    name,
    type,
    description,
    body,
    body_hash,
    session_id,
    source,
    metadata,
    deleted_at,
    created_at,
    updated_at
)
SELECT
    id,
    namespace,
    name,
    type,
    description,
    body,
    body_hash,
    session_id,
    source,
    metadata,
    deleted_at,
    created_at,
    updated_at
FROM memories_v008_type_check;

CREATE TABLE memory_versions (
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

INSERT INTO memory_versions (
    id,
    memory_id,
    version,
    name,
    type,
    description,
    body,
    metadata,
    changed_by,
    change_reason,
    created_at
)
SELECT
    id,
    memory_id,
    version,
    name,
    type,
    description,
    body,
    metadata,
    changed_by,
    change_reason,
    created_at
FROM memory_versions_v008_type_check;

CREATE TABLE memory_chunks (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_id    INTEGER NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    chunk_idx    INTEGER NOT NULL,
    chunk_text   TEXT    NOT NULL,
    start_offset INTEGER NOT NULL,
    end_offset   INTEGER NOT NULL,
    token_count  INTEGER NOT NULL,
    UNIQUE(memory_id, chunk_idx)
);

INSERT INTO memory_chunks (
    id,
    memory_id,
    chunk_idx,
    chunk_text,
    start_offset,
    end_offset,
    token_count
)
SELECT
    id,
    memory_id,
    chunk_idx,
    chunk_text,
    start_offset,
    end_offset,
    token_count
FROM memory_chunks_v008_type_check;

CREATE TABLE memory_entities (
    memory_id  INTEGER NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    entity_id  INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    PRIMARY KEY (memory_id, entity_id)
);

INSERT INTO memory_entities (memory_id, entity_id)
SELECT memory_id, entity_id
FROM memory_entities_v008_type_check;

CREATE TABLE memory_relationships (
    memory_id       INTEGER NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    relationship_id INTEGER NOT NULL REFERENCES relationships(id) ON DELETE CASCADE,
    PRIMARY KEY (memory_id, relationship_id)
);

INSERT INTO memory_relationships (memory_id, relationship_id)
SELECT memory_id, relationship_id
FROM memory_relationships_v008_type_check;

CREATE TABLE memory_urls (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_id   INTEGER NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    url         TEXT    NOT NULL CHECK(length(url) >= 10 AND length(url) <= 4096),
    url_offset  INTEGER,
    created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE(memory_id, url)
);

INSERT INTO memory_urls (id, memory_id, url, url_offset, created_at)
SELECT id, memory_id, url, url_offset, created_at
FROM memory_urls_v008_type_check;

DROP TABLE memory_urls_v008_type_check;
DROP TABLE memory_relationships_v008_type_check;
DROP TABLE memory_entities_v008_type_check;
DROP TABLE memory_chunks_v008_type_check;
DROP TABLE memory_versions_v008_type_check;
DROP TABLE memories_v008_type_check;

CREATE INDEX IF NOT EXISTS idx_memories_ns_type  ON memories(namespace, type);
CREATE INDEX IF NOT EXISTS idx_memories_ns_live  ON memories(namespace) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_memories_body_hash ON memories(body_hash);
CREATE INDEX IF NOT EXISTS idx_memory_chunks_memory_id ON memory_chunks(memory_id);
CREATE INDEX IF NOT EXISTS idx_memory_relationships_relationship_id ON memory_relationships(relationship_id);
CREATE INDEX IF NOT EXISTS idx_me_entity ON memory_entities(entity_id);
CREATE INDEX IF NOT EXISTS idx_memory_urls_memory_id ON memory_urls(memory_id);
CREATE INDEX IF NOT EXISTS idx_memory_urls_url ON memory_urls(url);

CREATE TRIGGER IF NOT EXISTS trg_memories_updated_at
AFTER UPDATE ON memories FOR EACH ROW
BEGIN
    UPDATE memories SET updated_at = unixepoch() WHERE id = OLD.id;
END;

CREATE TRIGGER IF NOT EXISTS trg_fts_ai
AFTER INSERT ON memories WHEN NEW.deleted_at IS NULL BEGIN
    INSERT INTO fts_memories(rowid, name, description, body)
    VALUES (NEW.id, NEW.name, NEW.description, NEW.body);
END;

CREATE TRIGGER IF NOT EXISTS trg_fts_ad
AFTER DELETE ON memories BEGIN
    INSERT INTO fts_memories(fts_memories, rowid, name, description, body)
    VALUES('delete', OLD.id, OLD.name, OLD.description, OLD.body);
END;

INSERT INTO fts_memories(fts_memories) VALUES('rebuild');

PRAGMA legacy_alter_table = OFF;
PRAGMA foreign_keys = ON;
