-- 0003: body search, seen addresses and the keyset-list index.
-- bodies_fts holds the first 64 KiB of a downloaded body's text, keyed by the
-- message's row id; the trigger drops it with the message. addresses is a
-- cache of the people mail came from or went to, refilled by sync.

CREATE VIRTUAL TABLE bodies_fts USING fts5 (text);

CREATE TRIGGER bodies_fts_delete AFTER DELETE ON messages BEGIN
    DELETE FROM bodies_fts WHERE rowid = old.id;
END;

CREATE TABLE addresses (
    address   TEXT    PRIMARY KEY,
    name      TEXT    NOT NULL DEFAULT '',
    -- name lower-cased by Rust (SQLite folds only ASCII); suggest matches it.
    name_key  TEXT    NOT NULL DEFAULT '',
    last_seen INTEGER NOT NULL,
    count     INTEGER NOT NULL DEFAULT 1
) STRICT;

DROP INDEX messages_by_date;
CREATE INDEX messages_by_folder_date ON messages (folder_id, date DESC, id DESC);
