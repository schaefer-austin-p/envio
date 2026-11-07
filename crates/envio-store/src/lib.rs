//! Envio's local store: the SQLite cache of the server, message bodies on
//! disk, and the unsent work (account settings, action journal, Outbox,
//! drafts) kept in files outside the cache so a rebuild can't lose it.

mod db;
mod error;
mod migrate;

pub use db::Db;
pub use error::{Result, StoreError};
pub use migrate::SCHEMA_VERSION;
