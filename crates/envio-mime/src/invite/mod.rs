//! Meeting invites (iMIP, RFC 6047) in a message's `text/calendar` part.
//! Untrusted input: capped, lenient, never panics.

mod event;
mod lines;
mod reply;
mod when;
mod zones;

use mail_parser::MimeHeaders;

use crate::mail::{is_broken_multipart, parse_mail};

pub use event::parse_invite;
pub use reply::{reply_ics, reply_outgoing};
pub use when::{NaiveParts, When};

/// The iTIP method an invite carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteMethod {
    Request,
    Cancel,
    Reply,
}

/// An attendee's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartStat {
    Accepted,
    Tentative,
    Declined,
    NeedsAction,
}

/// An organizer or attendee.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    pub email: String,
    /// The CN parameter; empty when absent.
    pub name: String,
    pub partstat: Option<PartStat>,
}

/// The first event of one calendar part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    pub method: InviteMethod,
    pub uid: String,
    /// Missing reads as 0.
    pub sequence: u32,
    /// Unescaped, control characters stripped, at most 500 characters.
    pub summary: String,
    pub location: String,
    pub when: When,
    /// The DTSTART line as received (unfolded, with its TZID), copied into
    /// a reply.
    pub dtstart_line: String,
    /// The RECURRENCE-ID value (trimmed) of an event that is one occurrence
    /// of a repeating meeting; empty for the series.
    pub recurrence_id: String,
    /// The RECURRENCE-ID line as received, copied into a reply.
    pub recurrence_id_line: Option<String>,
    /// The ORGANIZER line as received; `Some` exactly when `organizer` is.
    pub organizer_line: Option<String>,
    /// Only a `mailto:` value counts.
    pub organizer: Option<Person>,
    /// At most 500 kept.
    pub attendees: Vec<Person>,
    pub repeats: bool,
    /// The RRULE's UNTIL as Unix seconds, when it has a readable one.
    pub repeats_until: Option<i64>,
    /// METHOD:CANCEL or STATUS:CANCELLED.
    pub cancelled: bool,
    /// The part as received, for the reply and for export.
    pub raw: String,
}

/// The first `text/calendar` part of `raw` (inline or attached) that holds
/// an invite, and its index in the attachment list when it is listed there.
pub fn invite_of(raw: &[u8]) -> Option<(Invite, Option<usize>)> {
    let message = parse_mail(raw)?;
    if is_broken_multipart(&message) {
        return None;
    }
    message.parts.iter().enumerate().find_map(|(id, part)| {
        let ct = part.content_type()?;
        if !ct.ctype().eq_ignore_ascii_case("text")
            || !ct.subtype()?.eq_ignore_ascii_case("calendar")
        {
            return None;
        }
        let invite = parse_invite(part.contents())?;
        let index = message
            .attachments
            .iter()
            .position(|&a| usize::try_from(a).is_ok_and(|a| a == id));
        Some((invite, index))
    })
}
