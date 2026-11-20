//! The worker: one thread, one current-thread runtime, one command loop.

mod act;
mod act_local;
mod alerts;
mod attachment;
mod compose;
mod conn;
mod dispatch;
mod dkim_keys;
mod edit;
mod edit_keywords;
mod folder_order;
mod inline;
mod invite;
mod label;
mod local_list;
mod local_rows;
mod oauth;
mod older;
mod open;
mod outbox;
mod plan;
mod prefetch;
mod read;
mod remove;
mod rows;
mod run;
mod search;
mod sender_check;
mod sender_lookup;
mod setup;
mod shown;
mod start;
mod store;
#[cfg(test)]
mod testing;
mod token;
mod trust;
mod turn;
mod userdata;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use futures::stream::FuturesUnordered;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::alert::PosterSlot;
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
use envio_sync::plan::sync::Kept;
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
    /// Background connects and Inbox SELECTs in flight.
    opens: FuturesUnordered<open::OpenFuture>,
    /// Downloads are stopped until then (storage was full).
    disk_full_until: Option<i64>,
    /// Outbox items the user must decide on (filled by `file_sent`).
    /// Session only: after a restart Sent is searched again first.
    asked: BTreeSet<String>,
    /// Outbox items sent but not yet in Sent.
    unfiled: BTreeSet<String>,
    /// The schedule entry already acted on.
    handled_due_ms: i64,
    /// The held Outbox release time already acted on (its own watermark).
    handled_release_ms: i64,
    /// The account that ran last, for round robin.
    last_ran: Option<String>,
    /// Servers `Detect` found outside the address's domain: connecting to them
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
    /// Edits of a server draft waiting for the live keyword check of its
    /// original, with the original's bytes (see `edit_keywords`).
    awaiting_keywords: BTreeMap<MessageRowId, Vec<(ComposeId, Vec<u8>)>>,
    /// Messages whose running body step found a forward that needs the
    /// whole message (see `Worker::wait_for_whole`).
    whole_again: BTreeSet<MessageRowId>,
    /// Open messages that got a whole big message and still wait for their
    /// partial body (see `Worker::body_fetched`).
    open_again: BTreeSet<MessageRowId>,
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
    floors: BTreeMap<FolderId, Kept>,
    /// The `LoadOlder` commands waiting for their folders.
    loads: HashMap<ListRef, older::Load>,
    /// Where alerts and the badge go (shared with `Core::set_poster`).
    poster: Arc<PosterSlot>,
    /// Folders whose Load older rows a window change drops with their next
    /// sync; `OlderReset` follows that sync.
    reset_pending: BTreeSet<FolderId>,
    /// The row ids of local drafts shown as pseudo-rows in Drafts lists.
    local_ids: RefCell<local_rows::LocalIds>,
    /// The providers a user can sign in with (`Config.oauth`).
    providers: Vec<envio_net::oauth::Provider>,
    /// The sign-in that waits for the browser, if any.
    oauth: Option<oauth::Attempt>,
    /// Where DKIM keys are looked up (sender trust, #285).
    dns: Arc<dyn envio_net::dns::TxtLookup>,
    /// The worker's own queue, for the answers of lookup tasks.
    inputs: UnboundedSender<Input>,
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
    /// The draft thread saved the draft of a window that closed: its
    /// account may have something to upload.
    DraftsSaved,
    /// `Core::set_poster` ran: the poster is told the current badge, on the
    /// worker's thread, in order with later unread changes.
    PosterSet,
    /// A DKIM key lookup for the open message `id` ended (sender trust).
    DkimKeys {
        id: MessageRowId,
        answers: Vec<(String, envio_net::dns::TxtAnswer)>,
    },
}

/// Returns on `Shutdown` or when the handle is gone.
pub(crate) fn run(
    config: &Config,
    commands: &mut UnboundedReceiver<Input>,
    sink: &dyn EventSink,
    (book, setup, poster): (&Arc<DraftBook>, &Arc<SetupThread>, &Arc<PosterSlot>),
) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    runtime.block_on(async {
        let Some(mut worker) = Worker::open(config, commands, sink, (book, setup, poster)).await
        else {
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
        (book, setup, poster): (&Arc<DraftBook>, &Arc<SetupThread>, &Arc<PosterSlot>),
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
                                        Input::DraftsSaved => "drafts saved",
                                        Input::PosterSet => "poster set",
                                        Input::DkimKeys { .. } => "dkim keys",
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
            foreground: poster.foreground().unwrap_or(config.foreground),
            live,
            idles: FuturesUnordered::new(),
            opens: FuturesUnordered::new(),
            disk_full_until: None,
            asked: BTreeSet::new(),
            unfiled: BTreeSet::new(),
            handled_due_ms: i64::MIN,
            handled_release_ms: i64::MIN,
            last_ran: None,
            srv_hosts: BTreeSet::new(),
            opening: BTreeMap::new(),
            undo: UndoBook::default(),
            book: Arc::clone(book),
            composing: BTreeMap::new(),
            awaiting_keywords: BTreeMap::new(),
            whole_again: BTreeSet::new(),
            open_again: BTreeSet::new(),
            setup: Arc::clone(setup),
            detecting: BTreeMap::new(),
            probing: BTreeMap::new(),
            server_search: None,
            unread_sent: RefCell::new(BTreeMap::new()),
            floors: BTreeMap::new(),
            loads: HashMap::new(),
            poster: Arc::clone(poster),
            reset_pending: BTreeSet::new(),
            local_ids: RefCell::default(),
            providers: config.oauth.clone(),
            oauth: None,
            dns: config
                .dns
                .clone()
                .unwrap_or_else(|| Arc::new(envio_net::dns::SystemTxt::new())),
            inputs: setup.inputs(),
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
                label: a.label().unwrap_or_default().to_owned(),
                email: a.settings.email.clone(),
                status: a
                    .broken
                    .clone()
                    .map_or(AccountStatus::Ready, AccountStatus::Broken),
                broken: a.broken.is_some(),
                oauth: a.settings.is_oauth(),
            })
            .collect();
        self.sink.emit(Event::Started { accounts, cache });
        self.sink.emit(Event::Settings(self.store.settings.clone()));
        // A restarted worker re-seeds the counts, so nothing looks changed:
        // the poster (if one is set) hears the total now.
        self.poster_set();
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
            self.run_polling(&account, step).await;
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
            () = self.ended() => Flow::Continue,
            () = sleep => Flow::Continue,
        }
    }

    /// Runs a step while the connects in flight are still polled: a step
    /// can take long, and a connect's timeout must not run out unseen.
    /// Their results are handled after the step.
    async fn run_polling(&mut self, account: &str, step: plan::Step) {
        let mut opens = std::mem::take(&mut self.opens);
        let ended = {
            let run = run::run_one(self, account, step);
            open::polling(&mut opens, run).await.1
        };
        opens.extend(std::mem::take(&mut self.opens));
        self.opens = opens;
        for (account, result) in ended {
            self.open_done(&account, result);
        }
    }

    /// An IDLE or a connect ended (never, with none in flight): its result
    /// is handled here, with no await after it.
    async fn ended(&mut self) {
        tokio::select! {
            Some((account, result)) = self.idles.next(), if !self.idles.is_empty() => {
                self.idle_done(&account, result);
            }
            Some((account, result)) = self.opens.next(), if !self.opens.is_empty() => {
                self.open_done(&account, result);
            }
            else => std::future::pending().await,
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

    /// The words for `status` of `account` (an OAuth account is told to sign
    /// in again, not to update a password).
    pub(crate) fn status_text(&self, account: &str, status: &AccountStatus) -> String {
        let oauth = self
            .store
            .accounts
            .get(account)
            .is_some_and(|a| a.settings.is_oauth());
        let provider = self
            .provider_of(envio_net::oauth::ProviderKind::Microsoft)
            .is_some();
        text::status::status_with(status, oauth, provider)
    }

    /// Tells the app if the account's status is not `before`.
    pub(crate) fn report(&self, account: &str, before: &AccountStatus) {
        let now = self.status_of(account);
        self.store
            .token_cells
            .set_paused(account, conn::paused_problem(&now));
        if now != *before {
            let status = match self.store.accounts.get(account) {
                Some(a) => with_host(now, &a.settings),
                None => now,
            };
            self.sink.emit(Event::AccountStatus {
                account: account.to_owned(),
                text: self.status_text(account, &status),
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
            Input::DraftsSaved => {
                self.want_drafts_after_close();
                Flow::Continue
            }
            Input::PosterSet => {
                self.poster_set();
                Flow::Continue
            }
            Input::DkimKeys { id, answers } => {
                self.dkim_keys(id, answers);
                Flow::Continue
            }
        }
    }
}
