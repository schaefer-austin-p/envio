//! What the core tells the app.

use super::command::{AccountSetup, ComposeId, DraftInput, UndoId};
use std::path::PathBuf;

use super::view::{AccountInfo, CacheState, FolderRow, MessageRow, MessageView, UnsentCounts};
use crate::health::AccountStatus;
use crate::servers::Service;
use envio_store::{FolderId, MessageRowId, Page};

#[derive(Debug, Clone)]
pub enum Event {
    /// The store is open; sent at every (re)start of the worker.
    Started {
        accounts: Vec<AccountInfo>,
        cache: CacheState,
    },
    /// The worker panicked and came back.
    WorkerRestarted {
        text: String,
    },
    /// A one-line message for the user.
    Notice {
        text: String,
    },
    /// An account's status changed; `text` is what to show.
    AccountStatus {
        account: String,
        status: AccountStatus,
        text: String,
    },
    /// The account's folder list changed.
    FoldersChanged {
        account: String,
    },
    /// Messages in these folders changed (new, gone, flags, bodies).
    MessagesChanged {
        folders: Vec<FolderId>,
    },
    /// The Outbox changed (sent, failed, filed).
    OutboxChanged,
    /// Servers found for an address; `text` says where, or asks the user to
    /// confirm the hosts.
    Detected {
        email: String,
        setup: AccountSetup,
        text: String,
    },
    /// Adding the account did not work.
    SetupProblem {
        email: String,
        text: String,
    },
    /// The server's certificate is not trusted: show the fingerprint and
    /// ask; on yes, `Connect` again with it in `accepted`.
    TrustPrompt {
        email: String,
        service: Service,
        host: String,
        fingerprint: String,
        hex: String,
        text: String,
    },
    AccountAdded {
        account: AccountInfo,
    },
    /// Unsent work would be lost: ask, then `RemoveAccount` confirmed.
    ConfirmRemove {
        account: String,
        unsent: UnsentCounts,
        text: String,
    },
    AccountRemoved {
        account: String,
    },
    /// The answer to `ListFolders`.
    Folders {
        account: String,
        folders: Vec<FolderRow>,
    },
    /// The answer to `ListMessages`.
    Messages {
        folder: FolderId,
        page: Page,
        rows: Vec<MessageRow>,
    },
    /// The answer to `Search`.
    SearchResults {
        account: String,
        query: String,
        rows: Vec<MessageRow>,
    },
    MessageOpened {
        id: MessageRowId,
        view: MessageView,
    },
    /// The body can't be had now (gone from the server, or the account
    /// can't connect); `text` says so.
    MessageUnavailable {
        id: MessageRowId,
        text: String,
    },
    AttachmentSaved {
        id: MessageRowId,
        index: usize,
        path: PathBuf,
    },
    /// An action was done; it can be taken back until `expires_at_ms`
    /// (unix milliseconds).
    UndoOffer {
        undo: UndoId,
        text: String,
        expires_at_ms: i64,
    },
    /// The offer is over: taken back (`undone`) or gone for good.
    UndoClosed {
        undo: UndoId,
        undone: bool,
    },
    /// The answer to `StartCompose`: the fields to show. `quoted` is the
    /// reply or forward text, kept apart so the app can fold it.
    ComposeReady {
        compose: ComposeId,
        draft: DraftInput,
        quoted: String,
    },
    /// The draft was stored for the first time as `draft_id`.
    DraftSaved {
        compose: ComposeId,
        draft_id: String,
    },
    /// Compose can not go on (or `Send` did not queue); `text` says why.
    /// The draft is kept.
    ComposeRefused {
        compose: ComposeId,
        text: String,
    },
    /// The message is in the Outbox as `item`.
    Queued {
        compose: ComposeId,
        item: String,
    },
}
