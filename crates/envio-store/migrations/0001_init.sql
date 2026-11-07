-- 0001: folders, messages and the search index.
-- Accounts are not here: their settings live in accounts.json so a rebuilt
-- cache keeps them. Rows name their account by its key.

CREATE TABLE folders (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    account       TEXT    NOT NULL,
    path          TEXT    NOT NULL,
    role          TEXT    NOT NULL DEFAULT 'other',
    uidvalidity   INTEGER,
    uidnext       INTEGER,
    highestmodseq INTEGER,
    UNIQUE (account, path)
) STRICT;

CREATE TABLE messages (
    id          INTEGER PRIMARY KEY,
    folder_id   INTEGER NOT NULL REFERENCES folders (id) ON DELETE CASCADE,
    uid         INTEGER NOT NULL,
    message_id  TEXT,
    subject     TEXT    NOT NULL DEFAULT '',
    from_addr   TEXT    NOT NULL DEFAULT '',
    to_addrs    TEXT    NOT NULL DEFAULT '',
    date        INTEGER NOT NULL,
    in_reply_to TEXT,
    refs        TEXT    NOT NULL DEFAULT '',
    flags       INTEGER NOT NULL DEFAULT 0,
    preview     TEXT    NOT NULL DEFAULT '',
    body_state  INTEGER NOT NULL DEFAULT 0,
    UNIQUE (folder_id, uid)
) STRICT;

CREATE INDEX messages_by_date ON messages (folder_id, date DESC);
CREATE INDEX messages_by_message_id ON messages (message_id);

CREATE VIRTUAL TABLE messages_fts USING fts5 (
    subject, from_addr, to_addrs, preview,
    content = 'messages', content_rowid = 'id'
);

CREATE TRIGGER messages_fts_insert AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts (rowid, subject, from_addr, to_addrs, preview)
    VALUES (new.id, new.subject, new.from_addr, new.to_addrs, new.preview);
END;

CREATE TRIGGER messages_fts_delete AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts (messages_fts, rowid, subject, from_addr, to_addrs, preview)
    VALUES ('delete', old.id, old.subject, old.from_addr, old.to_addrs, old.preview);
END;

CREATE TRIGGER messages_fts_update
AFTER UPDATE OF subject, from_addr, to_addrs, preview ON messages BEGIN
    INSERT INTO messages_fts (messages_fts, rowid, subject, from_addr, to_addrs, preview)
    VALUES ('delete', old.id, old.subject, old.from_addr, old.to_addrs, old.preview);
    INSERT INTO messages_fts (rowid, subject, from_addr, to_addrs, preview)
    VALUES (new.id, new.subject, new.from_addr, new.to_addrs, new.preview);
END;
