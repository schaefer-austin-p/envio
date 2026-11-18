//! The thin driver: runs the planners' steps against a server and the
//! store, one database transaction per step. A network error ends the run
//! (the session is unusable after it); everything done so far is kept, and
//! the next run resumes from the cache.

mod account;
mod actions;
mod body;
mod drafts;
mod folders;
mod older;
mod outbox;
mod prefetch;
mod replay;
mod search;

pub use account::{AccountSynced, Unsent, has_due_mail, remove_account, sync_account, unsent};
pub use actions::{Undo, record, undo};
pub use body::{PARTIAL_ABOVE, fetch_body, fetch_part, fetch_whole_body, is_big};
pub use drafts::{DraftRun, DraftsSynced, sync_drafts};
pub use folders::{BATCHES_PER_STEP, Options, Synced, sync_folder, sync_folder_as, sync_folders};
pub use older::{Older, load_older};
pub use outbox::{Filed, Sent, file_sent, send_outbox};
pub use prefetch::{Prefetched, prefetch_bodies};
pub use replay::{Replayed, replay};
pub use search::search_server;

use envio_store::{Bodies, Db, Journal, Outbox};

use crate::schedule::Schedule;

/// The store as sync uses it, for one run on core's worker thread.
pub struct Local<'a> {
    pub db: &'a Db,
    pub bodies: &'a Bodies,
    pub journal: &'a mut Journal,
    pub outbox: &'a Outbox,
    /// Retry waits for journal actions and Outbox items (in memory).
    pub schedule: &'a mut Schedule,
}
