//! One-shot events the screens react to (dialogs, banners, navigation).

use std::path::PathBuf;

use envio_core::{AccountSetup, ComposeId, DraftInput, MessageView, Service, Settings, UndoId};

#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    Notice(String),
    StoreProblem(String),
    WorkerRestarted(String),
    Detected {
        email: String,
        text: String,
        /// SRV hosts outside the email domain: ask before the password goes.
        needs_confirm: bool,
        setup: AccountSetup,
    },
    SetupProblem {
        email: String,
        text: String,
    },
    TrustPrompt {
        email: String,
        service: Service,
        host: String,
        fingerprint: String,
        /// The fingerprint as `Connect` takes it in `accepted`.
        hex: String,
        text: String,
    },
    AccountAdded(String),
    ConfirmRemove {
        account: String,
        text: String,
    },
    MessageOpened(Box<MessageView>),
    MessageUnavailable {
        id: i64,
        text: String,
    },
    AttachmentSaved {
        path: PathBuf,
    },
    UndoOffer {
        undo: UndoId,
        text: String,
    },
    UndoClosed,
    ComposeReady {
        compose: ComposeId,
        draft: DraftInput,
        quoted: String,
    },
    DraftSaved {
        compose: ComposeId,
    },
    ComposeRefused {
        compose: ComposeId,
        text: String,
    },
    Queued {
        compose: ComposeId,
    },
    Attached {
        compose: ComposeId,
        path: PathBuf,
        name: String,
        size: u64,
    },
    AttachFailed {
        compose: ComposeId,
        text: String,
    },
    Settings(Settings),
    SearchFailed(String),
    /// An account's status line changed.
    Status {
        account: String,
        text: String,
    },
}
