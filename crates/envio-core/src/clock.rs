//! The wall clock, in one place.

use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "test-hooks")]
thread_local! {
    /// How far a test moved this thread's clock (`Command::AdvanceClock`).
    static SKEW_MS: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
}

/// Moves this thread's clock forward (the worker's, in a test).
#[cfg(feature = "test-hooks")]
pub(crate) fn advance(ms: i64) {
    SKEW_MS.with(|s| s.set(s.get().saturating_add(ms)));
}

/// Unix milliseconds now.
pub(crate) fn now_ms() -> i64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
    #[cfg(feature = "test-hooks")]
    let now = now.saturating_add(SKEW_MS.with(std::cell::Cell::get));
    now
}
