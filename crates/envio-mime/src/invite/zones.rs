//! Resolving a TZID to a UTC offset. A1 has only the seam: every TZID is
//! unknown. A2 fills `resolve` with IANA, Windows and VTIMEZONE lookups.

use super::when::NaiveParts;

/// What a part's `BEGIN:VTIMEZONE` block says about its zone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VTimezone {
    pub tzid: String,
    /// `TZOFFSETTO` of the first `STANDARD` block, in seconds.
    pub standard_offset_s: i32,
}

/// Unix seconds of `local` in zone `tzid`; `None` when the zone is unknown.
pub(super) fn resolve(_tzid: &str, _local: NaiveParts, _vtz: &[VTimezone]) -> Option<i64> {
    None
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
