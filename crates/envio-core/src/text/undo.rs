//! The words for acting on messages and taking it back.

use crate::api::command::UserAction;
use crate::i18n::{fill, tr, tr_n};

/// What was done, shown with the Undo button.
pub fn offer(action: UserAction, count: usize) -> String {
    let n = u64::try_from(count).unwrap_or(u64::MAX);
    let shown = n.to_string();
    let text = match action {
        UserAction::MarkRead(true) => tr_n("Marked as read", "Marked {n} messages as read", n),
        UserAction::MarkRead(false) => tr_n("Marked as unread", "Marked {n} messages as unread", n),
        UserAction::Flag(true) => tr_n("Flagged", "Flagged {n} messages", n),
        UserAction::Flag(false) => tr_n("Flag removed", "Removed the flag from {n} messages", n),
        UserAction::Archive => tr_n("Archived", "Archived {n} messages", n),
        UserAction::Delete => tr_n("Deleted", "Deleted {n} messages", n),
        UserAction::Move { .. } => tr_n("Moved", "Moved {n} messages", n),
    };
    fill(&text, &[("n", &shown)])
}

pub fn too_late() -> String {
    tr("Too late to undo: the change has already gone to the server")
}

pub fn cannot_undo() -> String {
    tr("Done, but it can not be undone")
}

/// Archive or Move was asked of a message in a Drafts folder: its local
/// draft would stay behind, so Delete is the only way out.
pub fn drafts_delete_only() -> String {
    tr("A draft can only be deleted.")
}

pub fn no_archive() -> String {
    tr("This account has no Archive folder")
}

/// The action could not be recorded; nothing changed.
pub fn not_done() -> String {
    tr("Could not do that. Nothing was changed")
}
