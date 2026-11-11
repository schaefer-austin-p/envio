//! Everything an event carries.

use crate::health::AccountStatus;
use envio_store::{Flags, FolderId, FolderRole, MessageRowId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheState {
    /// There was no cache; sync fills it.
    Fresh,
    Existing,
    /// The old cache was damaged and is being rebuilt from the server.
    Rebuilt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountInfo {
    pub key: String,
    pub name: String,
    pub email: String,
    pub status: AccountStatus,
    /// Unread messages in the Inbox (filled from Task 6 on; 0 until then).
    pub unread: u32,
    /// The stored entry cannot be read; `status` names why.
    pub broken: bool,
}

/// Work an account has that no server holds yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnsentCounts {
    pub outbox: usize,
    pub drafts: usize,
    pub actions: usize,
}

/// A folder as the folder list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRow {
    pub id: FolderId,
    pub path: String,
    /// The path's last segment, split on `/` and `.`: the store keeps no
    /// decoded name.
    pub name: String,
    pub role: FolderRole,
    /// Always 0 in this plan: the store has no count query, and Plan 7 adds
    /// it alongside its other UI queries (counting by paging through
    /// `Db::messages` is not acceptable).
    pub unread: u32,
}

/// A message as a list shows it. Subjects and addresses are for the UI
/// only; they are never logged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageRow {
    pub id: MessageRowId,
    pub folder: FolderId,
    pub from: String,
    pub subject: String,
    pub preview: String,
    /// Unix seconds; `None` when the message has no usable date.
    pub date: Option<i64>,
    pub flags: Flags,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentRow {
    pub index: usize,
    /// Safe to use as a file name.
    pub name: String,
    pub content_type: String,
    pub size: usize,
}

/// A message ready to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageView {
    /// Sanitised HTML (escaped text when `raw`).
    pub html: String,
    /// The message could not be read as mail and is shown as plain text.
    pub raw: bool,
    /// Remote images left out.
    pub blocked_images: usize,
    pub attachments: Vec<AttachmentRow>,
}

/// Where a queued message stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutboxStatus {
    Queued,
    Sending,
    /// `asked`: Sent was searched this session and came up empty, so the
    /// user may decide.
    MayHaveBeenSent {
        asked: bool,
    },
    /// The server took it; it is not in Sent yet.
    SentNotFiled,
    Failed {
        reason: String,
    },
    /// The files can not be read; it can only be deleted.
    Damaged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxRow {
    pub id: String,
    pub account: String,
    pub subject: String,
    pub recipients: Vec<String>,
    pub queued_at_ms: i64,
    pub status: OutboxStatus,
    /// The status in words.
    pub text: String,
}
