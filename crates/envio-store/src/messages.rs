//! Message metadata: envelope, flags, preview, thread references and
//! whether the body is on disk.

use std::ops::BitOr;

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use crate::db::{Db, found};
use crate::error::{Result, StoreError};
use crate::folders::FolderId;

/// A message's row id in the cache (not its Message-ID header). It doesn't
/// survive a rebuild or a UIDVALIDITY change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MessageRowId(pub i64);

/// The IMAP system flags envio uses (RFC 3501 §2.3.2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Flags(u8);

impl Flags {
    pub const SEEN: Self = Self(1);
    pub const ANSWERED: Self = Self(1 << 1);
    pub const FLAGGED: Self = Self(1 << 2);
    pub const DELETED: Self = Self(1 << 3);
    pub const DRAFT: Self = Self(1 << 4);
    const ALL: u8 = 0b1_1111;

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Unknown bits are dropped.
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits & Self::ALL)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

impl BitOr for Flags {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        self.with(other)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Envelope {
    /// The Message-ID header with its angle brackets, if the message has one.
    pub message_id: Option<String>,
    pub subject: String,
    /// The From header as shown, e.g. `Ada <ada@example.org>`.
    pub from: String,
    /// The To header as shown.
    pub to: String,
    /// Unix seconds.
    pub date: i64,
    pub in_reply_to: Option<String>,
    /// The References header's Message-IDs, oldest first.
    pub references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMessage {
    pub uid: u32,
    pub envelope: Envelope,
    pub flags: Flags,
    pub preview: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyState {
    Missing,
    Downloaded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: MessageRowId,
    pub folder: FolderId,
    pub uid: u32,
    pub envelope: Envelope,
    pub flags: Flags,
    pub preview: String,
    pub body: BodyState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Page {
    pub offset: u32,
    pub limit: u32,
}

/// The columns `from_row` reads, from `messages` aliased as `m`.
pub(crate) const COLUMNS: &str = "m.id, m.folder_id, m.uid, m.message_id, m.subject, \
     m.from_addr, m.to_addrs, m.date, m.in_reply_to, m.refs, m.flags, m.preview, m.body_state";

pub(crate) fn from_row(row: &Row<'_>) -> rusqlite::Result<Message> {
    let refs: String = row.get(9)?;
    let body: i64 = row.get(12)?;
    Ok(Message {
        id: MessageRowId(row.get(0)?),
        folder: FolderId(row.get(1)?),
        uid: row.get(2)?,
        envelope: Envelope {
            message_id: row.get(3)?,
            subject: row.get(4)?,
            from: row.get(5)?,
            to: row.get(6)?,
            date: row.get(7)?,
            in_reply_to: row.get(8)?,
            references: refs.split_whitespace().map(str::to_owned).collect(),
        },
        flags: Flags::from_bits(row.get(10)?),
        preview: row.get(11)?,
        body: if body == 0 {
            BodyState::Missing
        } else {
            BodyState::Downloaded
        },
    })
}

impl Db {
    /// Adds a message, or updates it by UID. An update keeps the body state.
    pub fn upsert_message(&self, folder: FolderId, message: &NewMessage) -> Result<MessageRowId> {
        let envelope = &message.envelope;
        let id = self.conn().query_row(
            "INSERT INTO messages (folder_id, uid, message_id, subject, from_addr, to_addrs,
                                   date, in_reply_to, refs, flags, preview)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT (folder_id, uid) DO UPDATE SET
                 message_id = excluded.message_id, subject = excluded.subject,
                 from_addr = excluded.from_addr, to_addrs = excluded.to_addrs,
                 date = excluded.date, in_reply_to = excluded.in_reply_to,
                 refs = excluded.refs, flags = excluded.flags, preview = excluded.preview
             RETURNING id",
            params![
                folder.0,
                message.uid,
                envelope.message_id,
                envelope.subject,
                envelope.from,
                envelope.to,
                envelope.date,
                envelope.in_reply_to,
                envelope.references.join(" "),
                message.flags.bits(),
                message.preview,
            ],
            |row| row.get(0),
        )?;
        Ok(MessageRowId(id))
    }

    pub fn set_flags(&self, folder: FolderId, uid: u32, flags: Flags) -> Result<()> {
        found(self.conn().execute(
            "UPDATE messages SET flags = ?3 WHERE folder_id = ?1 AND uid = ?2",
            params![folder.0, uid, flags.bits()],
        )?)
    }

    /// A folder's messages, newest first.
    pub fn messages(&self, folder: FolderId, page: Page) -> Result<Vec<Message>> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT {COLUMNS} FROM messages m WHERE m.folder_id = ?1
             ORDER BY m.date DESC, m.id DESC LIMIT ?2 OFFSET ?3"
        ))?;
        let messages = stmt
            .query_map(params![folder.0, page.limit, page.offset], from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(messages)
    }

    pub fn message(&self, id: MessageRowId) -> Result<Message> {
        self.conn()
            .query_row(
                &format!("SELECT {COLUMNS} FROM messages m WHERE m.id = ?1"),
                [id.0],
                from_row,
            )
            .optional()?
            .ok_or(StoreError::NotFound)
    }

    /// Every cached copy of a Message-ID in one account's folders.
    pub fn find_by_message_id(&self, account: &str, message_id: &str) -> Result<Vec<Message>> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT {COLUMNS} FROM messages m JOIN folders f ON f.id = m.folder_id
             WHERE f.account = ?1 AND m.message_id = ?2 ORDER BY m.id"
        ))?;
        let messages = stmt
            .query_map(params![account, message_id], from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(messages)
    }

    /// The UIDs cached for a folder, ascending.
    pub fn uids(&self, folder: FolderId) -> Result<Vec<u32>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT uid FROM messages WHERE folder_id = ?1 ORDER BY uid")?;
        let uids = stmt
            .query_map([folder.0], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(uids)
    }

    /// Deletes the given UIDs; returns how many were cached.
    pub fn delete_messages(&self, folder: FolderId, uids: &[u32]) -> Result<usize> {
        self.savepoint(|| {
            let mut stmt = self
                .conn()
                .prepare("DELETE FROM messages WHERE folder_id = ?1 AND uid = ?2")?;
            let mut deleted = 0;
            for uid in uids {
                deleted += stmt.execute(params![folder.0, uid])?;
            }
            Ok(deleted)
        })
    }

    pub fn set_body_state(&self, id: MessageRowId, state: BodyState) -> Result<()> {
        let value = match state {
            BodyState::Missing => 0,
            BodyState::Downloaded => 1,
        };
        found(self.conn().execute(
            "UPDATE messages SET body_state = ?2 WHERE id = ?1",
            params![id.0, value],
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_combine_and_test() {
        let flags = Flags::SEEN | Flags::FLAGGED;
        assert!(flags.contains(Flags::SEEN));
        assert!(flags.contains(Flags::SEEN | Flags::FLAGGED));
        assert!(!flags.contains(Flags::ANSWERED));
        assert_eq!(flags.without(Flags::SEEN), Flags::FLAGGED);
        assert_eq!(Flags::empty().with(Flags::DRAFT), Flags::DRAFT);
        assert!(flags.contains(Flags::empty()));
    }

    #[test]
    fn unknown_flag_bits_are_dropped() {
        assert_eq!(Flags::from_bits(0xFF).bits(), 0b1_1111);
        assert_eq!(Flags::from_bits(Flags::DELETED.bits()), Flags::DELETED);
    }

    #[test]
    fn flags_serialize_as_a_number() {
        let json = serde_json::to_string(&(Flags::SEEN | Flags::DRAFT)).unwrap();
        assert_eq!(json, "17");
        let back: Flags = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Flags::SEEN | Flags::DRAFT);
    }
}
