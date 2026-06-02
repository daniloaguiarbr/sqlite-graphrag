-- G09: Add timestamps to relationships table for parity with entities and memories.
-- Prevents copy-paste bugs where queries reference updated_at on relationships.
-- Default values ensure zero-cost for existing INSERT statements.

ALTER TABLE relationships ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE relationships ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;

-- Backfill existing rows with current timestamp.
UPDATE relationships SET created_at = unixepoch(), updated_at = unixepoch() WHERE created_at = 0;

-- Auto-update trigger for updated_at on relationship modification.
CREATE TRIGGER IF NOT EXISTS trg_relationships_updated_at
AFTER UPDATE ON relationships
FOR EACH ROW
BEGIN
    UPDATE relationships SET updated_at = unixepoch() WHERE id = NEW.id;
END;
