//! The calendar's first VEVENT: its properties into an [`Invite`].

use super::lines::{Prop, split, unescape, unfold};
use super::when::read;
use super::zones::VTimezone;
use super::{Invite, InviteMethod, PartStat, Person};
use crate::link::strip_bidi_controls;

const MAX_BYTES: usize = 256 * 1024;
const MAX_LINES: usize = 10_000;
/// Stack marker for a VEVENT after the first.
const SKIPPED: &str = "VEVENT (skipped)";
const MAX_TEXT: usize = 500;
const MAX_ATTENDEES: usize = 500;

/// Reads one calendar part. `None` for an oversized part, a METHOD other
/// than REQUEST, CANCEL or REPLY, or an event without UID or readable
/// DTSTART.
pub fn parse_invite(ics: &[u8]) -> Option<Invite> {
    if ics.len() > MAX_BYTES {
        return None;
    }
    let raw = String::from_utf8_lossy(ics);
    let mut scan = Scan::default();
    for line in unfold(&raw, MAX_LINES)? {
        if let Some(p) = split(&line) {
            scan.feed(p);
        }
    }
    let Scan {
        method,
        vtz,
        event,
        seen_event,
        ..
    } = scan;
    let method = method?;
    if !seen_event {
        return None;
    }
    let get = |name: &str| event.iter().find(|p| p.name == name);
    let uid = get("UID")?.value.trim().to_owned();
    if uid.is_empty() {
        return None;
    }
    let when = read(get("DTSTART")?, get("DTEND"), get("DURATION"), &vtz)?;
    let text = |name: &str| get(name).map_or_else(String::new, |p| unescape(&p.value, MAX_TEXT));
    let status_cancelled =
        get("STATUS").is_some_and(|p| p.value.trim().eq_ignore_ascii_case("CANCELLED"));
    Some(Invite {
        method,
        uid,
        sequence: get("SEQUENCE")
            .and_then(|p| p.value.trim().parse().ok())
            .unwrap_or(0),
        summary: text("SUMMARY"),
        location: text("LOCATION"),
        when,
        organizer: get("ORGANIZER").and_then(person),
        attendees: event
            .iter()
            .filter(|p| p.name == "ATTENDEE")
            .filter_map(person)
            .take(MAX_ATTENDEES)
            .collect(),
        repeats: get("RRULE").is_some(),
        cancelled: method == InviteMethod::Cancel || status_cancelled,
        raw: raw.into_owned(),
    })
}

/// The calendar's first event and what the reader needs from outside it,
/// gathered line by line.
#[derive(Default)]
struct Scan {
    method: Option<InviteMethod>,
    vtz: Vec<VTimezone>,
    event: Vec<Prop>,
    seen_event: bool,
    /// Open components below VCALENDAR; `["VEVENT"]` while directly in the
    /// first event, anything longer or other while in a nested or skipped
    /// component.
    stack: Vec<String>,
    tz_id: Option<String>,
    tz_offset: Option<i32>,
    in_standard: bool,
}

impl Scan {
    fn feed(&mut self, p: Prop) {
        match p.name.as_str() {
            "BEGIN" => self.begin(p.value.trim().to_ascii_uppercase()),
            "END" => self.end(&p.value.trim().to_ascii_uppercase()),
            _ if self.stack.is_empty() => {
                if p.name == "METHOD" && self.method.is_none() {
                    self.method = method_of(&p.value);
                }
            }
            _ if self.stack == ["VEVENT"] => self.event.push(p),
            "TZID" if self.stack == ["VTIMEZONE"] => self.tz_id = Some(p.value.trim().to_owned()),
            "TZOFFSETTO" if self.in_standard && self.tz_offset.is_none() => {
                self.tz_offset = offset_seconds(&p.value);
            }
            _ => {}
        }
    }

    fn begin(&mut self, kind: String) {
        if kind == "VCALENDAR" {
            return;
        }
        if kind == "VEVENT" && self.stack.is_empty() {
            if self.seen_event {
                // Later events do not count; a marker keeps their
                // properties out of the first one.
                self.stack.push(SKIPPED.to_owned());
                return;
            }
            self.seen_event = true;
        } else if kind == "STANDARD" && self.stack == ["VTIMEZONE"] {
            self.in_standard = true;
        }
        self.stack.push(kind);
    }

    fn end(&mut self, kind: &str) {
        if kind == "VCALENDAR" {
            return;
        }
        if kind == "STANDARD" {
            self.in_standard = false;
        }
        if kind == "VTIMEZONE"
            && let (Some(tzid), Some(off)) = (self.tz_id.take(), self.tz_offset.take())
        {
            self.vtz.push(VTimezone {
                tzid,
                standard_offset_s: off,
            });
        }
        let skipped_event = kind == "VEVENT" && self.stack.last().is_some_and(|k| k == SKIPPED);
        if skipped_event || self.stack.last().is_some_and(|k| k == kind) {
            self.stack.pop();
        }
    }
}

fn method_of(value: &str) -> Option<InviteMethod> {
    match value.trim().to_ascii_uppercase().as_str() {
        "REQUEST" => Some(InviteMethod::Request),
        "CANCEL" => Some(InviteMethod::Cancel),
        "REPLY" => Some(InviteMethod::Reply),
        _ => None,
    }
}

/// `+0100`, `-0530` or `+010000` as seconds.
fn offset_seconds(value: &str) -> Option<i32> {
    let v = value.trim();
    let (sign, digits) = match v.as_bytes().first()? {
        b'-' => (-1, &v[1..]),
        b'+' => (1, &v[1..]),
        _ => (1, v),
    };
    if !(digits.len() == 4 || digits.len() == 6) || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = |r: std::ops::Range<usize>| digits[r].parse::<i32>().ok();
    let secs = if digits.len() == 6 { n(4..6)? } else { 0 };
    Some(sign * (n(0..2)? * 3600 + n(2..4)? * 60 + secs))
}

/// ORGANIZER or ATTENDEE; only a `mailto:` value counts.
fn person(p: &Prop) -> Option<Person> {
    let v = p.value.trim();
    let address = v
        .get(..7)
        .filter(|s| s.eq_ignore_ascii_case("mailto:"))
        .map(|_| &v[7..])?;
    let email = address.split('?').next().unwrap_or("").trim();
    if email.is_empty() {
        return None;
    }
    let partstat = p
        .param("PARTSTAT")
        .and_then(|s| match s.to_ascii_uppercase().as_str() {
            "ACCEPTED" => Some(PartStat::Accepted),
            "TENTATIVE" => Some(PartStat::Tentative),
            "DECLINED" => Some(PartStat::Declined),
            "NEEDS-ACTION" => Some(PartStat::NeedsAction),
            _ => None,
        });
    Some(Person {
        email: strip_bidi_controls(&email.chars().take(MAX_TEXT).collect::<String>()),
        name: p
            .param("CN")
            .map_or_else(String::new, |cn| unescape(cn, MAX_TEXT)),
        partstat,
    })
}
