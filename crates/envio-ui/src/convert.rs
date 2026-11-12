//! Core views to rows, and one-shot core events to signals.

use envio_core::{
    AccountInfo, AccountStatus, Event, FolderRole, FolderRow, OutboxRow, OutboxStatus, Service,
    Suggestion,
};

use crate::rows::{AccountRow, FolderRowUi, OutboxRowUi, Row, SuggestionRow};
use crate::signals::Signal;

/// The state word QML switches on.
pub fn state_word(status: &AccountStatus) -> &'static str {
    match status {
        AccountStatus::Ready | AccountStatus::Syncing | AccountStatus::Removing => "ok",
        AccountStatus::Offline { .. } => "offline",
        AccountStatus::LocalError { .. } => "local",
        AccountStatus::SignInFailed => "auth",
        AccountStatus::Certificate(_) => "pin",
        AccountStatus::DiskFull => "diskfull",
        AccountStatus::Broken(_) => "broken",
    }
}

/// The unread count for display: the digits, or nothing for zero.
pub fn unread_text(n: u32) -> String {
    if n == 0 { String::new() } else { n.to_string() }
}

pub fn account_row(info: &AccountInfo, status_text: &str) -> AccountRow {
    let state = if info.broken {
        "broken"
    } else {
        state_word(&info.status)
    };
    AccountRow {
        key: info.key.clone(),
        name: info.name.clone(),
        email: info.email.clone(),
        status: status_text.to_owned(),
        state: state.to_owned(),
        unread: info.unread,
        unread_text: unread_text(info.unread),
        pin: Box::default(),
    }
}

/// The certificate fields of an account row for a status: filled for a
/// certificate problem with details, empty otherwise.
pub fn set_pin(row: &mut AccountRow, status: &AccountStatus, text: &str) {
    let (service, host, fingerprint, hex, text) = match status {
        AccountStatus::Certificate(Some(d)) => (
            match d.service {
                Service::Imap => "imap",
                Service::Smtp => "smtp",
            },
            d.host.clone(),
            d.presented.display(),
            d.presented.to_hex(),
            text,
        ),
        _ => ("", String::new(), String::new(), String::new(), ""),
    };
    service.clone_into(&mut row.pin.service);
    row.pin.host = host;
    row.pin.fingerprint = fingerprint;
    row.pin.hex = hex;
    text.clone_into(&mut row.pin.text);
}

pub fn role_word(role: FolderRole) -> &'static str {
    match role {
        FolderRole::Inbox => "inbox",
        FolderRole::Sent => "sent",
        FolderRole::Drafts => "drafts",
        FolderRole::Trash => "trash",
        FolderRole::Archive => "archive",
        FolderRole::Junk => "junk",
        FolderRole::Other => "",
    }
}

pub fn folder_row(account: &str, f: &FolderRow) -> Row {
    let depth = f.path.split(['/', '.']).count().saturating_sub(1);
    Row::Folder(FolderRowUi {
        id: f.id.0,
        account: account.to_owned(),
        name: f.name.clone(),
        role: role_word(f.role).to_owned(),
        unread: f.unread,
        depth: u32::try_from(depth).unwrap_or(u32::MAX),
    })
}

/// The state word QML switches on. Only `waiting` (and `sending`, on its
/// way) counts as waiting to send; `sentnotfiled` is done and has no actions.
pub fn outbox_state(status: &OutboxStatus) -> &'static str {
    match status {
        OutboxStatus::Queued => "waiting",
        OutboxStatus::Sending => "sending",
        OutboxStatus::SentNotFiled => "sentnotfiled",
        OutboxStatus::MayHaveBeenSent { .. } => "maybesent",
        OutboxStatus::Failed { .. } | OutboxStatus::Damaged => "failed",
    }
}

pub fn outbox_row(r: &OutboxRow) -> Row {
    let status = outbox_state(&r.status);
    Row::Outbox(OutboxRowUi {
        id: r.id.clone(),
        account: r.account.clone(),
        subject: r.subject.clone(),
        to: r.recipients.join(", "),
        status: status.to_owned(),
        text: r.text.clone(),
    })
}

pub fn suggestion_row(s: &Suggestion, query: &str) -> Row {
    Row::Suggestion(SuggestionRow {
        name: s.name.clone(),
        address: s.address.clone(),
        from_contacts: s.from_contacts,
        last_seen: s.last_seen.unwrap_or(0),
        query: query.to_owned(),
    })
}

/// The signal a one-shot event stands for; `None` for events that change
/// models or need the Ui's state.
pub fn signal_of(event: &Event) -> Option<Signal> {
    Some(match event.clone() {
        Event::Notice { text } => Signal::Notice(text),
        Event::StoreProblem { text } => Signal::StoreProblem(text),
        Event::WorkerRestarted { text } => Signal::WorkerRestarted(text),
        Event::Detected { email, setup, text } => Signal::Detected {
            email,
            text,
            needs_confirm: setup.needs_confirm,
            setup,
        },
        Event::SetupProblem { email, text } => Signal::SetupProblem { email, text },
        Event::TrustPrompt {
            email,
            service,
            host,
            fingerprint,
            hex,
            text,
        } => Signal::TrustPrompt {
            email,
            service,
            host,
            fingerprint,
            hex,
            text,
        },
        Event::ConfirmRemove { account, text, .. } => Signal::ConfirmRemove { account, text },
        Event::MessageOpened { id, view } => Signal::MessageOpened {
            id: id.0,
            view: Box::new(view),
        },
        Event::MessageUnavailable { id, text } => Signal::MessageUnavailable { id: id.0, text },
        Event::AttachmentSaved { path, .. } => Signal::AttachmentSaved { path },
        Event::AttachmentFailed { id, .. } => Signal::AttachmentFailed { id: id.0 },
        Event::UndoOffer { undo, text, .. } => Signal::UndoOffer { undo, text },
        Event::UndoClosed { undo, .. } => Signal::UndoClosed { undo },
        Event::ComposeReady {
            compose,
            draft,
            quoted,
        } => Signal::ComposeReady {
            compose,
            draft,
            quoted,
        },
        Event::DraftSaved { compose, .. } => Signal::DraftSaved { compose },
        Event::ComposeRefused { compose, text } => Signal::ComposeRefused { compose, text },
        Event::Queued { compose, text, .. } => Signal::Queued { compose, text },
        Event::Attached {
            compose,
            path,
            name,
            size,
        } => Signal::Attached {
            compose,
            path,
            name,
            size,
        },
        Event::AttachFailed { compose, text } => Signal::AttachFailed { compose, text },
        Event::Settings(settings) => Signal::Settings(settings),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outbox_states_keep_sending_and_sent_apart_from_waiting() {
        assert_eq!(outbox_state(&OutboxStatus::Queued), "waiting");
        assert_eq!(outbox_state(&OutboxStatus::Sending), "sending");
        assert_eq!(outbox_state(&OutboxStatus::SentNotFiled), "sentnotfiled");
        assert_eq!(
            outbox_state(&OutboxStatus::MayHaveBeenSent { asked: true }),
            "maybesent"
        );
        assert_eq!(outbox_state(&OutboxStatus::Damaged), "failed");
    }
}
