//! The words for reading mail.

use crate::i18n::{fill, tr};

/// Shown in place of an image that was not loaded.
pub fn blocked_images() -> String {
    tr("Image not loaded")
}

/// Shown after a message's text that envio kept only the start of.
pub fn text_cut_short() -> String {
    tr("The rest of this long text is not shown")
}

pub fn message_unavailable() -> String {
    tr("This message is not available right now. Check your connection and try again")
}

pub fn attachment_not_saved() -> String {
    tr("Could not save the attachment")
}

pub fn invite_not_exported() -> String {
    tr("Could not add the invite to the calendar")
}

pub fn could_not_read() -> String {
    tr("Could not read your mail from this phone")
}

pub fn search_unavailable() -> String {
    tr("This folder can't be searched on the server right now")
}

pub fn older_unavailable() -> String {
    tr("Couldn't load older messages. Try again when you are online.")
}

/// Some of the unified inbox's accounts could not load older mail.
pub fn older_unavailable_for(accounts: &str) -> String {
    fill(
        &tr("Couldn't load older messages for {accounts}. Try again when you are online."),
        &[("accounts", accounts)],
    )
}

/// Shown in a list row whose message has no usable From.
pub fn no_sender() -> String {
    tr("(no sender)")
}

/// The line a Drafts row shows in place of the sender: who the draft is for.
pub fn to_recipients(recipients: &str) -> String {
    fill(&tr("To: {recipients}"), &[("recipients", recipients)])
}

/// What `to_recipients` lists for a draft with no recipient yet.
pub fn no_recipients_listed() -> String {
    tr("(no recipients)")
}

/// Shown in a list row whose message has a blank subject.
pub fn no_subject() -> String {
    tr("(no subject)")
}
