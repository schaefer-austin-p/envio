//! A whole store in a temporary folder, as core's worker would hold it.

use std::path::PathBuf;

use envio_store::{Bodies, Db, Drafts, Folder, Journal, ListRef, Outbox};
use envio_sync::Schedule;
use envio_sync::driver::Local;
use tempfile::TempDir;

pub const ACCOUNT: &str = "1700";

pub struct Store {
    pub dir: TempDir,
    /// The cache file, for `on_file` stores; empty for in-memory ones.
    pub db_path: PathBuf,
    pub db: Db,
    pub bodies: Bodies,
    pub journal: Journal,
    pub outbox: Outbox,
    pub drafts: Drafts,
    pub schedule: Schedule,
}

impl Store {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        Self {
            db: Db::open_in_memory().unwrap(),
            bodies: Bodies::new(dir.path().join("bodies")),
            journal: Journal::open(&dir.path().join("journal")).unwrap(),
            outbox: Outbox::open(&dir.path().join("outbox")).unwrap(),
            drafts: Drafts::open(&dir.path().join("drafts")).unwrap(),
            schedule: Schedule::default(),
            db_path: PathBuf::new(),
            dir,
        }
    }

    /// A store whose cache is a file, for code that opens it a second time
    /// (`GraphMap`).
    pub fn on_file() -> Self {
        let mut store = Self::new();
        store.db_path = store.dir.path().join("envio.db");
        store.db = Db::open(&store.db_path).unwrap();
        store
    }

    pub fn local(&mut self) -> Local<'_> {
        Local {
            db: &self.db,
            bodies: &self.bodies,
            journal: &mut self.journal,
            outbox: &self.outbox,
            schedule: &mut self.schedule,
        }
    }

    /// `local()` plus the drafts, for the calls that take both.
    pub fn with_drafts(&mut self) -> (Local<'_>, &Drafts) {
        let local = Local {
            db: &self.db,
            bodies: &self.bodies,
            journal: &mut self.journal,
            outbox: &self.outbox,
            schedule: &mut self.schedule,
        };
        (local, &self.drafts)
    }

    pub fn folder(&self, path: &str) -> Folder {
        self.db
            .folder_by_path(ACCOUNT, path)
            .unwrap()
            .expect("folder is cached")
    }

    /// The cached subjects in `path`, newest first.
    pub fn subjects(&self, path: &str) -> Vec<String> {
        self.db
            .list(ListRef::Folder(self.folder(path).id), None, 10_000)
            .unwrap()
            .into_iter()
            .map(|message| message.envelope.subject)
            .collect()
    }

    pub fn uids(&self, path: &str) -> Vec<u32> {
        self.db.uids(self.folder(path).id).unwrap()
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}
