//! Resolving a TZID to Unix seconds: an IANA name (jiff, system zoneinfo or
//! its bundled copy), a Windows zone name, then the part's own VTIMEZONE.

use super::when::NaiveParts;
use jiff::civil::DateTime;
use jiff::tz::TimeZone;

mod windows;

/// What a part's `BEGIN:VTIMEZONE` block says about its zone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VTimezone {
    pub tzid: String,
    /// `TZOFFSETTO` of the first `STANDARD` block, in seconds.
    pub standard_offset_s: i32,
    /// The zone also has a `DAYLIGHT` block: one fixed offset would be wrong
    /// for part of the year.
    pub has_daylight: bool,
}

/// Unix seconds of `local` in zone `tzid`; `None` when the zone is unknown.
pub(super) fn resolve(tzid: &str, local: NaiveParts, vtz: &[VTimezone]) -> Option<i64> {
    let name = clean(tzid);
    if let Some(secs) = iana(name, local) {
        return Some(secs);
    }
    if let Some(secs) = windows_iana(name).and_then(|n| iana(n, local)) {
        return Some(secs);
    }
    let zone = vtz.iter().find(|z| z.tzid == tzid)?;
    // Only a zone without daylight time is one fixed offset.
    if zone.has_daylight {
        return None;
    }
    // A VTIMEZONE offset is a real UTC offset: anything over a day is junk.
    if zone.standard_offset_s.unsigned_abs() >= 86_400 {
        return None;
    }
    Some(local.as_utc_secs() - i64::from(zone.standard_offset_s))
}

/// `(unix seconds, resolved)`: a missing or unknown zone reads the wall time
/// as UTC and reports `false`.
pub(super) fn resolve_or_utc(
    tzid: Option<&str>,
    local: NaiveParts,
    vtz: &[VTimezone],
) -> (i64, bool) {
    match tzid.and_then(|t| resolve(t, local, vtz)) {
        Some(s) => (s, true),
        None => (local.as_utc_secs(), false),
    }
}

/// Drops a leading `/` and Thunderbird's `/mozilla.org/<version>/` prefix.
fn clean(tzid: &str) -> &str {
    let name = tzid.trim().trim_start_matches('/');
    match name.strip_prefix("mozilla.org/") {
        Some(rest) => rest.split_once('/').map_or(rest, |(_, zone)| zone),
        None => name,
    }
}

/// An IANA name looks like `Area/Location` or `Etc/GMT+5`; anything else is
/// kept away from the zoneinfo directory.
fn iana_shaped(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('/')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'_' | b'+' | b'-'))
}

/// Compatible disambiguation: a gap reads after the jump, a fold takes the
/// earlier of the two instants.
fn iana(name: &str, local: NaiveParts) -> Option<i64> {
    if !iana_shaped(name) {
        return None;
    }
    let zone = TimeZone::get(name).ok()?;
    let dt = DateTime::new(
        i16::try_from(local.year).ok()?,
        i8::try_from(local.month).ok()?,
        i8::try_from(local.day).ok()?,
        i8::try_from(local.hour).ok()?,
        i8::try_from(local.minute).ok()?,
        i8::try_from(local.second).ok()?,
        0,
    )
    .ok()?;
    let ts = zone.to_ambiguous_timestamp(dt).compatible().ok()?;
    Some(ts.as_second())
}

fn windows_iana(name: &str) -> Option<&'static str> {
    let table = windows::WINDOWS_ZONES;
    table
        .binary_search_by(|(w, _)| (*w).cmp(name))
        .ok()
        .map(|i| table[i].1)
}
