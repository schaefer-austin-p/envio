//! What the core tells the app.

use super::command::AccountSetup;
use super::view::{AccountInfo, CacheState, UnsentCounts};
use crate::health::AccountStatus;
use crate::servers::Service;
use envio_store::FolderId;

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
}
