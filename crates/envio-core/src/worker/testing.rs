//! A worker over a temp dir, for unit tests of what needs no server.

use std::path::Path;
use std::sync::{Arc, Mutex};

use envio_store::{
    AccountSettings, Accounts, Db, Envelope, Flags, FolderId, FolderRole, MessageRowId, NewMessage,
    Security, Server, StorePaths, SyncState,
};
use tokio::sync::mpsc::unbounded_channel;

use super::Worker;
use crate::api::event::Event;
use crate::core::{Config, EventSink};
use crate::drafts::DraftBook;
use crate::setup_thread::SetupThread;

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
        oauth: Vec::new(),
        dns: None,
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
        backend: envio_store::BackendKind::Imap,
        imap: Some(server()),
        smtp: Some(server()),
        auth: None,
        migration: None,
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

/// Another account with an Inbox holding `unread` unread messages. The
/// worker's unread counts are not refreshed.
pub(super) fn add_account(worker: &mut Worker<'_>, name: &str, unread: u32) -> String {
    let server = || Server {
        host: "127.0.0.1".into(),
        port: 1,
        security: Security::Tls,
        pin: None,
    };
    let settings = AccountSettings {
        name: name.into(),
        email: format!("{name}@example.org"),
        username: format!("{name}@example.org"),
        backend: envio_store::BackendKind::Imap,
        imap: Some(server()),
        smtp: Some(server()),
        auth: None,
        migration: None,
    };
    let account = worker.store.accounts.add(settings, 2_000).unwrap();
    let db = &worker.store.db;
    let folder = db
        .upsert_folder(&account, "INBOX", FolderRole::Inbox)
        .unwrap();
    for uid in 1..=unread {
        let new = NewMessage {
            uid,
            size: 10,
            envelope: Envelope {
                subject: format!("m{uid}"),
                ..Envelope::default()
            },
            flags: Flags::default(),
            preview: String::new(),
        };
        db.upsert_message(folder, &new).unwrap();
    }
    account
}

pub(super) fn open<'s>(config: &Config, sink: &'s Sink) -> Worker<'s> {
    open_with(config, sink, &Arc::default())
}

/// Like `open`, with the platform slot a restarted worker would share.
pub(super) fn open_with<'s>(
    config: &Config,
    sink: &'s Sink,
    poster: &Arc<crate::alert::PosterSlot>,
) -> Worker<'s> {
    let paths = StorePaths::new(&config.cache_dir, &config.data_dir);
    let book = Arc::new(DraftBook::new(&paths.drafts()));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let (keep, mut commands) = unbounded_channel();
    let setup = Arc::new(SetupThread::start(config.roots.clone(), keep).unwrap());
    let mut worker = runtime
        .block_on(Worker::open(
            config,
            &mut commands,
            sink,
            (&book, &setup, poster),
        ))
        .unwrap();
    // No server to ask: tests plan without trying IDLE.
    for live in worker.live.values_mut() {
        live.idle_capable = Some(false);
    }
    worker
}

/// Turns an account of `worker` into a Graph account: no IMAP or SMTP
/// server, a Graph refresh token, and a token endpoint that always answers
/// with the access token `acc`. The mailbox rows stay as they were.
pub(super) fn into_graph(worker: &mut Worker<'_>, key: &str) {
    use std::rc::Rc;

    use envio_net::oauth::{Provider, ProviderKind, Tokens};
    use futures::FutureExt;
    use secrecy::SecretString;

    let mut settings = worker.store.accounts.get(key).unwrap().settings.clone();
    settings.backend = envio_store::BackendKind::Graph;
    settings.imap = None;
    settings.smtp = None;
    settings.auth = Some("microsoft".into());
    worker.store.accounts.update(key, settings).unwrap();
    worker
        .store
        .tokens
        .borrow_mut()
        .set(key, ProviderKind::MicrosoftGraph, SecretString::from("rt"))
        .unwrap();
    let refresh: super::token::Refresher = Rc::new(|_, _| {
        async {
            Ok(Tokens {
                access: SecretString::from("acc"),
                refresh: None,
                expires_in: 3600,
                identity: None,
            })
        }
        .boxed_local()
    });
    worker.store.token_cells = super::token::TokenCells::with(
        vec![Provider::microsoft_graph_template()],
        refresh,
        Rc::new(crate::clock::now_ms),
        Rc::clone(&worker.store.tokens),
    );
    worker.live.get_mut(key).unwrap().idle_capable = Some(false);
}
