//! What a model row holds: flat fields, one per QML role.

use crate::dates::DateBucket;
use crate::models::Model;

#[derive(Debug, Clone, PartialEq)]
pub struct AccountRow {
    pub key: String,
    pub name: String,
    pub email: String,
    /// What the drawer shows: name and address, or the address once when the
    /// name is empty or the same.
    pub heading: String,
    /// The status in words.
    pub status: String,
    /// "ok", "offline", "local", "auth", "pin", "broken" or "diskfull".
    pub state: String,
    pub unread: u32,
    /// The count as the number's digits, empty for none: QML shows it
    /// without formatting anything.
    pub unread_text: String,
    /// The certificate to look at, while the status is a certificate
    /// problem with details (boxed to keep rows small).
    pub pin: Box<PinRow>,
}

/// What the trust dialog shows; all empty when there is nothing to trust.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PinRow {
    /// "imap" or "smtp".
    pub service: String,
    /// The server the certificate came from.
    pub host: String,
    /// The dialog title, chosen by the core: names the server (IMAP or SMTP).
    pub title: String,
    /// The presented fingerprint as the dialog shows it.
    pub fingerprint: String,
    /// The presented fingerprint in the form `AcceptCertificate` takes.
    pub hex: String,
    /// The core's words about the certificate.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FolderRowUi {
    pub id: i64,
    pub account: String,
    pub name: String,
    /// "inbox", "sent", "drafts", "trash", "archive", "junk" or "".
    pub role: String,
    pub unread: u32,
    pub depth: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MessageRowUi {
    pub id: i64,
    pub account: String,
    pub account_name: String,
    pub from: String,
    pub from_address: String,
    pub subject: String,
    pub preview: String,
    /// Unix seconds, 0 when the message has none.
    pub date: i64,
    pub date_bucket: DateBucket,
    pub unread: bool,
    pub flagged: bool,
    pub has_attachments: bool,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutboxRowUi {
    pub id: String,
    pub account: String,
    pub subject: String,
    /// The recipients, joined.
    pub to: String,
    /// "waiting", "sending", "sentnotfiled", "failed" or "maybesent".
    pub status: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SuggestionRow {
    pub name: String,
    pub address: String,
    pub from_contacts: bool,
    /// Unix seconds, 0 when never seen.
    pub last_seen: i64,
    /// What was asked for; an answer to an earlier query is not this one's.
    pub query: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    Account(AccountRow),
    Folder(FolderRowUi),
    Message(MessageRowUi),
    Outbox(OutboxRowUi),
    Suggestion(SuggestionRow),
}

/// A role's value, for the glue to turn into a QVariant.
#[derive(Debug, Clone, PartialEq)]
pub enum RowValue {
    Str(String),
    Int(i64),
    Bool(bool),
}

const ACCOUNT: &[&str] = &[
    "key",
    "name",
    "email",
    "heading",
    "status",
    "state",
    "unread",
    "unreadText",
    "pinService",
    "pinHost",
    "pinTitle",
    "pinFingerprint",
    "pinHex",
    "pinText",
];
const FOLDER: &[&str] = &["id", "account", "name", "role", "unread", "depth"];
const MESSAGE: &[&str] = &[
    "id",
    "account",
    "accountName",
    "from",
    "fromAddress",
    "subject",
    "preview",
    "date",
    "dateBucket",
    "unread",
    "flagged",
    "hasAttachments",
    "selected",
];
const OUTBOX: &[&str] = &["id", "account", "subject", "to", "status", "text"];
const SUGGESTION: &[&str] = &["name", "address", "fromContacts", "lastSeen", "query"];

fn s(v: &str) -> RowValue {
    RowValue::Str(v.to_owned())
}

impl Row {
    /// The QML role names of a model, in role order.
    pub fn role_names(model: Model) -> &'static [&'static str] {
        match model {
            Model::Accounts => ACCOUNT,
            Model::Folders => FOLDER,
            Model::Messages | Model::Search => MESSAGE,
            Model::Outbox => OUTBOX,
            Model::Suggestions => SUGGESTION,
        }
    }

    /// The value of a role, or `None` when the row has no such role.
    pub fn value(&self, role: &str) -> Option<RowValue> {
        use RowValue::{Bool, Int};
        Some(match (self, role) {
            (Self::Account(r), "key") => s(&r.key),
            (Self::Account(r), "name") => s(&r.name),
            (Self::Account(r), "email") => s(&r.email),
            (Self::Account(r), "heading") => s(&r.heading),
            (Self::Account(r), "status") => s(&r.status),
            (Self::Account(r), "state") => s(&r.state),
            (Self::Account(r), "unread") => Int(r.unread.into()),
            (Self::Account(r), "unreadText") => s(&r.unread_text),
            (Self::Account(r), "pinService") => s(&r.pin.service),
            (Self::Account(r), "pinHost") => s(&r.pin.host),
            (Self::Account(r), "pinTitle") => s(&r.pin.title),
            (Self::Account(r), "pinFingerprint") => s(&r.pin.fingerprint),
            (Self::Account(r), "pinHex") => s(&r.pin.hex),
            (Self::Account(r), "pinText") => s(&r.pin.text),
            (Self::Folder(r), "id") => Int(r.id),
            (Self::Folder(r), "account") => s(&r.account),
            (Self::Folder(r), "name") => s(&r.name),
            (Self::Folder(r), "role") => s(&r.role),
            (Self::Folder(r), "unread") => Int(r.unread.into()),
            (Self::Folder(r), "depth") => Int(r.depth.into()),
            (Self::Message(r), "id") => Int(r.id),
            (Self::Message(r), "account") => s(&r.account),
            (Self::Message(r), "accountName") => s(&r.account_name),
            (Self::Message(r), "from") => s(&r.from),
            (Self::Message(r), "fromAddress") => s(&r.from_address),
            (Self::Message(r), "subject") => s(&r.subject),
            (Self::Message(r), "preview") => s(&r.preview),
            (Self::Message(r), "date") => Int(r.date),
            (Self::Message(r), "dateBucket") => s(r.date_bucket.as_str()),
            (Self::Message(r), "unread") => Bool(r.unread),
            (Self::Message(r), "flagged") => Bool(r.flagged),
            (Self::Message(r), "hasAttachments") => Bool(r.has_attachments),
            (Self::Message(r), "selected") => Bool(r.selected),
            (Self::Outbox(r), "id") => s(&r.id),
            (Self::Outbox(r), "account") => s(&r.account),
            (Self::Outbox(r), "subject") => s(&r.subject),
            (Self::Outbox(r), "to") => s(&r.to),
            (Self::Outbox(r), "status") => s(&r.status),
            (Self::Outbox(r), "text") => s(&r.text),
            (Self::Suggestion(r), "name") => s(&r.name),
            (Self::Suggestion(r), "address") => s(&r.address),
            (Self::Suggestion(r), "fromContacts") => Bool(r.from_contacts),
            (Self::Suggestion(r), "lastSeen") => Int(r.last_seen),
            (Self::Suggestion(r), "query") => s(&r.query),
            _ => return None,
        })
    }
}
