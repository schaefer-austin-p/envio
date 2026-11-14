//! Dates and times of an event: the value forms of DTSTART, DTEND and
//! DURATION, and the civil-date arithmetic they need.

use super::lines::Prop;
use super::zones::{self, VTimezone};

const MAX_DURATION_S: i64 = 10_000_000_000;

/// A local wall-clock time with no zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NaiveParts {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

/// When an event happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum When {
    /// Unix seconds. `end == start` when the event has neither DTEND nor
    /// DURATION. `zone_unknown`: a TZID that could not be resolved; the
    /// times then read the wall time as UTC.
    Timed {
        start: i64,
        end: i64,
        zone_unknown: bool,
    },
    /// `(year, month, day)`; `end` is exclusive and defaults to the day
    /// after `start`.
    AllDay {
        start: (i32, u8, u8),
        end: (i32, u8, u8),
    },
    /// Local wall time, no zone.
    Floating { start: NaiveParts, end: NaiveParts },
}

impl NaiveParts {
    /// Seconds since 1970-01-01 reading the fields as UTC.
    pub(crate) fn as_utc_secs(self) -> i64 {
        days_from_civil(self.year, self.month, self.day) * 86_400
            + i64::from(self.hour) * 3600
            + i64::from(self.minute) * 60
            + i64::from(self.second)
    }

    pub(crate) fn from_utc_secs(secs: i64) -> Self {
        let rem = secs.rem_euclid(86_400);
        let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
        // rem < 86_400, so each of these fits.
        Self {
            year,
            month,
            day,
            hour: u8::try_from(rem / 3600).unwrap_or(0),
            minute: u8::try_from(rem % 3600 / 60).unwrap_or(0),
            second: u8::try_from(rem % 60).unwrap_or(0),
        }
    }
}

/// Days since 1970-01-01 (proleptic Gregorian).
fn days_from_civil(y: i32, m: u8, d: u8) -> i64 {
    let y = i64::from(y) - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (i64::from(m) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i32, u8, u8) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (
        i32::try_from(y).unwrap_or(0),
        u8::try_from(m).unwrap_or(1),
        u8::try_from(d).unwrap_or(1),
    )
}

fn days_in_month(y: i32, m: u8) -> u8 {
    match m {
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn digits(b: &[u8]) -> Option<u32> {
    if b.is_empty() || !b.iter().all(u8::is_ascii_digit) {
        return None;
    }
    b.iter().try_fold(0u32, |n, d| {
        n.checked_mul(10)?.checked_add(u32::from(d - b'0'))
    })
}

/// `YYYYMMDD`, validated, years 1 to 9999.
fn date_of(b: &[u8]) -> Option<(i32, u8, u8)> {
    if b.len() != 8 {
        return None;
    }
    let y = i32::try_from(digits(&b[..4])?).ok()?;
    let m = u8::try_from(digits(&b[4..6])?).ok()?;
    let d = u8::try_from(digits(&b[6..])?).ok()?;
    ((1..=12).contains(&m) && y >= 1 && (1..=days_in_month(y, m)).contains(&d)).then_some((y, m, d))
}

enum Stamp {
    Date((i32, u8, u8)),
    Utc(NaiveParts),
    /// With the TZID parameter when there was one.
    Local(NaiveParts, Option<String>),
}

fn stamp(p: &Prop) -> Option<Stamp> {
    let v = p.value.trim().as_bytes();
    if v.len() == 8 {
        return Some(Stamp::Date(date_of(v)?));
    }
    let utc = v.last().is_some_and(|c| c.eq_ignore_ascii_case(&b'Z'));
    let v = if utc { &v[..v.len() - 1] } else { v };
    if v.len() != 15 || !v[8].eq_ignore_ascii_case(&b'T') {
        return None;
    }
    let (year, month, day) = date_of(&v[..8])?;
    let hour = u8::try_from(digits(&v[9..11])?).ok()?;
    let minute = u8::try_from(digits(&v[11..13])?).ok()?;
    // A leap second reads as :59.
    let second = u8::try_from(digits(&v[13..15])?).ok()?.min(59);
    if hour > 23 || minute > 59 {
        return None;
    }
    let parts = NaiveParts {
        year,
        month,
        day,
        hour,
        minute,
        second,
    };
    Some(if utc {
        Stamp::Utc(parts)
    } else {
        Stamp::Local(parts, p.param("TZID").map(str::to_owned))
    })
}

/// ISO 8601 duration as seconds: `[+-]P[nW][nD][T[nH][nM][nS]]`.
fn duration(value: &str) -> Option<i64> {
    let v = value.trim();
    let (sign, v) = match v.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, v.strip_prefix('+').unwrap_or(v)),
    };
    let v = v.strip_prefix(['P', 'p'])?;
    let mut total: i64 = 0;
    let mut n: Option<i64> = None;
    let mut seen = false;
    let mut in_time = false;
    for c in v.chars() {
        if let Some(d) = c.to_digit(10) {
            n = Some(n.unwrap_or(0).checked_mul(10)?.checked_add(i64::from(d))?);
            continue;
        }
        let unit = match (c.to_ascii_uppercase(), in_time) {
            ('T', _) => {
                in_time = true;
                continue;
            }
            ('W', false) => 604_800,
            ('D', false) => 86_400,
            ('H', true) => 3600,
            ('M', true) => 60,
            ('S', true) => 1,
            _ => return None,
        };
        total = total.checked_add(n.take()?.checked_mul(unit)?)?;
        seen = true;
    }
    // Beyond about 300 years is nonsense, and keeps every sum below in range.
    (seen && n.is_none() && total <= MAX_DURATION_S).then_some(total * sign)
}

fn add_days(d: (i32, u8, u8), days: i64) -> (i32, u8, u8) {
    let midnight = NaiveParts {
        year: d.0,
        month: d.1,
        day: d.2,
        hour: 0,
        minute: 0,
        second: 0,
    };
    let c = NaiveParts::from_utc_secs(midnight.as_utc_secs() + days * 86_400);
    (c.year, c.month, c.day)
}

/// DTSTART, DTEND and DURATION of one event. `None` without a readable
/// DTSTART. A DTEND of another kind than DTSTART, or an end not after the
/// start of an all-day event, is ignored.
pub(super) fn read(
    start: &Prop,
    end: Option<&Prop>,
    dur: Option<&Prop>,
    vtz: &[VTimezone],
) -> Option<When> {
    let end = end.and_then(stamp);
    let secs = dur.and_then(|p| duration(&p.value)).filter(|s| *s >= 0);
    Some(match stamp(start)? {
        Stamp::Date(d) => {
            let end = match (end, secs) {
                (Some(Stamp::Date(e)), _) if e > d => e,
                (_, Some(s)) if s >= 86_400 => add_days(d, s / 86_400),
                _ => add_days(d, 1),
            };
            When::AllDay { start: d, end }
        }
        Stamp::Utc(p) => {
            let a = p.as_utc_secs();
            let (b, ok) = match (end, secs) {
                (Some(Stamp::Utc(e)), _) => (e.as_utc_secs(), true),
                (Some(Stamp::Local(e, tz)), _) => zones::resolve_or_utc(tz.as_deref(), e, vtz),
                (_, Some(s)) => (a + s, true),
                _ => (a, true),
            };
            When::Timed {
                start: a,
                end: b.max(a),
                zone_unknown: !ok,
            }
        }
        Stamp::Local(p, Some(tz)) => {
            let (a, ok_a) = zones::resolve_or_utc(Some(&tz), p, vtz);
            let (b, ok_b) = match (end, secs) {
                (Some(Stamp::Local(e, e_tz)), _) => {
                    zones::resolve_or_utc(e_tz.as_deref().or(Some(&tz)), e, vtz)
                }
                (Some(Stamp::Utc(e)), _) => (e.as_utc_secs(), true),
                (_, Some(s)) => (a + s, true),
                _ => (a, true),
            };
            When::Timed {
                start: a,
                end: b.max(a),
                zone_unknown: !(ok_a && ok_b),
            }
        }
        Stamp::Local(p, None) => {
            let a = p.as_utc_secs();
            let b = match (end, secs) {
                (Some(Stamp::Local(e, _)), _) => e.as_utc_secs(),
                (_, Some(s)) => a + s,
                _ => a,
            };
            When::Floating {
                start: p,
                end: NaiveParts::from_utc_secs(b.max(a)),
            }
        }
    })
}
