-- CrossTrace normalized store. Everything imported from any platform lands
-- here. SQLite with FTS5 gives offline full-text search with no server.

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS sources (
    id           INTEGER PRIMARY KEY,
    name         TEXT NOT NULL,
    path         TEXT NOT NULL,
    kind         TEXT NOT NULL,
    platform     TEXT NOT NULL DEFAULT 'unknown',
    record_count INTEGER NOT NULL DEFAULT 0,
    imported_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS records (
    id         INTEGER PRIMARY KEY,
    source_id  INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    kind       TEXT NOT NULL,
    platform   TEXT NOT NULL,
    timestamp  INTEGER,
    title      TEXT,
    body       TEXT,
    raw        TEXT NOT NULL DEFAULT 'null'
);

CREATE INDEX IF NOT EXISTS idx_records_ts ON records(timestamp);
CREATE INDEX IF NOT EXISTS idx_records_source ON records(source_id);
CREATE INDEX IF NOT EXISTS idx_records_platform ON records(platform);

-- Full-text index over the searchable columns. `content=records` keeps it an
-- external-content index so we don't duplicate the body text.
CREATE VIRTUAL TABLE IF NOT EXISTS records_fts USING fts5(
    title,
    body,
    platform,
    content='records',
    content_rowid='id',
    tokenize='unicode61'
);

-- Keep the FTS index in sync with the base table.
CREATE TRIGGER IF NOT EXISTS records_ai AFTER INSERT ON records BEGIN
    INSERT INTO records_fts(rowid, title, body, platform)
    VALUES (new.id, new.title, new.body, new.platform);
END;
CREATE TRIGGER IF NOT EXISTS records_ad AFTER DELETE ON records BEGIN
    INSERT INTO records_fts(records_fts, rowid, title, body, platform)
    VALUES ('delete', old.id, old.title, old.body, old.platform);
END;

CREATE TABLE IF NOT EXISTS entities (
    id           INTEGER PRIMARY KEY,
    kind         TEXT NOT NULL,
    value        TEXT NOT NULL,
    display_name TEXT,
    UNIQUE(kind, value)
);

CREATE INDEX IF NOT EXISTS idx_entities_value ON entities(value);

-- Many-to-many: which records reference which entities, and in what role
-- (e.g. sender / recipient / mentioned).
CREATE TABLE IF NOT EXISTS record_entities (
    record_id INTEGER NOT NULL REFERENCES records(id) ON DELETE CASCADE,
    entity_id INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    role      TEXT NOT NULL DEFAULT 'mentioned',
    PRIMARY KEY (record_id, entity_id, role)
);

CREATE INDEX IF NOT EXISTS idx_re_entity ON record_entities(entity_id);
CREATE INDEX IF NOT EXISTS idx_re_record ON record_entities(record_id);

-- Append-only audit log of processing steps (privacy requirement).
CREATE TABLE IF NOT EXISTS audit_log (
    id      INTEGER PRIMARY KEY,
    at      INTEGER NOT NULL,
    action  TEXT NOT NULL,
    detail  TEXT
);
