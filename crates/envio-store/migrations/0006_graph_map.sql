-- 0006: the uid map of the Microsoft Graph backend.

-- Graph has no uids, so envio mints them. One row per Graph folder: the
-- generation (reported as uid_validity; non-zero u32, new on every reset),
-- the next uid of the high range [2^31, 2^32) and of the low range
-- [1, 2^31) (counting down), the folder's change counter, and the delta link
-- of the last committed sync. Both tables go with their folder row.
CREATE TABLE graph_folders (
    folder_row_id INTEGER PRIMARY KEY REFERENCES folders (id) ON DELETE CASCADE,
    graph_id      TEXT    NOT NULL,
    generation    INTEGER NOT NULL,
    uid_next      INTEGER NOT NULL,
    low_next      INTEGER NOT NULL,
    modseq        INTEGER NOT NULL,
    delta_link    TEXT    NOT NULL DEFAULT ''
) STRICT;

-- Graph folder ids are immutable and global, so one id is one folder row.
CREATE UNIQUE INDEX graph_folders_graph_id ON graph_folders (graph_id);

-- graph_id <-> uid, with what the delta feed reports: flags (the Flags
-- bits), categories (a JSON array) and the modseq of their last change.
CREATE TABLE graph_messages (
    folder_row_id INTEGER NOT NULL REFERENCES folders (id) ON DELETE CASCADE,
    graph_id      TEXT    NOT NULL,
    uid           INTEGER NOT NULL,
    received_at   INTEGER NOT NULL,
    flags         INTEGER NOT NULL,
    categories    TEXT    NOT NULL,
    modseq        INTEGER NOT NULL,
    PRIMARY KEY (folder_row_id, uid),
    UNIQUE (folder_row_id, graph_id)
) STRICT;
