//! Which messages' bodies are worth downloading before they are opened.

use rusqlite::params;

use crate::db::Db;
use crate::error::Result;
use crate::folders::FolderId;
use crate::messages::MessageRowId;

impl Db {
    /// Messages of `folder` with no body yet, dated `since` (unix seconds,
    /// as the `date` column) or later and at most `max_size` bytes, newest
    /// first.
    #[allow(clippy::too_many_arguments)] // the signature the plan fixes
    pub fn prefetch_candidates(
        &self,
        folder: FolderId,
        since: i64,
        max_size: u32,
        limit: u32,
    ) -> Result<Vec<MessageRowId>> {
        let mut stmt = self.conn().prepare(
            "SELECT id FROM messages
             WHERE folder_id = ?1 AND body_state = 0 AND date >= ?2 AND size <= ?3
             ORDER BY date DESC, id DESC LIMIT ?4",
        )?;
        let ids = stmt
            .query_map(params![folder.0, since, max_size, limit], |row| {
                Ok(MessageRowId(row.get(0)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(ids)
    }
}
