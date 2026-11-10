//! What the core tells the app.

use super::view::{AccountInfo, CacheState};

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
}
