//! Envio's local store: the SQLite cache of the server, message bodies on
//! disk, and the unsent work (account settings, action journal, Outbox and
//! drafts) kept in files outside the cache so a rebuild can't lose it.

mod accounts;
mod addresses;
mod atomic;
mod bodies;
mod db;
mod drafts;
mod error;
mod folders;
mod ids;
mod invites;
mod journal;
mod journal_replay;
mod lookup;
mod messages;
mod migrate;
mod outbox;
mod paths;
mod prefetch;
mod rebuild;
mod search;
mod userdata;

pub use accounts::{Account, AccountSettings, Accounts, Security, Server};
pub use addresses::SeenAddress;
pub use bodies::Bodies;
pub use db::Db;
pub use drafts::{Draft, Drafts};
pub use error::{Result, StoreError};
pub use folders::{Folder, FolderId, FolderRole, SyncState};
pub use invites::{InviteAnswer, forget_invite, load_invites, record_invite};
pub use journal::{Action, ActionKind, Journal, MessageRef};
pub use journal_replay::{SkipReason, Skipped};
pub use messages::{
    BodyState, Cursor, Envelope, Flags, ListRef, Message, MessageRowId, NewMessage, PAGE,
};
pub use migrate::SCHEMA_VERSION;
pub use outbox::{NewOutboxItem, Outbox, OutboxItem, OutboxMeta, SendState};
pub use paths::StorePaths;
pub use rebuild::{Opened, open_or_rebuild};
pub use search::{BODY_INDEX_BYTES, Within};
pub use userdata::{
    Contact, DarkMode, ImagesDefault, Settings, ThemeName, load_contacts, load_settings,
    save_contacts, save_settings,
};
