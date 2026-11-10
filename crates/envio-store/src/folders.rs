//! Folders and what sync last saw of them, per account.

use rusqlite::{OptionalExtension, Row, params};

use crate::db::{Db, found};
use crate::error::{Result, StoreError};

/// A folder's row id in the cache. It doesn't survive a rebuild: anything
/// kept outside the cache names a folder by account key and path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FolderId(pub i64);

/// What a folder is for, from SPECIAL-USE (RFC 6154) or its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderRole {
    Inbox,
    Sent,
    Drafts,
    Trash,
    Archive,
    Junk,
    Other,
}

impl FolderRole {
    fn as_db(self) -> &'static str {
        match self {
            Self::Inbox => "inbox",
            Self::Sent => "sent",
            Self::Drafts => "drafts",
            Self::Trash => "trash",
            Self::Archive => "archive",
            Self::Junk => "junk",
            Self::Other => "other",
        }
    }

    fn from_db(value: &str) -> Self {
        match value {
            "inbox" => Self::Inbox,
            "sent" => Self::Sent,
            "drafts" => Self::Drafts,
            "trash" => Self::Trash,
            "archive" => Self::Archive,
            "junk" => Self::Junk,
            _ => Self::Other,
        }
    }
}

/// What sync last saw of a folder: UIDVALIDITY and UIDNEXT (RFC 3501) and,
/// with CONDSTORE, HIGHESTMODSEQ (RFC 7162, at most 2^63 - 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncState {
    pub uidvalidity: u32,
    pub uidnext: u32,
    pub highestmodseq: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub id: FolderId,
    pub account: String,
    pub path: String,
    pub role: FolderRole,
    /// `None` until the folder's first sync, and again after a reset.
    pub sync: Option<SyncState>,
}

pub(crate) const COLUMNS: &str = "id, account, path, role, uidvalidity, uidnext, highestmodseq";

pub(crate) fn from_row(row: &Row<'_>) -> rusqlite::Result<Folder> {
    let role: String = row.get(3)?;
    let uidvalidity: Option<u32> = row.get(4)?;
    let uidnext: Option<u32> = row.get(5)?;
    let highestmodseq: Option<i64> = row.get(6)?;
    let sync = uidvalidity
        .zip(uidnext)
        .map(|(uidvalidity, uidnext)| SyncState {
            uidvalidity,
            uidnext,
            // Written from a u64 checked to fit in an i64, so never negative.
            highestmodseq: highestmodseq.map(i64::unsigned_abs),
        });
    Ok(Folder {
        id: FolderId(row.get(0)?),
        account: row.get(1)?,
        path: row.get(2)?,
        role: FolderRole::from_db(&role),
        sync,
    })
}

impl Db {
    /// Adds the folder, or updates its role if it's already known.
    pub fn upsert_folder(&self, account: &str, path: &str, role: FolderRole) -> Result<FolderId> {
        let id = self.conn().query_row(
            "INSERT INTO folders (account, path, role) VALUES (?1, ?2, ?3)
             ON CONFLICT (account, path) DO UPDATE SET role = excluded.role
             RETURNING id",
            params![account, path, role.as_db()],
            |row| row.get(0),
        )?;
        Ok(FolderId(id))
    }

    pub fn folders(&self, account: &str) -> Result<Vec<Folder>> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT {COLUMNS} FROM folders WHERE account = ?1 ORDER BY path"
        ))?;
        let folders = stmt
            .query_map([account], from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(folders)
    }

    pub fn folder(&self, id: FolderId) -> Result<Folder> {
        self.conn()
            .query_row(
                &format!("SELECT {COLUMNS} FROM folders WHERE id = ?1"),
                [id.0],
                from_row,
            )
            .optional()?
            .ok_or(StoreError::NotFound)
    }

    pub fn set_sync_state(&self, id: FolderId, state: &SyncState) -> Result<()> {
        let modseq = state
            .highestmodseq
            .map(|modseq| {
                i64::try_from(modseq).map_err(|_| StoreError::BadRecord {
                    file: "folders".into(),
                    reason: format!("HIGHESTMODSEQ {modseq} is over 2^63 - 1"),
                })
            })
            .transpose()?;
        let changed = self.conn().execute(
            "UPDATE folders SET uidvalidity = ?2, uidnext = ?3, highestmodseq = ?4 WHERE id = ?1",
            params![id.0, state.uidvalidity, state.uidnext, modseq],
        )?;
        found(changed)
    }

    /// Drops a folder's messages and sync state, for when its UIDVALIDITY
    /// changed. Pending actions find their messages again by Message-ID.
    /// The folder's bodies on disk go with `Bodies::remove_folder`.
    pub fn reset_folder(&self, id: FolderId) -> Result<()> {
        self.savepoint(|| {
            self.conn()
                .execute("DELETE FROM messages WHERE folder_id = ?1", [id.0])?;
            let changed = self.conn().execute(
                "UPDATE folders SET uidvalidity = NULL, uidnext = NULL, highestmodseq = NULL
                 WHERE id = ?1",
                [id.0],
            )?;
            found(changed)
        })
    }

    /// Removes a folder and its messages (it was deleted on the server).
    pub fn remove_folder(&self, id: FolderId) -> Result<()> {
        found(
            self.conn()
                .execute("DELETE FROM folders WHERE id = ?1", [id.0])?,
        )
    }

    /// Drops everything cached for an account, when the account is removed.
    /// Returns the removed folders' ids, so the caller can call
    /// `Bodies::remove_folder` for each.
    pub fn remove_account_cache(&self, account: &str) -> Result<Vec<FolderId>> {
        let mut statement = self
            .conn()
            .prepare("DELETE FROM folders WHERE account = ?1 RETURNING id")?;
        let ids = statement
            .query_map([account], |row| row.get(0).map(FolderId))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(ids)
    }
}
