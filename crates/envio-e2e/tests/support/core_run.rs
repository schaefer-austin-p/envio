//! A `Core` over a temporary folder with its events in a channel, set up
//! for tests against Stalwart. (Not `core.rs`: that would shadow `core`.)

use std::cell::RefCell;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use envio_core::{
    AccountSetup, Command, Config, ConnectRequest, Core, Event, EventSink, FolderId, FolderRole,
    ListRef, MessageRow, ServerAddr, Service,
};
use envio_net::dns::TxtLookup;
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
    core: Option<Core>,
    pub events: Receiver<Event>,
    level: LevelFilter,
    dns: Option<Arc<dyn TxtLookup>>,
    /// Every event `wait` or `happens` took off `events`, matched or
    /// not, for `wait_ever`.
    history: RefCell<Vec<Event>>,
}

/// How long a core may take to stop before the test fails.
const STOP: Duration = Duration::from_secs(10);

impl Drop for CoreRun {
    fn drop(&mut self) {
        stop(self.core.take());
    }
}

/// `Core::drop` joins the worker with no bound: a stuck worker would hang
/// the test for ever. Drop it on a helper thread and give up after `STOP`
/// (#116): the test fails, unless it is already failing.
fn stop(core: Option<Core>) {
    let Some(core) = core else { return };
    let (done, stopped) = channel();
    std::thread::spawn(move || {
        drop(core);
        let _ = done.send(());
    });
    if stopped.recv_timeout(STOP).is_err() {
        let message = "core did not stop within 10 s: the worker is stuck";
        eprintln!("{message}");
        if !std::thread::panicking() {
            panic!("{message}");
        }
    }
}

fn begin(
    dir: &TempDir,
    level: LevelFilter,
    dns: Option<Arc<dyn TxtLookup>>,
) -> (Core, Receiver<Event>) {
    let (sender, events) = channel();
    let config = Config {
        cache_dir: dir.path().join("cache"),
        data_dir: dir.path().join("data"),
        roots: Roots::bundled(),
        oauth: Vec::new(),
        dns,
        log_level: level,
        foreground: true,
    };
    let sink = Arc::new(ChannelSink(Mutex::new(sender)));
    (Core::start(config, sink).unwrap(), events)
}

impl CoreRun {
    pub fn start(level: LevelFilter) -> CoreRun {
        CoreRun::start_with_dns(level, None)
    }

    /// `start`, with DKIM key lookups (#285) answered by `dns` instead of
    /// the system resolver.
    pub fn start_with_dns(level: LevelFilter, dns: Option<Arc<dyn TxtLookup>>) -> CoreRun {
        let dir = tempfile::tempdir().unwrap();
        let (core, events) = begin(&dir, level, dns.clone());
        CoreRun {
            dir,
            core: Some(core),
            events,
            level,
            dns,
            history: RefCell::default(),
        }
    }

    /// The running core.
    pub fn core(&self) -> &Core {
        self.core.as_ref().expect("the core runs until drop")
    }

    /// Stops the core (it flushes and joins) and starts a new one on the
    /// same folder.
    pub fn restart(self) -> CoreRun {
        self.restart_with(|_| {})
    }

    /// `restart`, with `between` run while no core is running.
    pub fn restart_with(mut self, between: impl FnOnce(&StorePaths)) -> CoreRun {
        let paths = self.paths();
        stop(self.core.take());
        between(&paths);
        let (core, events) = begin(&self.dir, self.level, self.dns.clone());
        self.core = Some(core);
        self.events = events;
        self.history.borrow_mut().clear();
        self
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
                    self.history.borrow_mut().push(event.clone());
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

    /// Like `wait`, but an event an earlier `wait`, `happens` or helper
    /// (`add_account`, `folder`, `rows`) already took off the channel
    /// counts too. Use it for a state change that may have happened while
    /// the test was waiting for something else, such as the account being
    /// Ready (#257).
    pub fn wait_ever<T>(&self, timeout: Duration, pick: impl Fn(&Event) -> Option<T>) -> T {
        if let Some(found) = self.history.borrow().iter().find_map(&pick) {
            return found;
        }
        self.wait(timeout, pick)
    }

    /// True if an event `pick` takes arrives within `window`.
    pub fn happens(&self, window: Duration, pick: impl Fn(&Event) -> bool) -> bool {
        let end = Instant::now() + window;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(left) {
                Ok(event) => {
                    let hit = pick(&event);
                    self.history.borrow_mut().push(event);
                    if hit {
                        return true;
                    }
                }
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
            self.core()
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
            self.core().send(Command::ListFolders {
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

    /// Every row of the folder, page after page.
    pub fn rows(&self, folder: FolderId) -> Vec<MessageRow> {
        let list = ListRef::Folder(folder);
        let mut all: Vec<MessageRow> = Vec::new();
        let mut after = None;
        loop {
            self.core().send(Command::ListMessages { list, after });
            let (rows, end) = self.wait(NET, |event| match event {
                Event::Messages {
                    list: l,
                    after: a,
                    rows,
                    end,
                } if *l == list && *a == after => Some((rows.clone(), *end)),
                _ => None,
            });
            after = rows.last().map(|row| row.cursor);
            all.extend(rows);
            if end || after.is_none() {
                return all;
            }
        }
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
