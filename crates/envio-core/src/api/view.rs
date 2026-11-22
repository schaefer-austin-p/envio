//! Everything an event carries.

use crate::health::AccountStatus;
use envio_store::{Cursor, Flags, FolderId, FolderRole, MessageRowId};

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
    /// The sender name on mail.
    pub name: String,
    /// What the user called the account; empty when unset.
    pub label: String,
    pub email: String,
    pub status: AccountStatus,
    /// Unread messages in the Inbox.
    pub unread: u32,
    /// The stored entry cannot be read; `status` names why.
    pub broken: bool,
    /// Signs in with a provider (OAuth), not a password.
    pub oauth: bool,
    /// An Outlook.com account on IMAP that can move to Graph: its row offers
    /// "Update sign-in" (#342).
    pub update_sign_in: bool,
}

/// Where the move to Graph is, for the page that shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationStage {
    /// Running, or waiting for the Outbox.
    Moving,
    /// Three failed runs: Keep waiting or Switch anyway.
    Stuck,
    /// IMAP refuses the sign-in: Sign in again, Keep waiting or Switch anyway.
    SignIn,
    Done,
}

impl MigrationStage {
    /// The word QML switches on.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Moving => "moving",
            Self::Stuck => "stuck",
            Self::SignIn => "signin",
            Self::Done => "done",
        }
    }
}

/// What the move's page and the account's row say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationState {
    pub stage: MigrationStage,
    pub title: String,
    pub text: String,
    /// The line under `text`: how to leave, or what "Switch anyway" does.
    pub hint: String,
    /// Changes dropped by "Switch anyway" or made after step 1: the notice
    /// above the result, empty for none.
    pub note_title: String,
    pub note_text: String,
    pub can_switch: bool,
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
    /// Messages not yet read.
    pub unread: u32,
}

/// A message as a list shows it. Subjects and addresses are for the UI
/// only; they are never logged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageRow {
    pub id: MessageRowId,
    pub folder: FolderId,
    /// The account key the message's folder belongs to.
    pub account: String,
    /// Where the next page of its list starts.
    pub cursor: Cursor,
    /// Whether the message has attachments. The cache keeps no
    /// BODYSTRUCTURE yet, so this is `false` for now.
    pub has_attachments: bool,
    /// The sender's name (the address when the name is missing or looks like
    /// one), else the raw From line, else a placeholder.
    pub from: String,
    /// The sender's address, or the key the stamp is tinted by when the From
    /// line does not parse; empty when there is no sender. Only the stamp
    /// reads it.
    pub from_address: String,
    pub subject: String,
    pub preview: String,
    /// Unix seconds; `None` when the message has no usable date.
    pub date: Option<i64>,
    pub flags: Flags,
    /// Set on a pseudo-row: the local draft's id (it has no server copy yet).
    pub local_draft: Option<String>,
    /// The words of a pseudo-row's "not uploaded" chip: "Not uploaded", or
    /// the refusal in at most four words. Empty on a server row.
    pub not_uploaded_text: String,
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
    pub header: MessageHeader,
    /// Sanitised HTML (escaped text when `raw`).
    pub html: String,
    /// The message could not be read as mail and is shown as plain text.
    pub raw: bool,
    /// Remote images left out.
    pub blocked_images: usize,
    /// The remote images the HTML loads, each once; the reader preloads
    /// them and lays the body out again once they are in (#163).
    pub remote_images: Vec<String>,
    /// The mail sets its own colours, so the reader draws it on a light
    /// card. Always false for a raw message.
    pub own_colours: bool,
    pub attachments: Vec<AttachmentRow>,
    /// The meeting card, when the message carries an invite.
    pub invite: Option<InviteCard>,
    /// The message's folder has the Drafts role.
    pub is_draft: bool,
}

/// What kind of invite mail this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardMethod {
    Request,
    Cancel,
    Reply,
}

impl CardMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Cancel => "cancel",
            Self::Reply => "reply",
        }
    }
}

/// Which of the card's states applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardState {
    /// A request nobody has answered: the buttons.
    Open,
    /// The user's answer is remembered and is not older than the request.
    Answered,
    /// The request changed after the answer.
    Updated,
    Cancelled,
    /// Somebody's answer to our invite.
    Replied,
}

impl CardState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Answered => "answered",
            Self::Updated => "updated",
            Self::Cancelled => "cancelled",
            Self::Replied => "replied",
        }
    }
}

/// The state of our own reply to an invite, as the Outbox shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyState {
    /// Not in the Outbox: sent, or never queued.
    Gone,
    /// In the Outbox and not failed.
    Sending,
    /// In the Outbox as failed.
    Failed,
}

/// The meeting card of an invite message (the contract with the app: times
/// are Unix seconds, or dates for all-day and floating events). Texts are
/// sanitised plain text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InviteCard {
    pub method: CardMethod,
    pub state: CardState,
    /// Translated; empty for `Open`.
    pub state_text: String,
    pub answer: Option<super::command::Answer>,
    /// Our reply is still in the Outbox and has not failed.
    pub sending: bool,
    /// Our reply is in the Outbox as failed.
    pub send_failed: bool,
    pub summary: String,
    pub location: String,
    pub all_day: bool,
    /// A wall time with no zone: shown as the phone's local time.
    pub floating: bool,
    /// Unix seconds; 0 when all-day or floating.
    pub start: i64,
    pub end: i64,
    /// All-day: `YYYY-MM-DD` (end exclusive); floating:
    /// `YYYY-MM-DDTHH:MM:SS`; else empty.
    pub start_date: String,
    pub end_date: String,
    /// The TZID resolved nowhere.
    pub zone_unknown: bool,
    pub repeats: bool,
    /// Display name, else address.
    pub organizer: String,
    /// The address an answer goes to (the CN in `organizer` can say anything).
    pub organizer_email: String,
    /// A request with a `mailto:` organizer.
    pub can_answer: bool,
    /// A live request: a cancellation or a reply is not an event to add.
    pub can_add_to_calendar: bool,
    /// Attachment index of the calendar part; -1 when it is inline only.
    pub ics_index: i64,
}

/// The header lines the reader shows. Never logged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageHeader {
    /// The sender's name, or the address when there is no name.
    pub from: String,
    pub from_address: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    /// Unix seconds.
    pub date: Option<i64>,
    /// The account key.
    pub account: String,
    /// The sender is in the contacts or is allowed to load images.
    pub sender_known: bool,
    /// The account's server vouched for the From address (see
    /// `envio_mime::sender_verified`); "always load from this sender" only
    /// works for such mail.
    pub sender_verified: bool,
    /// The message carries a DKIM signature that could verify the From
    /// address, so "always load from this sender" is worth offering even
    /// before it is verified (verified mail is always checkable).
    pub sender_checkable: bool,
}

/// One address offered while typing a recipient.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub name: String,
    pub address: String,
    pub from_contacts: bool,
    pub last_seen: Option<i64>,
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
        /// Move to Sent is offered; not on Graph, which keeps the sent
        /// copy itself.
        can_file: bool,
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
