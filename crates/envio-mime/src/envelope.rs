//! The envelope: what the message list and the store keep for each message.

use mail_parser::{HeaderValue, MimeHeaders};

use crate::address::{Address, from_header};
use crate::attachments::{AttachmentInfo, list};
use crate::mail::{Parsed, parse_mail};
use crate::preview::preview;
use crate::thread::{normalize_id, thread_root};

/// Headers plus the preview and the attachment flag.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Envelope {
    /// Without angle brackets.
    pub message_id: Option<String>,
    /// Decoded; empty when missing.
    pub subject: String,
    pub from: Vec<Address>,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub reply_to: Vec<Address>,
    /// Unix seconds; `None` when missing or invalid.
    pub date: Option<i64>,
    /// Without angle brackets.
    pub in_reply_to: Option<String>,
    /// Without angle brackets, oldest first.
    pub references: Vec<String>,
    pub preview: String,
    /// A non-inline attachment is present. Always false for a header-only
    /// blob, which has no parts to look at: see
    /// [`header_suggests_attachments`] for a guess from its headers.
    pub has_attachments: bool,
}

impl Envelope {
    /// The id this message's thread is grouped under.
    pub fn thread_root(&self) -> String {
        thread_root(
            self.message_id.as_deref().unwrap_or_default(),
            self.in_reply_to.as_deref(),
            &self.references,
        )
    }
}

/// The envelope of a full message or a header-only blob (then the preview is
/// empty). `None` when the bytes don't look like mail.
pub fn envelope(raw: &[u8]) -> Option<Envelope> {
    parse_mail(raw).map(|message| from_parsed(&message, &list(&message)))
}

/// A guess from the headers alone, for a header-only blob that has no parts
/// to look at: `Content-Type: multipart/mixed` usually carries attachments
/// (multipart/related, alternative and signed usually do not). The full
/// message's [`Envelope::has_attachments`] is exact and replaces the guess.
pub fn header_suggests_attachments(header: &[u8]) -> bool {
    parse_mail(header)
        .and_then(|message| {
            message.content_type().map(|ct| {
                ct.c_type.eq_ignore_ascii_case("multipart")
                    && ct
                        .subtype()
                        .is_some_and(|s| s.eq_ignore_ascii_case("mixed"))
            })
        })
        .unwrap_or(false)
}

/// `attachments` is [`list`] of `message`, passed in because
/// [`crate::parse`] keeps it too.
pub(crate) fn from_parsed(message: &Parsed<'_>, attachments: &[AttachmentInfo]) -> Envelope {
    Envelope {
        message_id: message.message_id().and_then(normalize_id),
        // Bidi controls could reorder the subject on screen ("fdp.exe").
        subject: crate::link::strip_bidi_controls(message.subject().unwrap_or_default().trim()),
        from: from_header(message.from()),
        to: from_header(message.to()),
        cc: from_header(message.cc()),
        reply_to: from_header(message.reply_to()),
        date: message
            .date()
            .filter(|d| d.is_valid())
            .map(|d| d.to_timestamp()),
        in_reply_to: ids(message.in_reply_to()).into_iter().next(),
        references: ids(message.references()),
        preview: message
            .body_text(0)
            .map(|text| preview(&text))
            .unwrap_or_default(),
        has_attachments: attachments.iter().any(|a| !a.inline),
    }
}

/// Message-IDs from an In-Reply-To or References header, normalized.
fn ids(value: &HeaderValue<'_>) -> Vec<String> {
    match value {
        HeaderValue::Text(id) => normalize_id(id).into_iter().collect(),
        HeaderValue::TextList(list) => list.iter().filter_map(|id| normalize_id(id)).collect(),
        _ => Vec::new(),
    }
}
