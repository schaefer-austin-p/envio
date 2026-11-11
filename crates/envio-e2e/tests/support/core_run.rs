//! A `Core` over a temporary folder with its events in a channel, set up
//! for tests against Stalwart. (Not `core.rs`: that would shadow `core`.)

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use envio_core::{
    AccountSetup, Command, Config, ConnectRequest, Core, Event, EventSink, FolderId, FolderRole,
    MessageRow, Page, ServerAddr, Service,
};
use envio_net::{Roots, Security};
use envio_store::StorePaths;
use log::LevelFilter;
use secrecy::SecretString;
use tempfile::TempDir;

use super::stalwart::Stalwart;

/// What a network step may take.
pub const NET: Duration = Duration::from_secs(30);

struct ChannelSink(Mutex<Sender<Event>>);

impl EventSink for ChannelSink {
    fn emit(&self, event: Event) {
        let _ = self.0.lock().unwrap().send(event);
    }
}

pub struct CoreRun {
    pub dir: TempDir,
    pub core: Core,
    pub events: Receiver<Event>,
    level: LevelFilter,
}

fn begin(dir: &TempDir, level: LevelFilter) -> (Core, Receiver<Event>) {
    let (sender, events) = channel();
    let config = Config {
        cache_dir: dir.path().join("cache"),
        data_dir: dir.path().join("data"),
        roots: Roots::empty(),
        log_level: level,
        foreground: true,
    };
    let sink = Arc::new(ChannelSink(Mutex::new(sender)));
    (Core::start(config, sink).unwrap(), events)
}

impl CoreRun {
    pub fn start(level: LevelFilter) -> CoreRun {
        let dir = tempfile::tempdir().unwrap();
        let (core, events) = begin(&dir, level);
        CoreRun {
            dir,
            core,
            events,
            level,
        }
    }

    /// Stops the core (it flushes and joins) and starts a new one on the
    /// same folder.
    pub fn restart(self) -> CoreRun {
        self.restart_with(|_| {})
    }

    /// `restart`, with `between` run while no core is running.
    pub fn restart_with(self, between: impl FnOnce(&StorePaths)) -> CoreRun {
        let paths = self.paths();
        let CoreRun {
            dir, core, level, ..
        } = self;
        drop(core);
        between(&paths);
        let (core, events) = begin(&dir, level);
        CoreRun {
            dir,
            core,
            events,
            level,
        }
    }

    pub fn paths(&self) -> StorePaths {
        StorePaths::new(self.dir.path().join("cache"), self.dir.path().join("data"))
    }

    /// The next event `pick` takes, skipping the others; panics after
    /// `timeout`.
    pub fn wait<T>(&self, timeout: Duration, pick: impl Fn(&Event) -> Option<T>) -> T {
        let end = Instant::now() + timeout;
        let mut seen = Vec::new();
        loop {
            let left = end.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(left) {
                Ok(event) => {
                    if let Some(found) = pick(&event) {
                        return found;
                    }
                    seen.push(event);
                }
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {
                    panic!("no matching event within {timeout:?}; saw {seen:?}")
                }
            }
        }
    }

    /// True if an event `pick` takes arrives within `window`.
    pub fn happens(&self, window: Duration, pick: impl Fn(&Event) -> bool) -> bool {
        let end = Instant::now() + window;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(left) {
                Ok(event) if pick(&event) => return true,
                Ok(_) => {}
                Err(_) => return false,
            }
        }
    }

    /// Adds the account the way the wizard does: connect, accept the
    /// certificate of each service as it is asked about. Returns its key.
    pub fn add_account(&self, server: &Stalwart, user: &str) -> String {
        self.add_account_with(user, &setup(server, user))
    }

    pub fn add_account_with(&self, user: &str, setup: &AccountSetup) -> String {
        let mut accepted: Vec<(Service, String)> = Vec::new();
        loop {
            self.core
                .send(connect(setup.clone(), user, accepted.clone()));
            let next = self.wait(NET, |event| match event {
                Event::TrustPrompt { service, hex, .. } => Some(Some((*service, hex.clone()))),
                Event::AccountAdded { .. } => Some(None),
                Event::SetupProblem { text, .. } => panic!("setup problem: {text}"),
                _ => None,
            });
            match next {
                Some(prompt) => accepted.push(prompt),
                None => break,
            }
        }
        self.account_key(user)
    }

    pub fn account_key(&self, user: &str) -> String {
        let accounts = envio_store::Accounts::open(&self.paths().accounts()).unwrap();
        accounts
            .list()
            .iter()
            .find(|a| a.settings.username == user)
            .map(|a| a.key.clone())
            .unwrap()
    }

    /// The account's folder with `role`, once the first sync has listed it.
    pub fn folder(&self, account: &str, role: FolderRole) -> (FolderId, String) {
        let end = Instant::now() + NET;
        loop {
            self.core.send(Command::ListFolders {
                account: account.to_owned(),
            });
            let folders = self.wait(NET, |event| match event {
                Event::Folders { folders, .. } => Some(folders.clone()),
                _ => None,
            });
            if let Some(row) = folders.iter().find(|f| f.role == role) {
                return (row.id, row.path.clone());
            }
            assert!(Instant::now() < end, "no {role:?} folder");
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    pub fn rows(&self, folder: FolderId) -> Vec<MessageRow> {
        let page = Page {
            offset: 0,
            limit: 200,
        };
        self.core.send(Command::ListMessages { folder, page });
        self.wait(NET, |event| match event {
            Event::Messages {
                folder: f, rows, ..
            } if *f == folder => Some(rows.clone()),
            _ => None,
        })
    }

    /// The folder's row with `subject`, once the core shows it.
    pub fn row_with(&self, folder: FolderId, subject: &str) -> MessageRow {
        self.row_when(folder, subject, true).unwrap()
    }

    /// Polls until the row with `subject` is there (`present`) or gone.
    pub fn row_when(&self, folder: FolderId, subject: &str, present: bool) -> Option<MessageRow> {
        let end = Instant::now() + NET;
        loop {
            let found = self
                .rows(folder)
                .into_iter()
                .find(|row| row.subject == subject);
            if found.is_some() == present {
                return found;
            }
            assert!(
                Instant::now() < end,
                "row {subject:?} never became present={present}; events since: {:?}",
                self.events.try_iter().collect::<Vec<_>>()
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

pub fn connect(setup: AccountSetup, user: &str, accepted: Vec<(Service, String)>) -> Command {
    Command::Connect(Box::new(ConnectRequest {
        setup,
        password: SecretString::from(password(user)),
        servers_confirmed: false,
        accepted,
    }))
}

pub fn password(user: &str) -> String {
    match user {
        "ivan" => "pässwort-ivan-1".to_owned(),
        other => format!("{other}-secret-1"),
    }
}

/// IMAPS and submissions of the server, for `user` (the name before `@`).
pub fn setup(server: &Stalwart, user: &str) -> AccountSetup {
    with_imap(server, user, &server.host, server.imaps)
}

/// Like `setup`, with IMAPS reached through a local `Proxy` on `port`.
/// The proxy listens on 127.0.0.1, which is not the server's host in CI
/// (Stalwart runs in the dind daemon there).
pub fn setup_at(server: &Stalwart, user: &str, port: u16) -> AccountSetup {
    with_imap(server, user, "127.0.0.1", port)
}

fn with_imap(server: &Stalwart, user: &str, imap_host: &str, imaps: u16) -> AccountSetup {
    let at = |host: &str, port| ServerAddr {
        host: host.to_owned(),
        port,
        security: Security::Tls,
    };
    AccountSetup {
        name: user.to_owned(),
        email: format!("{user}@envio.test"),
        username: user.to_owned(),
        imap: at(imap_host, imaps),
        smtp: at(&server.host, server.submissions),
        source: None,
        needs_confirm: false,
    }
}
