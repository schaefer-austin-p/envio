//! Envio's local store: the SQLite cache of the server, message bodies on
//! disk, and the unsent work (account settings, action journal, Outbox,
//! drafts) kept in files outside the cache so a rebuild can't lose it.

mod accounts;
mod atomic;
mod bodies;
mod db;
mod error;
mod folders;
mod ids;
mod journal;
mod messages;
mod migrate;
mod paths;
mod rebuild;
mod search;

pub use accounts::{Account, AccountSettings, Accounts, Server};
pub use bodies::Bodies;
pub use db::Db;
pub use error::{Result, StoreError};
pub use folders::{Folder, FolderId, FolderRole, SyncState};
pub use journal::{Action, ActionKind, Journal, MessageRef};
pub use messages::{BodyState, Envelope, Flags, Message, MessageRowId, NewMessage, Page};
pub use migrate::SCHEMA_VERSION;
pub use paths::StorePaths;
pub use rebuild::{Opened, open_or_rebuild};
