//! One-line notices the worker shows the user.

use crate::i18n::{fill, tr, tr_n};
use envio_store::{Action, ActionKind};

pub fn refused(reason: &str) -> String {
    fill(&tr("The server refused: {reason}"), &[("reason", reason)])
}

pub fn cache_rebuilt() -> String {
    tr("The mail cache was damaged and is being rebuilt from the server")
}

pub fn store_problem() -> String {
    tr("Envio cannot open its mail storage. Your unsent mail is safe; trying again")
}

pub fn rebuild_failed() -> String {
    tr("The mail cache is damaged and could not be rebuilt. Free some space or restart Envio")
}

pub fn disk_full() -> String {
    tr("Storage is full. Free some space to keep mail up to date")
}

/// `Core::start` failed (folders or logs could not be opened).
pub fn start_failed() -> String {
    tr("Envio could not start. Restart it, or free some space if that does not help")
}

pub fn worker_restarted() -> String {
    tr("Envio hit a problem and restarted its mail engine. Your unsent mail is safe")
}

/// A server-side action failed for good and was undone on screen.
pub fn reverted(action: &Action) -> String {
    match action.kind {
        ActionKind::SetFlags { .. } => tr("Couldn't update the message on the server; it's back"),
        ActionKind::Move { .. } => tr("Couldn't move the message on the server; it's back"),
        ActionKind::Delete => tr("Couldn't delete the message on the server; it's back"),
    }
}

/// A server-side action could not be done after many tries.
pub fn given_up(action: &Action) -> String {
    match action.kind {
        ActionKind::SetFlags { .. } => tr("Gave up updating the message on the server; it's back"),
        ActionKind::Move { .. } => tr("Gave up moving the message on the server; it's back"),
        ActionKind::Delete => tr("Gave up deleting the message on the server; it's back"),
    }
}

pub fn settings_not_saved() -> String {
    tr("Could not save your settings")
}

pub fn contact_not_saved() -> String {
    tr("Could not save the contact")
}

/// The journal could not make a new action durable: it may replay.
pub fn change_may_still_apply() -> String {
    tr("This change couldn't be saved on the phone. It may still be applied.")
}

/// The journal could not make an Undo durable: the change may come back.
pub fn undo_may_still_apply() -> String {
    tr("Undo couldn't be saved on the phone. The change may still be undone.")
}

/// Journal records a newer Envio wrote, left alone at start.
pub fn journal_skipped(n: u64) -> String {
    fill(
        &tr_n(
            "{n} pending change could not be read and was skipped",
            "{n} pending changes could not be read and were skipped",
            n,
        ),
        &[("n", &n.to_string())],
    )
}
