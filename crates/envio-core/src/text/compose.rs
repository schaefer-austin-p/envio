//! The words for writing mail.

use envio_mime::ForwardLabels;

use crate::i18n::{fill, tr};

pub fn queued() -> String {
    tr("Sent to the Outbox")
}

pub fn no_recipients() -> String {
    tr("Add at least one recipient")
}

/// `text` is the address as typed.
pub fn bad_address(text: &str) -> String {
    fill(
        &tr("{address} is not a valid email address"),
        &[("address", text)],
    )
}

pub fn attachment_unreadable(name: &str) -> String {
    fill(
        &tr("Could not read the attachment {name}"),
        &[("name", name)],
    )
}

/// The line above a quoted reply.
pub fn attribution(name: &str, date: &str) -> String {
    fill(
        &tr("On {date}, {name} wrote:"),
        &[("date", date), ("name", name)],
    )
}

pub fn forward_labels() -> ForwardLabels {
    ForwardLabels {
        forwarded_message: tr("Forwarded message"),
        from: tr("From"),
        date: tr("Date"),
        subject: tr("Subject"),
        to: tr("To"),
    }
}

pub fn queue_failed() -> String {
    tr("Could not queue the message. It is still a draft")
}

pub fn no_account() -> String {
    tr("There is no account to write from")
}

pub fn draft_already_open() -> String {
    tr("This draft is already open.")
}

pub fn draft_unavailable() -> String {
    tr("This draft can not be opened")
}

pub fn draft_not_saved() -> String {
    tr("Could not save your draft")
}
