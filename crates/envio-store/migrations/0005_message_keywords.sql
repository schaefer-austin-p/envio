-- 0005: the IMAP keywords of a cached message.

-- Keywords ($Forwarded, $Label1, a Gmail label, ...) sorted and joined by one
-- space (a keyword is an atom, so it holds no space). NULL means unknown, and
-- unknown counts as "may carry keywords": rows cached before this migration
-- stay NULL until a flag check fills them. '' means the server listed none.
ALTER TABLE messages ADD COLUMN keywords TEXT;
