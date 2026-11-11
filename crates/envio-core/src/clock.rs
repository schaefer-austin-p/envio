//! The wall clock, in one place.

use std::time::{SystemTime, UNIX_EPOCH};

/// Unix milliseconds now.
pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}
