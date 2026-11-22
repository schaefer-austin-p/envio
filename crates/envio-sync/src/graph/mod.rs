//! The Graph backend behind the `MailServer` and `MailSender` traits.
//! Graph has no uids, so `GraphServer` keeps a uid map in the cache file
//! (`GraphMap`), and the planner, journal and drafts above it run unchanged.
//! This file holds the struct and the trait impl; each concept is in its own
//! module: folders and paths, the delta refresh, the lookups, and (later)
//! messages, flags, search and send.

mod flags;
mod folders;
mod map;
mod messages;
mod meta;
mod moves;
mod refresh;
mod search;

use std::collections::BTreeMap;

use envio_net::graph::GraphClient;
use envio_net::imap::{Caps, Flag, FlagUpdate, Folder, FolderState, Summary};
use envio_net::{NetError, Result, StorageKind};
use envio_store::{Db, FolderId, GraphMap, StoreError};

use crate::server::MailServer;
use map::{Entry, Selected};

/// What the Graph backend can do: CONDSTORE is emulated from the map's
/// modseq, there is no IDLE, and a body always downloads whole.
pub const GRAPH_CAPS: Caps = Caps {
    condstore: true,
    qresync: false,
    idle: false,
    r#move: true,
    uidplus: false,
    special_use: true,
    literal_plus: false,
    literal_minus: false,
    no_partial: true,
};

/// A delta drained but not yet committed: the folder's sync state is saved
/// with `commit_folder`, in the step that stores the finished sync.
struct Pending {
    /// The delta link to keep once the sync is stored.
    link: String,
    modseq: u64,
}

pub struct GraphServer {
    client: GraphClient,
    map: GraphMap,
    paths: BTreeMap<String, Entry>,
    pending: BTreeMap<FolderId, Pending>,
    selected: Option<Selected>,
}

impl GraphServer {
    pub fn new(client: GraphClient, map: GraphMap) -> Self {
        Self {
            client,
            map,
            paths: BTreeMap::new(),
            pending: BTreeMap::new(),
            selected: None,
        }
    }
}

/// A store failure as the net error the driver understands.
pub(crate) fn storage(e: StoreError) -> NetError {
    let kind = if e.is_damage() {
        StorageKind::Damaged
    } else if e.is_disk_full() {
        StorageKind::DiskFull
    } else {
        StorageKind::Other
    };
    NetError::Storage {
        kind,
        msg: e.to_string(),
    }
}

impl MailServer for GraphServer {
    fn caps(&self) -> Caps {
        GRAPH_CAPS
    }

    async fn list_folders(&mut self) -> Result<Vec<Folder>> {
        self.list().await
    }

    async fn select(&mut self, path: &str) -> Result<FolderState> {
        self.select_path(path).await
    }

    async fn refresh(&mut self, path: &str) -> Result<()> {
        self.refresh_path(path).await
    }

    fn commit_folders(&mut self, db: &Db, account: &str) -> envio_store::Result<()> {
        self.commit_all(db, account)
    }

    fn commit_folder(&mut self, db: &Db, id: FolderId) -> envio_store::Result<()> {
        self.commit_one(db, id)
    }

    async fn search_since(&mut self, unix_secs: i64) -> Result<Vec<u32>> {
        self.uids_since(unix_secs.saturating_mul(1000))
    }

    async fn search_all(&mut self) -> Result<Vec<u32>> {
        self.uids_since(i64::MIN)
    }

    async fn search_message_id(&mut self, message_id: &str) -> Result<Vec<u32>> {
        self.find_id(message_id).await
    }

    async fn search_message_id_deep(&mut self, message_id: &str) -> Result<Vec<u32>> {
        // The server filter is exact, so there is no scan to fall back to.
        self.find_id(message_id).await
    }

    async fn search_text(&mut self, text: &str) -> Result<Vec<u32>> {
        self.find_text(text).await
    }

    async fn fetch_summaries(&mut self, uids: &[u32]) -> Result<Vec<Summary>> {
        self.summaries(uids).await
    }

    async fn fetch_flags(
        &mut self,
        uids: &[u32],
        changed_since: Option<u64>,
    ) -> Result<Vec<FlagUpdate>> {
        self.flag_updates(uids, changed_since)
    }

    async fn fetch_body(&mut self, uid: u32, _size: usize) -> Result<Option<Vec<u8>>> {
        self.body(uid).await
    }

    /// Never reached: `no_partial` makes bodies download whole.
    async fn fetch_partial(&mut self, _uid: u32) -> Result<Option<Vec<u8>>> {
        Err(NetError::Protocol("not on Graph".into()))
    }

    /// Never reached, like `fetch_partial`.
    async fn fetch_section(
        &mut self,
        _uid: u32,
        _section: &str,
        _size: usize,
    ) -> Result<Option<Vec<u8>>> {
        Err(NetError::Protocol("not on Graph".into()))
    }

    async fn store_flags(&mut self, uids: &[u32], add: bool, flags: &[Flag]) -> Result<()> {
        self.set_flags(uids, add, flags).await
    }

    async fn move_to(&mut self, uids: &[u32], target: &str) -> Result<()> {
        self.move_uids(uids, target).await
    }

    async fn delete(&mut self, uids: &[u32]) -> Result<()> {
        self.delete_uids(uids).await
    }

    async fn append(&mut self, folder: &str, _flags: &[Flag], eml: &[u8]) -> Result<()> {
        self.file(folder, eml).await
    }
}
