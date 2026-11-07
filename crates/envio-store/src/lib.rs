//! Envio's local store: the SQLite cache of the server, message bodies on
//! disk, and the unsent work (account settings, action journal, Outbox,
//! drafts) kept in files outside the cache so a rebuild can't lose it.

mod db;
mod error;
mod folders;
mod messages;
mod migrate;
mod rebuild;
mod search;

pub use db::Db;
pub use error::{Result, StoreError};
pub use folders::{Folder, FolderId, FolderRole, SyncState};
pub use messages::{BodyState, Envelope, Flags, Message, MessageRowId, NewMessage, Page};
pub use migrate::SCHEMA_VERSION;
pub use rebuild::{Opened, open_or_rebuild};
