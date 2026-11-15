//! The invite as a file a calendar app can import. Ubuntu Touch's Calendar
//! drops an event whose TZID has no matching VTIMEZONE (issue #206), and
//! many senders (Outlook, Google) leave it out, so such an invite is
//! rewritten with its times in UTC.

use std::collections::HashSet;

use super::event::MAX_LINES;
use super::lines::{split, unfold};
use super::reply::{escape, fold_into, plain, stamp};
use super::{Invite, NaiveParts, When};

/// The calendar text to hand to a calendar app: the part as received when
/// every TZID it uses is defined in it, else one VEVENT rebuilt from what
/// was read. A rebuilt event keeps UID, the times, SUMMARY, LOCATION and
/// ORGANIZER; it drops RRULE, so a repeating meeting exports only its
/// first occurrence. `dtstamp_unix` is when the file was made.
pub fn calendar_file(invite: &Invite, dtstamp_unix: i64) -> String {
    if self_contained(&invite.raw) {
        return invite.raw.clone();
    }
    let mut out = String::new();
    let mut line = |text: &str| fold_into(&mut out, &plain(text));
    line("BEGIN:VCALENDAR");
    line("PRODID:-//Envio//Envio 0.2//EN");
    line("VERSION:2.0");
    line("BEGIN:VEVENT");
    line(&format!("UID:{}", invite.uid));
    line(&format!("DTSTAMP:{}", stamp(dtstamp_unix)));
    for time in times(&invite.when) {
        line(&time);
    }
    if !invite.summary.is_empty() {
        line(&format!("SUMMARY:{}", escape(&invite.summary)));
    }
    if !invite.location.is_empty() {
        line(&format!("LOCATION:{}", escape(&invite.location)));
    }
    if let Some(organizer) = &invite.organizer_line {
        line(organizer);
    }
    line("END:VEVENT");
    line("END:VCALENDAR");
    out
}

/// True when every TZID parameter names a VTIMEZONE in the same text.
/// Unreadable text is not self-contained.
fn self_contained(raw: &str) -> bool {
    let Some(lines) = unfold(raw, MAX_LINES) else {
        return false;
    };
    let mut defined = HashSet::new();
    let mut used = HashSet::new();
    let mut in_zone = false;
    for prop in lines.iter().filter_map(|l| split(l)) {
        let value = prop.value.trim();
        match prop.name.as_str() {
            "BEGIN" if value.eq_ignore_ascii_case("VTIMEZONE") => in_zone = true,
            "END" if value.eq_ignore_ascii_case("VTIMEZONE") => in_zone = false,
            "TZID" if in_zone => {
                defined.insert(value.to_owned());
            }
            _ => {}
        }
        if let Some(tzid) = prop.param("TZID") {
            used.insert(tzid.trim().to_owned());
        }
    }
    used.is_subset(&defined)
}

/// DTSTART and, when it differs, DTEND.
fn times(when: &When) -> Vec<String> {
    let (start, end) = match *when {
        When::Timed {
            start,
            end,
            zone_unknown,
        } => {
            // An unknown zone's times are its wall time read as UTC, so
            // they go out floating rather than as a wrong instant.
            let utc = if zone_unknown { "" } else { "Z" };
            let at = |secs| format!(":{}{utc}", date_time(NaiveParts::from_utc_secs(secs)));
            (at(start), (end != start).then(|| at(end)))
        }
        When::Floating { start, end } => (
            format!(":{}", date_time(start)),
            (end != start).then(|| format!(":{}", date_time(end))),
        ),
        When::AllDay { start, end } => (
            format!(";VALUE=DATE:{}", date(start)),
            Some(format!(";VALUE=DATE:{}", date(end))),
        ),
    };
    let mut out = vec![format!("DTSTART{start}")];
    out.extend(end.map(|end| format!("DTEND{end}")));
    out
}

/// `20261120T150000`.
fn date_time(p: NaiveParts) -> String {
    format!(
        "{}T{:02}{:02}{:02}",
        date((p.year, p.month, p.day)),
        p.hour,
        p.minute,
        p.second
    )
}

/// `20261120`.
fn date((year, month, day): (i32, u8, u8)) -> String {
    format!("{year:04}{month:02}{day:02}")
}
