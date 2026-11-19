//! The screens' state: which list is shown, what is selected, what is
//! outstanding. Events in, outputs out; UI calls in, commands out. The
//! handlers live in the submodules, one concern each.

mod accounts;
mod act;
mod folder_counts;
mod messages;
mod notify;
mod older;
mod open;
mod search;
mod selection;

use std::collections::{BTreeSet, HashMap, HashSet};

use envio_core::{Command, Event, FolderRole, ListRef, OutboxRow, Service};

use crate::convert::{self, signal_of};
use crate::lists::ShownList;
use crate::mirror::Mirror;
use crate::models::{Model, ModelChange, Op};
use crate::rows::AccountRow;
use crate::signals::{OlderRow, Signal};
use messages::{Current, First};
use search::SearchState;

/// What `Ui::apply` hands the glue, in order.
#[derive(Debug)]
pub enum Output {
    Change(ModelChange),
    Signal(Signal),
    /// A command the Ui itself wants sent (a refresh after new mail).
    Send(Command),
}

fn change(model: Model, op: Op) -> Output {
    Output::Change(ModelChange::new(model, op))
}

#[derive(Debug, Default)]
pub struct Ui {
    accounts: Vec<AccountRow>,
    /// Accounts whose notifications are off, as the settings last said.
    notify_off: BTreeSet<String>,
    /// The Outbox as the core last listed it; rows are rebuilt from it when
    /// an account's status changes their reason.
    outbox: Vec<OutboxRow>,
    names: HashMap<String, String>,
    folders_of: Option<String>,
    /// The account the open drawer shows; `None` while it is closed.
    drawer_account: Option<String>,
    /// The account a `ListFolders` is outstanding for.
    folders_pending: Option<String>,
    /// Counts changed while that list was outstanding: ask once more.
    folders_again: bool,
    /// Folder id to (account, role), from folder lists and Inbox rows.
    folder_roles: HashMap<i64, (String, FolderRole)>,
    current: Option<Current>,
    search: Option<SearchState>,
    found: ShownList,
    /// Accounts that could not answer the current server search.
    server_failed: usize,
    selection: BTreeSet<i64>,
    /// The message the reader shows (the row's `open` role).
    open: Option<i64>,
    /// The model `open` was opened from; its row leaving clears `open`.
    open_from: Option<Model>,
    /// The "Load older" row's state, as last announced.
    older: OlderRow,
    /// Load older found nothing more on the server for the shown list.
    older_done: bool,
    /// Rows Load older cached are to be paged in: asked as soon as the
    /// list can take a page.
    older_paging: bool,
    /// A reset reached the shown list while its batch loaded: that batch's
    /// "exhausted" may stem from before it, so it does not end at Done.
    older_reset_while_loading: bool,
    /// Lists whose last Load older failed or found nothing, for the session:
    /// scrolling does not load them again (no retry loop, no toast nobody
    /// asked for on reopen), the row is tapped. A batch that brings rows or
    /// an `OlderReset` clears it.
    older_auto_blocked: HashSet<ListRef>,
    mirror: Mirror,
    /// Certificates accepted for the account being set up; the next
    /// `Connect` carries them.
    accepted: Vec<(Service, String)>,
    /// The address those acceptances were made for.
    setup_email: Option<String>,
    now: i64,
    utc_offset_s: i32,
}

impl Ui {
    pub fn new() -> Ui {
        Ui::default()
    }

    /// The rows a model shows now (a model attached late starts from them).
    pub fn rows(&self, model: Model) -> &[crate::rows::Row] {
        self.mirror.rows(model)
    }

    /// A certificate the user accepted while setting an account up.
    pub fn accept_setup_certificate(&mut self, service: Service, hex: String) {
        self.accepted.push((service, hex));
    }

    /// The accepted certificates, for the next `Connect`.
    pub fn accepted(&self) -> &[(Service, String)] {
        &self.accepted
    }

    /// A new setup starts: earlier acceptances do not carry over.
    pub fn clear_accepted(&mut self) {
        self.accepted.clear();
        self.setup_email = None;
    }

    /// A setup (`Detect` or `Connect`) for `email` begins: acceptances made
    /// for another address are dropped. They hold no host, so they must
    /// not follow the user to another address's servers.
    pub fn begin_setup(&mut self, email: &str) {
        let email = email.trim().to_lowercase();
        if self.setup_email.as_deref() != Some(email.as_str()) {
            self.accepted.clear();
            self.setup_email = Some(email);
        }
    }

    pub fn apply(&mut self, event: Event, now: i64, utc_offset_s: i32) -> Vec<Output> {
        self.now = now;
        self.utc_offset_s = utc_offset_s;
        if matches!(event, Event::Notice { .. }) {
            self.folders_failed();
        }
        if let Some(signal) = signal_of(&event) {
            let mut outs = match &event {
                Event::Settings(settings) => self.notify_off_is(&settings.notify_off),
                _ => Vec::new(),
            };
            self.mirror.track(&outs);
            outs.push(Output::Signal(signal));
            return outs;
        }
        let outs = match event {
            Event::Started { accounts, .. } => self.started(&accounts),
            Event::AccountAdded { account } => self.account_added(&account),
            Event::AccountStatus {
                account,
                status,
                text,
            } => self.account_status(account, &status, text),
            Event::AccountLabel { account, label } => self.account_label(&account, &label),
            Event::AccountRemoved { account } => self.account_removed(&account),
            Event::AccountUnread { account, unread } => {
                let mut outs = self.account_unread(&account, unread);
                outs.extend(self.counts_changed(&account));
                outs
            }
            Event::FoldersChanged { account } if self.folders_of.as_ref() == Some(&account) => {
                self.list_folders(account)
            }
            Event::Folders { account, folders } => {
                let answered = account.clone();
                let mut outs = self.folders(account, &folders);
                outs.extend(self.folders_answered(&answered));
                outs
            }
            Event::MessagesChanged { folders } => {
                let mut outs = self.messages_changed(&folders);
                outs.extend(self.folders_stale(&folders));
                outs
            }
            Event::Messages {
                list,
                after,
                rows,
                end,
            } => {
                let mut outs = self.messages((list, after), &rows, end);
                outs.extend(self.older_shown());
                outs
            }
            Event::OlderLoaded {
                list,
                fetched,
                exhausted,
            } => self.older_loaded(list, fetched, exhausted),
            Event::OlderReset { folder, inbox } => self.older_dropped(folder, inbox),
            Event::OlderFailed { list, text } => self.older_failed(list, text),
            Event::MessagesFailed { list, after } => self.messages_failed((list, after)),
            Event::SearchResults {
                scope,
                query,
                server,
                rows,
            } => self.search_results(&SearchState::new(scope, query, server), &rows),
            Event::SearchFailed { scope, query, text } => self.search_failed(&scope, &query, text),
            Event::OutboxChanged => vec![Output::Send(Command::ListOutbox)],
            Event::Outbox { items } => {
                let rows = items
                    .iter()
                    .map(|r| convert::outbox_row(r, convert::outbox_reason(&self.accounts, r)))
                    .collect();
                self.outbox = items;
                vec![change(Model::Outbox, Op::Reset(rows))]
            }
            Event::Suggestions { query, items } => {
                let rows = items
                    .iter()
                    .map(|s| convert::suggestion_row(s, &query))
                    .collect();
                vec![change(Model::Suggestions, Op::Reset(rows))]
            }
            _ => Vec::new(),
        };
        self.prune_selection();
        let mut outs = outs;
        outs.extend(self.prune_open());
        self.mirror.track(&outs);
        outs
    }

    /// Show a list: its first page replaces what is shown when it arrives (for
    /// the list already shown it is merged in, like a refresh).
    /// The outputs clear the selection marks of the rows being left, and the
    /// open message unless the same list is shown again.
    pub fn open_list(&mut self, list: ListRef) -> (Command, Vec<Output>) {
        let mut outs = self.clear_marks();
        // Re-selecting the list that is already shown keeps the rows and the
        // open message; the first page is read again as a refresh (when one
        // is already on its way, its answer does the same).
        if let Some(cur) = self.current.as_mut().filter(|c| c.list == list) {
            if cur.first == First::Idle {
                cur.first = First::Refresh;
            }
            // The older row and a Load older batch in flight stay as they are.
            self.mirror.track(&outs);
            return (Command::ListMessages { list, after: None }, outs);
        }
        outs.extend(self.clear_open());
        outs.extend(self.older_reset());
        self.mirror.track(&outs);
        self.current = Some(Current::new(list));
        (Command::ListMessages { list, after: None }, outs)
    }
}
