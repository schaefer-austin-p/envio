//! The worker: one thread, one current-thread runtime, one command loop.

mod act;
mod compose;
mod conn;
mod dispatch;
mod older;
mod outbox;
mod plan;
mod read;
mod remove;
mod run;
mod search;
mod setup;
mod start;
mod store;
#[cfg(test)]
mod testing;
mod trust;
mod turn;
mod userdata;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use futures::stream::FuturesUnordered;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::api::command::{Command, ComposeId, ImagesChoice, StartFrom};
use crate::api::event::Event;
use crate::api::view::{AccountInfo, CacheState};
use crate::clock::now_ms;
use crate::core::{Config, EventSink};
use crate::drafts::DraftBook;
use crate::health::AccountStatus;
use crate::setup_thread::{SetupDone, SetupThread};
use crate::text;
use crate::undo::UndoBook;
use conn::{IdleFuture, Live};
use dispatch::{Flow, handle};
use envio_net::Roots;
use envio_store::{FolderId, FolderRole, ListRef, MessageRowId};
use envio_sync::schedule::backoff_ms;
use store::Store;
use trust::with_host;

pub(crate) struct Worker<'s> {
    store: Store,
    sink: &'s dyn EventSink,
    roots: Roots,
    foreground: bool,
    live: BTreeMap<String, Live>,
    idles: FuturesUnordered<IdleFuture>,
    /// Downloads are stopped until then (storage was full).
    disk_full_until: Option<i64>,
    /// Outbox items the user must decide on (filled by `file_sent`).
    /// Session only: after a restart Sent is searched again first.
    asked: BTreeSet<String>,
    /// Outbox items sent but not yet in Sent.
    unfiled: BTreeSet<String>,
    /// The schedule entry already acted on.
    handled_due_ms: i64,
    /// The account that ran last, for round robin.
    last_ran: Option<String>,
    /// Servers found through DNS (SRV) by `Detect`: connecting to them
    /// needs the user's confirmation whatever the app says.
    srv_hosts: BTreeSet<(String, u16)>,
    /// Open requests waiting for a body: images, width.
    opening: BTreeMap<MessageRowId, (ImagesChoice, u32)>,
    /// Actions the user can still take back.
    undo: UndoBook,
    /// Drafts: saved by the draft thread; the worker only flushes a draft
    /// before it is sent and deletes it afterwards.
    book: Arc<DraftBook>,
    /// Replies and forwards waiting for a body.
    composing: BTreeMap<MessageRowId, Vec<(ComposeId, StartFrom)>>,
    /// The thread that does the network half of account setup.
    setup: Arc<SetupThread>,
    /// The newest `Detect` ticket by address: an older answer is dropped.
    detecting: BTreeMap<String, u64>,
    /// The newest `Connect` ticket by address.
    probing: BTreeMap<String, u64>,
    /// The "Search on server" the user waits for (the newest one).
    server_search: Option<search::ServerSearch>,
    /// The Inbox unread count each account's tab last showed.
    unread_sent: RefCell<BTreeMap<String, u32>>,
    /// The lowest UID "Load older" reached in each folder, this session
    /// only: a sync keeps the rows from there up to the window.
    floors: BTreeMap<FolderId, u32>,
    /// The `LoadOlder` commands waiting for their folders.
    loads: HashMap<ListRef, older::Load>,
}

/// A command and the time (ms) its sender gave it to the core.
pub(crate) type Stamped = (Command, i64);

/// What the worker's queue carries: the app's commands, and the answers of
/// the setup thread.
// A command is by far the common input; boxing it would allocate for each one.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Input {
    Command(Stamped),
    Setup(SetupDone),
}

/// Returns on `Shutdown` or when the handle is gone.
pub(crate) fn run(
    config: &Config,
    commands: &mut UnboundedReceiver<Input>,
    sink: &dyn EventSink,
    (book, setup): (&Arc<DraftBook>, &Arc<SetupThread>),
) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    runtime.block_on(async {
        let Some(mut worker) = Worker::open(config, commands, sink, (book, setup)).await else {
            return;
        };
        while worker.turn(commands).await == Flow::Continue {}
    });
}

impl<'s> Worker<'s> {
    /// Opens the store, retrying with backoff while it can't; `None` only
    /// when told to shut down meanwhile.
    async fn open(
        config: &Config,
        commands: &mut UnboundedReceiver<Input>,
        sink: &'s dyn EventSink,
        (book, setup): (&Arc<DraftBook>, &Arc<SetupThread>),
    ) -> Option<Worker<'s>> {
        let mut failures: u32 = 0;
        let mut queued: VecDeque<Input> = VecDeque::new();
        let (store, cache) = loop {
            match Store::open(config, book, now_ms()) {
                Ok(opened) => break opened,
                Err(error) => {
                    failures = failures.saturating_add(1);
                    log::error!("opening the store: {error}");
                    if failures == 1 {
                        // Once per open: it keeps trying, drafts are safe.
                        sink.emit(Event::StoreProblem {
                            text: text::notice::store_problem(),
                        });
                    }
                }
            }
            let wait = backoff_ms(failures, u64::from(failures));
            let wait = Duration::from_millis(u64::try_from(wait).unwrap_or(0));
            tokio::select! {
                () = tokio::time::sleep(wait) => {}
                command = commands.recv() => match command {
                    None => return None,
                    Some(command) => {
                        if matches!(&command, Input::Command((Command::Shutdown, _))) {
                            if !queued.is_empty() {
                                let names: Vec<&str> = queued
                                    .iter()
                                    .map(|input| match input {
                                        Input::Command((c, _)) => c.name(),
                                        Input::Setup(_) => "setup result",
                                    })
                                    .collect();
                                log::warn!(
                                    "shutdown with {} unhandled commands: {}",
                                    names.len(),
                                    names.join(", ")
                                );
                            }
                            return None;
                        }
                        queued.push_back(command);
                    }
                },
            }
        };
        let live = store
            .accounts
            .list()
            .iter()
            .map(|a| (a.key.clone(), Live::for_account(a)))
            .collect();
        let mut worker = Worker {
            store,
            sink,
            roots: config.roots.clone(),
            foreground: config.foreground,
            live,
            idles: FuturesUnordered::new(),
            disk_full_until: None,
            asked: BTreeSet::new(),
            unfiled: BTreeSet::new(),
            handled_due_ms: i64::MIN,
            last_ran: None,
            srv_hosts: BTreeSet::new(),
            opening: BTreeMap::new(),
            undo: UndoBook::default(),
            book: Arc::clone(book),
            composing: BTreeMap::new(),
            setup: Arc::clone(setup),
            detecting: BTreeMap::new(),
            probing: BTreeMap::new(),
            server_search: None,
            unread_sent: RefCell::new(BTreeMap::new()),
            floors: BTreeMap::new(),
            loads: HashMap::new(),
        };
        worker.announce(cache);
        // What was sent while the store could not open, in order.
        for input in queued {
            if worker.input(input).await == Flow::Stop {
                return None;
            }
        }
        Some(worker)
    }

    fn announce(&self, cache: CacheState) {
        if cache == CacheState::Rebuilt {
            self.sink.emit(Event::Notice {
                text: text::notice::cache_rebuilt(),
            });
        }
        let skipped = self.store.journal.skipped().len();
        if skipped > 0 {
            self.sink.emit(Event::Notice {
                text: text::notice::journal_skipped(u64::try_from(skipped).unwrap_or(u64::MAX)),
            });
        }
        let accounts = self
            .store
            .accounts
            .list()
            .iter()
            .map(|a| AccountInfo {
                unread: self.seed_unread(&a.key),
                key: a.key.clone(),
                name: a.settings.name.clone(),
                email: a.settings.email.clone(),
                status: a
                    .broken
                    .clone()
                    .map_or(AccountStatus::Ready, AccountStatus::Broken),
                broken: a.broken.is_some(),
            })
            .collect();
        self.sink.emit(Event::Started { accounts, cache });
        self.sink.emit(Event::Settings(self.store.settings.clone()));
    }

    /// One loop iteration: run a step if an account has one, else wait for
    /// a command, an IDLE to end or the next timer.
    pub(crate) async fn turn(&mut self, commands: &mut UnboundedReceiver<Input>) -> Flow {
        // Commands that came in during the last step go first: an Undo
        // pressed inside its window must not find the offer closed.
        if self.drain(commands).await == Flow::Stop {
            return Flow::Stop;
        }
        let now = now_ms();
        self.on_timers(now);
        if let Some((account, step)) = self.pick(now).await {
            run::run_one(self, &account, step).await;
            self.last_ran = Some(account);
            return self.drain(commands).await;
        }
        let wait = self
            .wake_at(now)
            .map(|at| Duration::from_millis(u64::try_from(at - now).unwrap_or(0)));
        let sleep = async {
            match wait {
                Some(wait) => tokio::time::sleep(wait).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            input = commands.recv() => match input {
                Some(input) => match self.input(input).await {
                    Flow::Continue => self.drain(commands).await,
                    Flow::Stop => Flow::Stop,
                },
                None => Flow::Stop,
            },
            Some((account, result)) = self.idles.next(), if !self.idles.is_empty() => {
                self.idle_done(&account, result);
                Flow::Continue
            }
            () = sleep => Flow::Continue,
        }
    }

    pub(crate) fn emit(&self, event: Event) {
        let may_count = matches!(
            event,
            Event::MessagesChanged { .. } | Event::FoldersChanged { .. }
        );
        self.sink.emit(event);
        if may_count {
            self.refresh_unread();
        }
    }

    pub(crate) fn status_of(&self, account: &str) -> AccountStatus {
        self.live
            .get(account)
            .map_or(AccountStatus::Ready, |live| live.health.status.clone())
    }

    /// Tells the app if the account's status is not `before`.
    pub(crate) fn report(&self, account: &str, before: &AccountStatus) {
        let now = self.status_of(account);
        if now != *before {
            let status = match self.store.accounts.get(account) {
                Some(a) => with_host(now, &a.settings),
                None => now,
            };
            self.sink.emit(Event::AccountStatus {
                account: account.to_owned(),
                text: text::status::status(&status),
                status,
            });
        }
    }

    /// The account's Inbox in the cache.
    pub(crate) fn inbox_of(&self, account: &str) -> Option<FolderId> {
        let folders = self.store.db.folders(account).ok()?;
        folders
            .into_iter()
            .find(|f| f.role == FolderRole::Inbox)
            .map(|f| f.id)
    }

    /// Handles the inputs already queued.
    async fn drain(&mut self, commands: &mut UnboundedReceiver<Input>) -> Flow {
        while let Ok(input) = commands.try_recv() {
            if self.input(input).await == Flow::Stop {
                return Flow::Stop;
            }
        }
        Flow::Continue
    }

    async fn input(&mut self, input: Input) -> Flow {
        match input {
            Input::Command((command, at_ms)) => handle(self, command, at_ms).await,
            Input::Setup(done) => {
                self.setup_done(done);
                Flow::Continue
            }
        }
    }
}
