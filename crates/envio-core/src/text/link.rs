//! Words about a link before it opens (the reader's link sheet).

use crate::i18n::{fill, tr};

/// Userinfo (text before the @) is decoration: the link opens `host`.
pub fn userinfo_decoration(host: &str) -> String {
    fill(
        &tr("Before the @ is decoration. This link opens {host}."),
        &[("host", host)],
    )
}

/// The host is written in another script; this is its ASCII form.
pub fn written_as(punycode: &str) -> String {
    fill(
        &tr("The address is written as {punycode}."),
        &[("punycode", punycode)],
    )
}

pub fn malformed() -> String {
    tr("This link looks malformed and can't be opened.")
}
