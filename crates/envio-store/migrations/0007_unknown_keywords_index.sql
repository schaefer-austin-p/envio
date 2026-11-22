-- 0007: a partial index over the rows whose keywords are unknown (#399).

-- Every sync asks whether a cached row in the window has NULL keywords. With
-- no index on that, a folder whose rows are all known was walked row by row
-- on each sync (each IDLE wake). The partial index holds only the unknown
-- rows: empty once the flag check has filled them, so the probe is O(1).
-- Rows and columns are unchanged.
CREATE INDEX messages_unknown_keywords ON messages (folder_id, uid) WHERE keywords IS NULL;
