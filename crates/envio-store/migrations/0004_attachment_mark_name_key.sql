-- 0004: the list's attachment mark and the address name key.
-- Separate from 0003 because databases already ran 0003 without them.

-- A non-inline attachment is present: a header-only guess from sync, exact
-- once the body is downloaded. 0 for rows cached before this (the exact
-- value needs the body file; the next body download sets it).
ALTER TABLE messages ADD COLUMN has_attachments INTEGER NOT NULL DEFAULT 0;

-- name lower-cased by Rust (SQLite folds only ASCII); suggest matches it.
-- Existing rows get the ASCII fold; a name seen again is rewritten in full.
ALTER TABLE addresses ADD COLUMN name_key TEXT NOT NULL DEFAULT '';
UPDATE addresses SET name_key = lower(name);
