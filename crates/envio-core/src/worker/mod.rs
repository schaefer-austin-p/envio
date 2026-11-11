//! The worker: one thread, one current-thread runtime, one command loop.

mod conn;
mod dispatch;
mod plan;
mod remove;
mod run;
mod setup;
mod store;
mod turn;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Duration;

use futures::StreamExt;
use futures::stream::FuturesUnordered;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::api::command::Command;
use crate::api::event::Event;
use crate::api::view::{AccountInfo, CacheState};
use crate::clock::now_ms;
use crate::core::{Config, EventSink};
use crate::health::AccountStatus;
use crate::text;
use conn::{IdleFuture, Live};
use dispatch::{Flow, handle};
use envio_net::Roots;
use envio_store::{FolderId, FolderRole};
use envio_sync::schedule::backoff_ms;
use store::Store;

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
    #[allow(dead_code)] // read by the Outbox commands (Task 13)
    asked: BTreeSet<String>,
    /// Outbox items sent but not yet in Sent.
    #[allow(dead_code)] // read by the Outbox commands (Task 13)
    unfiled: BTreeSet<String>,
    /// The schedule entry already acted on.
    handled_due_ms: i64,
    /// The account that ran last, for round robin.
    last_ran: Option<String>,
}

/// Returns on `Shutdown` or when the handle is gone.
pub(crate) fn run(
    config: &Config,
    commands: &mut UnboundedReceiver<Command>,
    sink: &dyn EventSink,
) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    runtime.block_on(async {
        let Some(mut worker) = Worker::open(config, commands, sink).await else {
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
        commands: &mut UnboundedReceiver<Command>,
        sink: &'s dyn EventSink,
    ) -> Option<Worker<'s>> {
        let mut failures: u32 = 0;
        let mut queued: VecDeque<Command> = VecDeque::new();
        let (store, cache) = loop {
            match Store::open(config, now_ms()) {
                Ok(opened) => break opened,
                Err(error) => {
                    failures = failures.saturating_add(1);
                    log::error!("opening the store: {error}");
                }
            }
            let wait = backoff_ms(failures, u64::from(failures));
            let wait = Duration::from_millis(u64::try_from(wait).unwrap_or(0));
            tokio::select! {
                () = tokio::time::sleep(wait) => {}
                command = commands.recv() => match command {
                    None => return None,
                    Some(command) => {
                        if matches!(command, Command::Shutdown) {
                            if !queued.is_empty() {
                                let names: Vec<&str> = queued.iter().map(Command::name).collect();
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
            .map(|a| (a.key.clone(), Live::new()))
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
        };
        worker.announce(cache);
        // What was sent while the store could not open, in order.
        for command in queued {
            if handle(&mut worker, command).await == Flow::Stop {
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
        let accounts = self
            .store
            .accounts
            .list()
            .iter()
            .map(|a| AccountInfo {
                key: a.key.clone(),
                name: a.settings.name.clone(),
                email: a.settings.email.clone(),
                status: AccountStatus::Ready,
            })
            .collect();
        self.sink.emit(Event::Started { accounts, cache });
    }

    /// One loop iteration: run a step if an account has one, else wait for
    /// a command, an IDLE to end or the next timer.
    async fn turn(&mut self, commands: &mut UnboundedReceiver<Command>) -> Flow {
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
            command = commands.recv() => match command {
                Some(command) => match handle(self, command).await {
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
        self.sink.emit(event);
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
            self.sink.emit(Event::AccountStatus {
                account: account.to_owned(),
                text: text::status::status(&now),
                status: now,
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

    /// Handles the commands already queued.
    async fn drain(&mut self, commands: &mut UnboundedReceiver<Command>) -> Flow {
        while let Ok(command) = commands.try_recv() {
            if handle(self, command).await == Flow::Stop {
                return Flow::Stop;
            }
        }
        Flow::Continue
    }

    /// Saves open drafts; filled in with the draft commands (Task 12).
    fn flush_drafts(&mut self) {}
}
