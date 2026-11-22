//! The Graph backend behind the `MailServer` and `MailSender` traits.
//! Graph has no uids, so `GraphServer` keeps a uid map in the cache file
//! (`GraphMap`), and the planner, journal and drafts above it run unchanged.
//! This file holds the struct and the trait impl; each concept is in its own
//! module: folders and paths, the delta refresh, the lookups, and (later)
//! messages, flags, search and send.

mod folders;
mod map;
mod meta;
mod refresh;

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

fn todo<T>() -> Result<T> {
    Err(NetError::Refused("not on Graph yet".into()))
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

    async fn search_since(&mut self, _unix_secs: i64) -> Result<Vec<u32>> {
        todo()
    }

    async fn search_all(&mut self) -> Result<Vec<u32>> {
        todo()
    }

    async fn search_message_id(&mut self, _message_id: &str) -> Result<Vec<u32>> {
        todo()
    }

    async fn search_message_id_deep(&mut self, _message_id: &str) -> Result<Vec<u32>> {
        todo()
    }

    async fn search_text(&mut self, _text: &str) -> Result<Vec<u32>> {
        todo()
    }

    async fn fetch_summaries(&mut self, _uids: &[u32]) -> Result<Vec<Summary>> {
        todo()
    }

    async fn fetch_flags(
        &mut self,
        _uids: &[u32],
        _changed_since: Option<u64>,
    ) -> Result<Vec<FlagUpdate>> {
        todo()
    }

    async fn fetch_body(&mut self, _uid: u32, _size: usize) -> Result<Option<Vec<u8>>> {
        todo()
    }

    async fn fetch_partial(&mut self, _uid: u32) -> Result<Option<Vec<u8>>> {
        todo()
    }

    async fn fetch_section(
        &mut self,
        _uid: u32,
        _section: &str,
        _size: usize,
    ) -> Result<Option<Vec<u8>>> {
        todo()
    }

    async fn store_flags(&mut self, _uids: &[u32], _add: bool, _flags: &[Flag]) -> Result<()> {
        todo()
    }

    async fn move_to(&mut self, _uids: &[u32], _target: &str) -> Result<()> {
        todo()
    }

    async fn delete(&mut self, _uids: &[u32]) -> Result<()> {
        todo()
    }

    async fn append(&mut self, _folder: &str, _flags: &[Flag], _eml: &[u8]) -> Result<()> {
        todo()
    }
}
