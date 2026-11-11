//! Helpers for the cross-crate tests. Stalwart's helpers are envio-net's
//! own, included by path so both crates start and find it the same way.
#![allow(dead_code)]

pub mod core_run;
pub mod proxy;

#[path = "../../../envio-net/tests/support/stalwart.rs"]
pub mod stalwart;

use std::time::{SystemTime, UNIX_EPOCH};

use envio_net::Credentials;
use envio_store::{Bodies, Db, Folder, FolderRole, Journal, Outbox, Page};
use envio_sync::Schedule;
use envio_sync::driver::Local;
use tempfile::TempDir;

/// One Stalwart user per test (up.sh creates them), so tests run in
/// parallel without seeing each other's mail.
pub struct User {
    pub name: &'static str,
    pub address: String,
}

impl User {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            address: format!("{name}@envio.test"),
        }
    }

    pub fn credentials(&self) -> Credentials {
        Credentials::new(self.name, &format!("{}-secret-1", self.name))
    }
}

pub fn now_ms() -> i64 {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    i64::try_from(since.as_millis()).unwrap()
}

/// A store in a temporary folder, as core's worker holds it.
pub struct Store {
    pub dir: TempDir,
    pub db: Db,
    pub bodies: Bodies,
    pub journal: Journal,
    pub outbox: Outbox,
    pub schedule: Schedule,
    pub account: String,
}

impl Store {
    pub fn new(account: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        Self {
            db: Db::open_in_memory().unwrap(),
            bodies: Bodies::new(dir.path().join("bodies")),
            journal: Journal::open(&dir.path().join("journal")).unwrap(),
            outbox: Outbox::open(&dir.path().join("outbox")).unwrap(),
            schedule: Schedule::default(),
            account: account.to_owned(),
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

    pub fn folder(&self, role: FolderRole) -> Folder {
        self.db
            .folders(&self.account)
            .unwrap()
            .into_iter()
            .find(|folder| folder.role == role)
            .unwrap_or_else(|| panic!("no {role:?} folder"))
    }

    /// The cached subjects in the folder, newest first.
    pub fn subjects(&self, role: FolderRole) -> Vec<String> {
        let page = Page {
            offset: 0,
            limit: 1_000,
        };
        self.db
            .messages(self.folder(role).id, page)
            .unwrap()
            .into_iter()
            .map(|message| message.envelope.subject)
            .collect()
    }
}
