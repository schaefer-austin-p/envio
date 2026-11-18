//! Shared test harness: a Core over a temp dir with its events in a channel.
#![allow(dead_code)]

pub mod fake;

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use envio_core::secrets::Secrets;
use envio_core::{Command, Config, Core, Event, EventSink};
use envio_store::{
    AccountSettings, Accounts, Bodies, BodyState, Db, Envelope, Flags, FolderId, FolderRole,
    NewMessage, Security, Server, StorePaths, SyncState,
};
use secrecy::SecretString;
use tempfile::TempDir;

pub struct ChannelSink(pub Mutex<Sender<Event>>);

impl EventSink for ChannelSink {
    fn emit(&self, event: Event) {
        let _ = self.0.lock().unwrap().send(event);
    }
}

pub struct Harness {
    pub dir: TempDir,
    pub core: Option<Core>,
    pub events: Receiver<Event>,
    /// `Config.foreground` for the next `start_core`.
    pub foreground: bool,
    /// `Config.roots` for the next `start_core`: the bundled ones, so a
    /// self-signed test server is untrusted, not "no roots".
    pub roots: envio_net::Roots,
    sender: Sender<Event>,
}

impl Harness {
    /// A temp dir with nothing running yet: seed files, then `start_core`.
    pub fn unstarted() -> Self {
        let (sender, events) = channel();
        Self {
            dir: tempfile::tempdir().unwrap(),
            core: None,
            events,
            foreground: false,
            roots: envio_net::Roots::bundled(),
            sender,
        }
    }

    pub fn start() -> Self {
        let mut harness = Self::unstarted();
        harness.start_core();
        harness
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.dir.path().join("cache")
    }

    pub fn data_dir(&self) -> PathBuf {
        self.dir.path().join("data")
    }

    pub fn paths(&self) -> StorePaths {
        StorePaths::new(self.cache_dir(), self.data_dir())
    }

    pub fn config(&self) -> Config {
        Config {
            cache_dir: self.cache_dir(),
            data_dir: self.data_dir(),
            roots: self.roots.clone(),
            log_level: log::LevelFilter::Info,
            foreground: self.foreground,
        }
    }

    pub fn start_core(&mut self) {
        let sink = Arc::new(ChannelSink(Mutex::new(self.sender.clone())));
        self.core = Some(Core::start(self.config(), sink).unwrap());
    }

    /// Stops the core (joining its thread) and starts a new one.
    pub fn restart(&mut self) {
        drop(self.core.take());
        self.start_core();
    }

    pub fn send(&self, command: Command) {
        self.core.as_ref().unwrap().send(command);
    }

    /// The first event `pick` accepts, within 5 s; panics with what it saw.
    pub fn wait<T>(&self, pick: impl Fn(&Event) -> Option<T>) -> T {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut seen = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(left) {
                Ok(event) => {
                    if let Some(found) = pick(&event) {
                        return found;
                    }
                    seen.push(event);
                }
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {
                    panic!("no matching event within 5 s; saw {seen:?}")
                }
            }
        }
    }

    /// Writes the account (servers at 127.0.0.1:1) and its password; call
    /// before `start_core`. Returns the account key.
    pub fn seed_account(&self, email: &str, password: Option<&str>) -> String {
        self.seed_account_at(email, password, 1)
    }

    /// Like `seed_account`, with the servers' port chosen.
    pub fn seed_account_at(&self, email: &str, password: Option<&str>, port: u16) -> String {
        self.seed_account_on(email, password, ("127.0.0.1", port))
    }

    /// Like `seed_account_at`, with the servers' host chosen too.
    pub fn seed_account_on(
        &self,
        email: &str,
        password: Option<&str>,
        (host, port): (&str, u16),
    ) -> String {
        let server = || Server {
            host: host.into(),
            port,
            security: Security::Tls,
            pin: None,
        };
        let settings = AccountSettings {
            name: "Test".into(),
            email: email.into(),
            username: email.into(),
            imap: server(),
            smtp: server(),
        };
        let mut accounts = Accounts::open(&self.paths().accounts()).unwrap();
        let key = accounts.add(settings, 1_000).unwrap();
        if let Some(password) = password {
            let mut secrets = Secrets::open(&self.data_dir()).unwrap();
            secrets
                .set(&key, SecretString::from(password.to_owned()))
                .unwrap();
        }
        key
    }

    /// Sets the IMAP pin of an account in the accounts file (before start).
    pub fn pin_imap(&self, key: &str, pin: &str) {
        let path = self.paths().accounts();
        let text = std::fs::read_to_string(&path).unwrap();
        let mut json: serde_json::Value = serde_json::from_str(&text).unwrap();
        for entry in json["accounts"].as_array_mut().unwrap() {
            if entry["key"] == key {
                entry["settings"]["imap"]["pin"] = pin.into();
            }
        }
        std::fs::write(&path, json.to_string()).unwrap();
    }

    /// A folder with messages (uid, eml) and their bodies, in the cache.
    #[allow(clippy::too_many_arguments)] // the signature the plan fixes
    pub fn seed_folder_with(
        &self,
        account: &str,
        path: &str,
        role: FolderRole,
        messages: &[(u32, &[u8])],
    ) -> FolderId {
        let paths = self.paths();
        std::fs::create_dir_all(self.cache_dir()).unwrap();
        let db = Db::open(&paths.db()).unwrap();
        let bodies = Bodies::new(paths.bodies());
        let folder = db.upsert_folder(account, path, role).unwrap();
        let uidnext = messages.iter().map(|m| m.0).max().unwrap_or(0) + 1;
        let state = SyncState {
            uidvalidity: 1,
            uidnext,
            highestmodseq: None,
        };
        db.set_sync_state(folder, &state).unwrap();
        for (uid, eml) in messages {
            let message = NewMessage {
                uid: *uid,
                size: u32::try_from(eml.len()).unwrap(),
                envelope: Envelope {
                    subject: format!("message {uid}"),
                    from: "Ada <ada@example.org>".into(),
                    to: "me@example.org".into(),
                    date: now_secs(),
                    ..Envelope::default()
                },
                flags: Flags::default(),
                preview: String::new(),
            };
            let id = db.upsert_message(folder, &message).unwrap();
            db.set_body_state(id, BodyState::Downloaded).unwrap();
            bodies.write(folder, *uid, eml).unwrap();
            // As the worker does when a body is downloaded.
            db.index_body(id, &envio_mime::quote_text(eml)).unwrap();
        }
        folder
    }
}

pub fn now_ms() -> i64 {
    now_secs() * 1000
        + i64::from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_millis(),
        )
}

pub fn now_secs() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
}
