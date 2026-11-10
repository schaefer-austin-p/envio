//! The lookups sync's planner and driver need (issue #22): one message by
//! UID, a folder's flags in one read, and a folder by its server path.

use rusqlite::{OptionalExtension, params};

use crate::db::Db;
use crate::error::Result;
use crate::folders::{self, Folder, FolderId};
use crate::messages::{self, Flags, Message};

impl Db {
    /// The cached message with this UID, if any.
    pub fn message_at(&self, folder: FolderId, uid: u32) -> Result<Option<Message>> {
        let message = self
            .conn()
            .query_row(
                &format!(
                    "SELECT {} FROM messages m WHERE m.folder_id = ?1 AND m.uid = ?2",
                    messages::COLUMNS
                ),
                params![folder.0, uid],
                messages::from_row,
            )
            .optional()?;
        Ok(message)
    }

    /// Every cached UID of a folder with its flags, ascending by UID: what
    /// the planner compares with the server's flags.
    pub fn flags_by_uid(&self, folder: FolderId) -> Result<Vec<(u32, Flags)>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT uid, flags FROM messages WHERE folder_id = ?1 ORDER BY uid")?;
        let flags = stmt
            .query_map([folder.0], |row| {
                Ok((row.get(0)?, Flags::from_bits(row.get(1)?)))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(flags)
    }

    /// An account's folder by its path on the server, if it is cached.
    pub fn folder_by_path(&self, account: &str, path: &str) -> Result<Option<Folder>> {
        let folder = self
            .conn()
            .query_row(
                &format!(
                    "SELECT {} FROM folders WHERE account = ?1 AND path = ?2",
                    folders::COLUMNS
                ),
                params![account, path],
                folders::from_row,
            )
            .optional()?;
        Ok(folder)
    }
}
