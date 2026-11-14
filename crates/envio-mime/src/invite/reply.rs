//! The iMIP REPLY (RFC 6047 §2.2.4): the calendar text and the mail around
//! it. Text values written here are escaped; the DTSTART and ORGANIZER lines
//! are copied as received (only re-folded).

use super::{Invite, NaiveParts, PartStat};
use crate::address::{Address, same_email};
use crate::build::{CalendarPart, Outgoing};
use crate::envelope::Envelope;
use crate::reply::{Quote, ReplyKind, reply};

const MAX_OCTETS: usize = 75;

/// The calendar text answering `invite` as `me`. `note` becomes a COMMENT
/// when not empty. `dtstamp_unix` is when the answer was made.
#[allow(clippy::too_many_arguments)] // the signature the plan fixes
pub fn reply_ics(
    invite: &Invite,
    me: &str,
    answer: PartStat,
    note: &str,
    dtstamp_unix: i64,
) -> String {
    let mut out = String::new();
    let mut line = |text: &str| fold_into(&mut out, &plain(text));
    line("BEGIN:VCALENDAR");
    line("PRODID:-//Envio//Envio 0.2//EN");
    line("VERSION:2.0");
    line("METHOD:REPLY");
    line("BEGIN:VEVENT");
    line(&format!("UID:{}", invite.uid));
    line(&format!("SEQUENCE:{}", invite.sequence));
    line(&format!("DTSTAMP:{}", stamp(dtstamp_unix)));
    line(&invite.dtstart_line);
    if let Some(organizer) = &invite.organizer_line {
        line(organizer);
    }
    line(&attendee(invite, me, answer));
    line(&format!("SUMMARY:{}", escape(&invite.summary)));
    if !note.is_empty() {
        line(&format!("COMMENT:{}", escape(note)));
    }
    line("END:VEVENT");
    line("END:VCALENDAR");
    out
}

/// The mail answering `invite` (found in `original`): To the organizer,
/// threaded like a plain reply. `text` is the body in the user's language;
/// a non-empty `note` is appended to it after a blank line.
#[allow(clippy::too_many_arguments)] // the signature the plan fixes
pub fn reply_outgoing(
    original: &Envelope,
    invite: &Invite,
    from: Address,
    answer: PartStat,
    note: &str,
    subject: String,
    text: String,
    dtstamp_unix: i64,
) -> Outgoing {
    let threading = reply(
        original,
        &Quote {
            text: "",
            attribution: "",
        },
        &[],
        ReplyKind::Sender,
    );
    let to = invite
        .organizer
        .iter()
        .map(|o| Address::new((!o.name.is_empty()).then_some(o.name.as_str()), &o.email))
        .collect();
    let ics = reply_ics(invite, &from.email, answer, note, dtstamp_unix);
    let text = if note.is_empty() {
        text
    } else {
        format!("{text}\n\n{note}")
    };
    Outgoing {
        from,
        to,
        subject,
        text,
        in_reply_to: threading.in_reply_to,
        references: threading.references,
        calendar: Some(CalendarPart {
            method: "REPLY",
            ics,
        }),
        ..Outgoing::default()
    }
}

/// The one ATTENDEE line: `me`, with the CN of the invite's attendee that
/// matches, if any.
fn attendee(invite: &Invite, me: &str, answer: PartStat) -> String {
    let stat = match answer {
        PartStat::Accepted => "ACCEPTED",
        PartStat::Tentative => "TENTATIVE",
        PartStat::Declined => "DECLINED",
        PartStat::NeedsAction => "NEEDS-ACTION",
    };
    let mut line = format!("ATTENDEE;PARTSTAT={stat}");
    let cn = invite
        .attendees
        .iter()
        .find(|a| same_email(&a.email, me))
        .map(|a| param_value(&a.name))
        .filter(|cn| !cn.is_empty());
    if let Some(cn) = cn {
        line.push_str(";CN=");
        if cn.contains([';', ':', ',']) {
            line.push('"');
            line.push_str(&cn);
            line.push('"');
        } else {
            line.push_str(&cn);
        }
    }
    line.push_str(":mailto:");
    line.push_str(&plain(me));
    line
}

/// A parameter value: no quote or control characters.
fn param_value(text: &str) -> String {
    plain(&text.replace('"', ""))
}

/// Text without control characters (a line can never carry a newline).
fn plain(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

/// TEXT escaping (RFC 5545 §3.3.11); other control characters dropped.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// `20261105T153000Z`.
fn stamp(unix: i64) -> String {
    let p = NaiveParts::from_utc_secs(unix);
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        p.year, p.month, p.day, p.hour, p.minute, p.second
    )
}

/// `text` plus CRLF, folded so no physical line passes 75 octets (the
/// continuation's leading space counts) and no character is split.
fn fold_into(out: &mut String, text: &str) {
    let mut width = 0;
    for c in text.chars() {
        if width + c.len_utf8() > MAX_OCTETS {
            out.push_str("\r\n ");
            width = 1;
        }
        out.push(c);
        width += c.len_utf8();
    }
    out.push_str("\r\n");
}
