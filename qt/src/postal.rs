//! The core's `Poster` on Ubuntu Touch: Lomiri's Postal over QtDBus
//! (qt/postal.cpp). The card's JSON comes from `Alert::to_postal_json`; the
//! calls are queued to the Qt thread, so they never block the worker. A post
//! clears the card's tag first, so bursts leave one card per account.

use envio_core::{Alert, Poster};

#[cfg(not(test))]
unsafe extern "C" {
    fn envio_postal_post(json: *const u8, len: i32, tag: *const u8, tag_len: i32);
    fn envio_postal_set_counter(count: i32);
}

// `cargo test` links no C++: the calls do nothing there.
#[cfg(test)]
unsafe fn envio_postal_post(_json: *const u8, _len: i32, _tag: *const u8, _tag_len: i32) {}
#[cfg(test)]
unsafe fn envio_postal_set_counter(_count: i32) {}

pub(crate) struct PostalPoster;

impl Poster for PostalPoster {
    fn post(&self, alert: &Alert) {
        let json = alert.to_postal_json();
        let Ok(len) = i32::try_from(json.len()) else {
            return;
        };
        // The card replaces the account's earlier one: the C++ side clears
        // the tag first (a local Post does not replace by tag itself).
        let Ok(tag_len) = i32::try_from(alert.tag.len()) else {
            return;
        };
        // SAFETY: the pointers and lengths describe `json` and `alert.tag`,
        // which outlive the call; the C++ side copies the bytes before it
        // returns.
        unsafe { envio_postal_post(json.as_ptr(), len, alert.tag.as_ptr(), tag_len) };
    }

    fn badge(&self, total_unread: u32) {
        // SAFETY: a plain value; the C++ side takes no pointers.
        unsafe { envio_postal_set_counter(counter(total_unread)) };
    }
}

fn counter(total_unread: u32) -> i32 {
    i32::try_from(total_unread).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::counter;

    #[test]
    fn the_counter_is_clamped_to_what_dbus_takes() {
        assert_eq!(counter(7), 7);
        assert_eq!(counter(0), 0);
        assert_eq!(counter(u32::MAX), i32::MAX);
    }
}
