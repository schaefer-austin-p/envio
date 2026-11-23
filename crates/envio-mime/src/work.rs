//! A deterministic work counter for the linear-growth tests (#365).
//!
//! Every scan the sanitizer passes make goes through a primitive here
//! ([`find_lt`], [`find`], [`tick`]), and the shared tag
//! primitives (`tag_len`, `parse_tag`, `Tag::attr`, `Tag::write`,
//! `escape_into`, `style_value`) tick for the bytes they visit. A pass that
//! rescans its input or its output, however deep inside a helper, therefore
//! adds to the count. A test compares the count at N and at 2N.
//!
//! Without the `test-hooks` feature every function is the plain `str`
//! method (or empty) and inlines away: release builds pay nothing.

#[cfg(feature = "test-hooks")]
mod counting {
    use std::cell::Cell;

    thread_local! {
        static WORK: Cell<u64> = const { Cell::new(0) };
    }

    pub(crate) fn tick(n: usize) {
        WORK.with(|w| w.set(w.get().saturating_add(n as u64)));
    }

    pub(super) fn take() -> u64 {
        WORK.with(|w| w.replace(0))
    }
}

/// Adds `n` units (bytes or nodes visited) to this thread's count.
#[inline(always)]
pub(crate) fn tick(n: usize) {
    #[cfg(feature = "test-hooks")]
    counting::tick(n);
    #[cfg(not(feature = "test-hooks"))]
    let _ = n;
}

/// `s.find('<')`, counting the bytes looked at.
#[inline(always)]
pub(crate) fn find_lt(s: &str) -> Option<usize> {
    find(s, "<")
}

/// `s.find(pat)`, counting the bytes looked at (to the match, else all).
#[inline(always)]
pub(crate) fn find(s: &str, pat: &str) -> Option<usize> {
    let at = s.find(pat);
    tick(at.map_or(s.len(), |i| i + pat.len()));
    at
}

/// Resets this thread's count and returns the work done since the last
/// reset. Test hook: `sanitize_html` itself never reads it.
#[cfg(feature = "test-hooks")]
#[doc(hidden)]
pub fn take_work() -> u64 {
    counting::take()
}
