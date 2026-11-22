//! The words for queued mail.

use crate::api::view::OutboxStatus;
use crate::i18n::{fill, tr};

pub fn status(status: &OutboxStatus) -> String {
    match status {
        OutboxStatus::Queued => tr("Waiting to be sent"),
        OutboxStatus::Sending => tr("Sending"),
        OutboxStatus::MayHaveBeenSent { asked: false, .. } => {
            tr("May have been sent: looking for it in Sent")
        }
        OutboxStatus::MayHaveBeenSent {
            asked: true,
            can_file: false,
        } => tr("May have been sent: not found in Sent. Send again, or delete it?"),
        OutboxStatus::MayHaveBeenSent { asked: true, .. } => {
            tr("May have been sent: not found in Sent. Send again, or move to Sent?")
        }
        OutboxStatus::SentNotFiled => tr("Sent, not yet in Sent"),
        OutboxStatus::Failed { reason } => {
            fill(&tr("Couldn't be sent: {reason}"), &[("reason", reason)])
        }
        OutboxStatus::Damaged => tr("This message can't be read"),
    }
}

/// The button on an Outbox entry held by the outgoing server's certificate.
pub fn check_certificate_label() -> String {
    tr("Check certificate")
}

/// The button on an Outbox entry held by a failed sign-in.
pub fn open_accounts_label() -> String {
    tr("Open accounts")
}

/// A decision the message's state does not allow.
pub fn not_now() -> String {
    tr("Not possible right now")
}

pub fn change_failed() -> String {
    tr("Could not change the message in the Outbox")
}

pub fn edit_invite_reply() -> String {
    tr("This reply to an invitation can't be edited. Retry it, or delete it.")
}

/// Added to the server's text when a Microsoft 365 mailbox refuses SMTP
/// sign-in with a good token.
pub fn smtp_disabled_hint() -> String {
    tr("Ask your administrator to allow Authenticated SMTP for this mailbox, then send again.")
}
