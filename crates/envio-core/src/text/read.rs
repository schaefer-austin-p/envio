//! The words for reading mail.

use crate::i18n::tr;

/// Shown in place of an image that was not loaded.
pub fn blocked_images() -> String {
    tr("Image not loaded")
}

pub fn message_unavailable() -> String {
    tr("This message is not available right now. Check your connection and try again")
}

pub fn attachment_not_saved() -> String {
    tr("Could not save the attachment")
}

pub fn could_not_read() -> String {
    tr("Could not read your mail from this phone")
}
