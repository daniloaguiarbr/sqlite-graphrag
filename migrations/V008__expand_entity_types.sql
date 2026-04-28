-- v1.0.25: Expand entities.type CHECK constraint to include BERT NER types
-- (organization, location, date) without breaking existing rows.
-- SQLite requires recreating the table to alter CHECK constraints.

PRAGMA foreign_keys = OFF;

CREATE TABLE entities_new (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    namespace   TEXT    NOT NULL,
    name        TEXT    NOT NULL,
    type        TEXT    NOT NULL CHECK(type IN (
        'project', 'tool', 'person', 'file', 'concept',
        'incident', 'decision', 'memory', 'dashboard', 'issue_tracker',
        'organization', 'location', 'date'
    )),
    description TEXT,
    aliases     TEXT    NOT NULL DEFAULT '[]' CHECK(json_valid(aliases)),
    degree      INTEGER NOT NULL DEFAULT 0,
    metadata    TEXT    NOT NULL DEFAULT '{}',
    created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE(namespace, name)
);

INSERT INTO entities_new SELECT * FROM entities;
DROP TABLE entities;
ALTER TABLE entities_new RENAME TO entities;

CREATE INDEX IF NOT EXISTS idx_entities_ns ON entities(namespace);
CREATE INDEX IF NOT EXISTS idx_entities_namespace_degree ON entities(namespace, degree DESC);

PRAGMA foreign_keys = ON;
