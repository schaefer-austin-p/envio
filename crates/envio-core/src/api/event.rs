//! What the core tells the app.

use super::view::{AccountInfo, CacheState};
use crate::health::AccountStatus;
use envio_store::FolderId;

#[derive(Debug, Clone)]
pub enum Event {
    /// The store is open; sent at every (re)start of the worker.
    Started {
        accounts: Vec<AccountInfo>,
        cache: CacheState,
    },
    /// The worker panicked and came back.
    WorkerRestarted { text: String },
    /// A one-line message for the user.
    Notice { text: String },
    /// An account's status changed; `text` is what to show.
    AccountStatus {
        account: String,
        status: AccountStatus,
        text: String,
    },
    /// The account's folder list changed.
    FoldersChanged { account: String },
    /// Messages in these folders changed (new, gone, flags, bodies).
    MessagesChanged { folders: Vec<FolderId> },
    /// The Outbox changed (sent, failed, filed).
    OutboxChanged,
}
