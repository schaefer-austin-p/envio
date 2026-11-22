//! One-shot events the screens react to (dialogs, banners, navigation).

use std::path::PathBuf;

use envio_core::{
    AccountSetup, ComposeId, DraftInput, MessageView, OAuthOffer, OAuthOutcome, Service, Settings,
    UndoId, UndoScope,
};

/// The "Load older messages" row at the foot of a list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OlderRow {
    /// Not shown: the list is not at its end, or it is a search.
    #[default]
    Hidden,
    /// Shown, pressing it loads a batch.
    Ready,
    /// A batch is on its way.
    Loading,
    /// Nothing older is left on the server.
    Done,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    Notice(String),
    StoreProblem(String),
    WorkerRestarted(String),
    /// The Ui moved the view to another folder by itself (#303): the page
    /// follows, so its search scope names the folder shown.
    ListSwitched {
        folder: i64,
    },
    Detected {
        email: String,
        text: String,
        /// SRV hosts outside the email domain: ask before the password goes.
        needs_confirm: bool,
        setup: AccountSetup,
        /// The two guessed servers, one per line.
        servers: String,
        /// Signing in with a provider is possible.
        oauth: Option<OAuthOffer>,
    },
    SetupProblem {
        email: String,
        text: String,
    },
    /// The address cannot be used (Microsoft, #381): a card with a title.
    SetupRefused {
        email: String,
        title: String,
        text: String,
    },
    OAuthBrowser {
        email: String,
        url: String,
        user_code: Option<String>,
    },
    OAuthConfirm {
        email: String,
        signed_in_as: Option<String>,
        text: String,
    },
    /// The sign-in was for the other kind of account than the pick.
    OAuthMismatch {
        email: String,
        title: String,
        text: String,
    },
    OAuthDone {
        email: String,
        outcome: OAuthOutcome,
    },
    TrustPrompt {
        email: String,
        service: Service,
        host: String,
        title: String,
        fingerprint: String,
        /// The fingerprint as `Connect` takes it in `accepted`.
        hex: String,
        text: String,
    },
    /// The new account and its address: a wizard only advances for the
    /// address it asked about.
    AccountAdded {
        account: String,
        email: String,
    },
    ConfirmRemove {
        account: String,
        text: String,
    },
    /// The opened message, named by its id: a reader that moved on to
    /// another message must be able to tell a late answer.
    MessageOpened {
        id: i64,
        view: Box<MessageView>,
    },
    MessageUnavailable {
        id: i64,
        text: String,
    },
    AttachmentSaved {
        path: PathBuf,
    },
    AttachmentFailed {
        id: i64,
    },
    /// The calendar file of message `id`'s invite was written.
    InviteExported {
        id: i64,
        path: PathBuf,
    },
    InviteExportFailed {
        id: i64,
    },
    UndoOffer {
        undo: UndoId,
        text: String,
        scope: UndoScope,
    },
    UndoClosed {
        undo: UndoId,
    },
    ComposeReady {
        compose: ComposeId,
        draft: DraftInput,
        quoted: String,
        note: String,
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
        text: String,
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
    /// The state of the "Load older messages" row changed.
    OlderRow(OlderRow),
    SearchFailed(String),
    /// The server search is over: its results came, or every account
    /// asked failed.
    SearchDone,
    /// An account's status line changed.
    Status {
        account: String,
        text: String,
    },
}
