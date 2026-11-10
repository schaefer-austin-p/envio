//! The thin driver: runs the planners' steps against a server and the
//! store, one database transaction per step. A network error ends the run
//! (the session is unusable after it); everything done so far is kept, and
//! the next run resumes from the cache.

mod actions;
mod folders;

pub use actions::{Undo, record, undo};
pub use folders::{Synced, fetch_body, sync_folder, sync_folders};

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
