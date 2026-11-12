//! Words about a link before it opens (the reader's link sheet).

use crate::i18n::{fill, tr};

/// Userinfo (text before the @) is decoration: the link opens `host`.
pub fn userinfo_decoration(host: &str) -> String {
    fill(
        &tr("Before the @ is decoration. This link opens {host}."),
        &[("host", host)],
    )
}

/// The sheet's bold host is ASCII; this is how the same host would look
/// in the address bar's Unicode form.
pub fn looks_like(unicode: &str) -> String {
    fill(
        &tr("Displays as {unicode}, which may imitate another site."),
        &[("unicode", unicode)],
    )
}

pub fn malformed() -> String {
    tr("This link looks malformed and can't be opened.")
}
