//! Envio's sync engine. Planners in `plan` are pure: server state and
//! cache state in, steps out. The `driver` runs those steps against an
//! IMAP or SMTP session and the store, one database transaction per step,
//! so a cut connection resumes where it stopped. Sync spawns nothing and
//! never sleeps: the caller (envio-core's worker) passes the time in and
//! asks `next_due` when to call again.

pub mod convert;
pub mod error;
pub mod schedule;

pub use error::{ErrorKind, Result, SyncError};
pub use schedule::{Key, POLL_MS, Schedule, UNDO_MS};
