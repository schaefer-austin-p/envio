//! A whole store in a temporary folder, as core's worker would hold it.

use envio_store::{Bodies, Db, Drafts, Folder, Journal, Outbox, Page};
use envio_sync::Schedule;
use envio_sync::driver::Local;
use tempfile::TempDir;

pub const ACCOUNT: &str = "1700";

pub struct Store {
    pub dir: TempDir,
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
            dir,
        }
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

    pub fn folder(&self, path: &str) -> Folder {
        self.db
            .folder_by_path(ACCOUNT, path)
            .unwrap()
            .expect("folder is cached")
    }

    /// The cached subjects in `path`, newest first.
    pub fn subjects(&self, path: &str) -> Vec<String> {
        let page = Page {
            offset: 0,
            limit: 10_000,
        };
        self.db
            .messages(self.folder(path).id, page)
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
