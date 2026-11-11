//! Core views to rows, and one-shot core events to signals.

use envio_core::{
    AccountInfo, AccountStatus, Event, FolderRole, FolderRow, OutboxRow, OutboxStatus, Suggestion,
};

use crate::rows::{AccountRow, FolderRowUi, OutboxRowUi, Row, SuggestionRow};
use crate::signals::Signal;

/// The state word QML switches on.
pub fn state_word(status: &AccountStatus) -> &'static str {
    match status {
        AccountStatus::Ready | AccountStatus::Syncing | AccountStatus::Removing => "ok",
        AccountStatus::Offline { .. } => "offline",
        AccountStatus::SignInFailed => "auth",
        AccountStatus::Certificate(_) => "pin",
        AccountStatus::DiskFull => "diskfull",
        AccountStatus::Broken(_) => "broken",
    }
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
    }
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

pub fn outbox_row(r: &OutboxRow) -> Row {
    let status = match r.status {
        OutboxStatus::Queued | OutboxStatus::SentNotFiled => "waiting",
        OutboxStatus::Sending => "sending",
        OutboxStatus::MayHaveBeenSent { .. } => "maybesent",
        OutboxStatus::Failed { .. } | OutboxStatus::Damaged => "failed",
    };
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
        Event::UndoClosed { .. } => Signal::UndoClosed,
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
