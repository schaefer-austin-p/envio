//! Message metadata: envelope, flags, preview, thread references and
//! whether the body is on disk.

use std::collections::BTreeMap;
use std::ops::BitOr;

use rusqlite::types::Value;
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
#[serde(from = "u8", into = "u8")]
pub struct Flags(u8);

impl From<u8> for Flags {
    fn from(bits: u8) -> Self {
        Self::from_bits(bits)
    }
}

impl From<Flags> for u8 {
    fn from(flags: Flags) -> Self {
        flags.0
    }
}

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
    /// A non-inline attachment is present. A guess from the headers until
    /// the body is downloaded, then exact.
    pub has_attachments: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMessage {
    pub uid: u32,
    /// RFC822.SIZE in bytes.
    pub size: u32,
    pub envelope: Envelope,
    pub flags: Flags,
    pub preview: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyState {
    Missing,
    Downloaded,
    /// A big message stored with its text and small inline images only: its
    /// other parts are stubs, to be fetched when asked for. It shows in the
    /// reader, but is not the whole message.
    Partial,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: MessageRowId,
    pub folder: FolderId,
    pub uid: u32,
    /// RFC822.SIZE in bytes; 0 if cached before migration 0002.
    pub size: u32,
    pub envelope: Envelope,
    pub flags: Flags,
    pub preview: String,
    pub body: BodyState,
}

/// A position in a newest-first list: the next page holds rows strictly
/// older than this `(date, id)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub date: i64,
    pub id: MessageRowId,
}

/// A list of messages: one folder, or every account's Inbox merged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ListRef {
    Folder(FolderId),
    Inboxes,
}

/// The usual page size of a list.
pub const PAGE: u32 = 50;

/// The columns `from_row` reads, from `messages` aliased as `m`.
pub(crate) const COLUMNS: &str = "m.id, m.folder_id, m.uid, m.message_id, m.subject, \
     m.from_addr, m.to_addrs, m.date, m.in_reply_to, m.refs, m.flags, m.preview, m.body_state, m.size, m.has_attachments";

pub(crate) fn from_row(row: &Row<'_>) -> rusqlite::Result<Message> {
    let refs: String = row.get(9)?;
    let body: i64 = row.get(12)?;
    Ok(Message {
        id: MessageRowId(row.get(0)?),
        folder: FolderId(row.get(1)?),
        uid: row.get(2)?,
        size: row.get(13)?,
        envelope: Envelope {
            message_id: row.get(3)?,
            subject: row.get(4)?,
            from: row.get(5)?,
            to: row.get(6)?,
            date: row.get(7)?,
            in_reply_to: row.get(8)?,
            references: refs.split_whitespace().map(str::to_owned).collect(),
            has_attachments: row.get(14)?,
        },
        flags: Flags::from_bits(row.get(10)?),
        preview: row.get(11)?,
        body: match body {
            0 => BodyState::Missing,
            2 => BodyState::Partial,
            _ => BodyState::Downloaded,
        },
    })
}

impl Db {
    /// Adds a message, or updates it by UID. An update keeps the body state
    /// and `has_attachments`: a header never changes, so a new row takes the
    /// header's guess and only [`Db::set_has_attachments`] (the full parse)
    /// changes it afterwards.
    pub fn upsert_message(&self, folder: FolderId, message: &NewMessage) -> Result<MessageRowId> {
        let envelope = &message.envelope;
        let id = self.conn().query_row(
            "INSERT INTO messages (folder_id, uid, message_id, subject, from_addr, to_addrs,
                                   date, in_reply_to, refs, flags, preview, size, has_attachments)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT (folder_id, uid) DO UPDATE SET
                 message_id = excluded.message_id, subject = excluded.subject,
                 from_addr = excluded.from_addr, to_addrs = excluded.to_addrs,
                 date = excluded.date, in_reply_to = excluded.in_reply_to,
                 refs = excluded.refs, flags = excluded.flags, preview = excluded.preview,
                 size = excluded.size
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
                message.size,
                envelope.has_attachments,
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

    /// Up to `limit` messages of `list`, newest first (`date DESC, id DESC`).
    /// With `after`, only rows strictly older than the cursor, so new mail
    /// and deletions between calls never repeat or skip a row.
    pub fn list(&self, list: ListRef, after: Option<Cursor>, limit: u32) -> Result<Vec<Message>> {
        let (folder_filter, key): (_, Value) = match list {
            ListRef::Folder(folder) => ("m.folder_id = ?1", folder.0.into()),
            ListRef::Inboxes => (
                "m.folder_id IN (SELECT id FROM folders WHERE role = ?1)",
                "inbox".to_owned().into(),
            ),
        };
        let (date, id) = after.map_or((i64::MAX, i64::MAX), |c| (c.date, c.id.0));
        let mut stmt = self.conn().prepare(&format!(
            "SELECT {COLUMNS} FROM messages m
             WHERE {folder_filter} AND (m.date < ?2 OR (m.date = ?2 AND m.id < ?3))
             ORDER BY m.date DESC, m.id DESC LIMIT ?4"
        ))?;
        let messages = stmt
            .query_map(params![key, date, id, limit], from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(messages)
    }

    /// The cursor that continues a list after `message`.
    pub fn cursor_of(message: &Message) -> Cursor {
        Cursor {
            date: message.envelope.date,
            id: message.id,
        }
    }

    /// Unseen messages per folder of `account` (folders with none are absent).
    pub fn unread_counts(&self, account: &str) -> Result<BTreeMap<FolderId, u32>> {
        let mut stmt = self.conn().prepare(
            "SELECT m.folder_id, count(*) FROM messages m
             JOIN folders f ON f.id = m.folder_id
             WHERE f.account = ?1 AND m.flags & ?2 = 0
             GROUP BY m.folder_id",
        )?;
        let counts = stmt
            .query_map(params![account, Flags::SEEN.bits()], |row| {
                Ok((FolderId(row.get(0)?), row.get(1)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(counts)
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

    /// The row ids of the cached messages with these UIDs (UIDs not cached
    /// are skipped), in the order given.
    pub fn ids_of(&self, folder: FolderId, uids: &[u32]) -> Result<Vec<MessageRowId>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT id FROM messages WHERE folder_id = ?1 AND uid = ?2")?;
        let mut ids = Vec::new();
        for uid in uids {
            if let Some(id) = stmt
                .query_row(params![folder.0, uid], |row| row.get(0))
                .optional()?
            {
                ids.push(MessageRowId(id));
            }
        }
        Ok(ids)
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

    /// The cached messages of a folder with `from <= uid < below`, by UID.
    pub fn messages_in_uid_range(
        &self,
        folder: FolderId,
        from: u32,
        below: u32,
    ) -> Result<Vec<Message>> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT {COLUMNS} FROM messages m
             WHERE m.folder_id = ?1 AND m.uid >= ?2 AND m.uid < ?3 ORDER BY m.uid"
        ))?;
        let messages = stmt
            .query_map(params![folder.0, from, below], from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(messages)
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

    /// Stores the exact attachment flag, from the whole message.
    pub fn set_has_attachments(&self, id: MessageRowId, has: bool) -> Result<()> {
        found(self.conn().execute(
            "UPDATE messages SET has_attachments = ?2 WHERE id = ?1",
            params![id.0, has],
        )?)
    }

    pub fn set_body_state(&self, id: MessageRowId, state: BodyState) -> Result<()> {
        let value = match state {
            BodyState::Missing => 0,
            BodyState::Downloaded => 1,
            BodyState::Partial => 2,
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
    fn deserializing_flags_drops_unknown_bits() {
        assert_eq!(
            serde_json::from_str::<Flags>("255").unwrap().bits(),
            0b1_1111
        );
        assert_eq!(serde_json::to_string(&Flags::SEEN).unwrap(), "1");
    }

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
