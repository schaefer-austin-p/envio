-- 0002: each message's size in bytes (RFC822.SIZE), so a body download gets
-- a time limit that grows with the message. 0 for rows cached before this.

ALTER TABLE messages ADD COLUMN size INTEGER NOT NULL DEFAULT 0;
