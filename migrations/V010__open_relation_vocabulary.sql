-- v1.0.49: Remove CHECK constraint on relationships.relation to allow
-- extensible relation vocabulary beyond the original 12 canonical values.
-- SQLite cannot alter CHECK constraints in place; full table rebuild required.
-- Pattern follows V008__expand_entity_types.sql.

PRAGMA foreign_keys = OFF;

CREATE TABLE relationships_new (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    namespace   TEXT    NOT NULL,
    source_id   INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    target_id   INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    relation    TEXT    NOT NULL,
    weight      REAL    NOT NULL DEFAULT 0.5 CHECK(weight BETWEEN 0.0 AND 1.0),
    description TEXT,
    metadata    TEXT    NOT NULL DEFAULT '{}',
    UNIQUE(source_id, target_id, relation)
);

INSERT INTO relationships_new SELECT * FROM relationships;
DROP TABLE relationships;
ALTER TABLE relationships_new RENAME TO relationships;

CREATE INDEX IF NOT EXISTS idx_relationships_ns ON relationships(namespace);
CREATE INDEX IF NOT EXISTS idx_relationships_source ON relationships(source_id);
CREATE INDEX IF NOT EXISTS idx_relationships_target ON relationships(target_id);

PRAGMA foreign_key_check;
PRAGMA foreign_keys = ON;
