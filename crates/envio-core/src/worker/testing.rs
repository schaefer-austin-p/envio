//! A worker over a temp dir, for unit tests of what needs no server.

use std::path::Path;
use std::sync::Mutex;

use envio_store::{
    AccountSettings, Accounts, Db, Envelope, Flags, FolderId, FolderRole, MessageRowId, NewMessage,
    Security, Server, StorePaths, SyncState,
};
use tokio::sync::mpsc::unbounded_channel;

use super::Worker;
use crate::api::event::Event;
use crate::core::{Config, EventSink};

#[derive(Default)]
pub(super) struct Sink(pub Mutex<Vec<Event>>);

impl EventSink for Sink {
    fn emit(&self, event: Event) {
        self.0.lock().unwrap().push(event);
    }
}

impl Sink {
    pub fn events(&self) -> Vec<Event> {
        self.0.lock().unwrap().clone()
    }
}

pub(super) struct Seeded {
    pub account: String,
    pub folder: FolderId,
    pub message: MessageRowId,
}

/// One account with an Inbox holding one message.
pub(super) fn seed(dir: &Path) -> (Config, Seeded) {
    let config = Config {
        cache_dir: dir.join("cache"),
        data_dir: dir.join("data"),
        roots: envio_net::Roots::empty(),
        log_level: log::LevelFilter::Info,
        foreground: false,
    };
    let paths = StorePaths::new(&config.cache_dir, &config.data_dir);
    std::fs::create_dir_all(&config.cache_dir).unwrap();
    let server = || Server {
        host: "127.0.0.1".into(),
        port: 1,
        security: Security::Tls,
        pin: None,
    };
    let settings = AccountSettings {
        name: "Test".into(),
        email: "me@example.org".into(),
        username: "me@example.org".into(),
        imap: server(),
        smtp: server(),
    };
    let account = Accounts::open(&paths.accounts())
        .unwrap()
        .add(settings, 1_000)
        .unwrap();
    let db = Db::open(&paths.db()).unwrap();
    let folder = db
        .upsert_folder(&account, "INBOX", FolderRole::Inbox)
        .unwrap();
    let state = SyncState {
        uidvalidity: 1,
        uidnext: 2,
        highestmodseq: None,
    };
    db.set_sync_state(folder, &state).unwrap();
    let new = NewMessage {
        uid: 1,
        size: 10,
        envelope: Envelope {
            subject: "hi".into(),
            ..Envelope::default()
        },
        flags: Flags::default(),
        preview: String::new(),
    };
    let message = db.upsert_message(folder, &new).unwrap();
    (
        config,
        Seeded {
            account,
            folder,
            message,
        },
    )
}

pub(super) fn open<'s>(config: &Config, sink: &'s Sink) -> Worker<'s> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let (_keep, mut commands) = unbounded_channel();
    runtime
        .block_on(Worker::open(config, &mut commands, sink))
        .unwrap()
}
