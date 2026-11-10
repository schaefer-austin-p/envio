//! One-line notices the worker shows the user.

use crate::i18n::{fill, tr};
use envio_store::{Action, ActionKind};

pub fn refused(reason: &str) -> String {
    fill(&tr("The server refused: {reason}"), &[("reason", reason)])
}

pub fn cache_rebuilt() -> String {
    tr("The mail cache was damaged and is being rebuilt from the server")
}

pub fn disk_full() -> String {
    tr("Storage is full. Free some space to keep mail up to date")
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
