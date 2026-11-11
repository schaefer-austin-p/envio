//! The screens' state: which list is shown, what is selected, what is
//! outstanding. Events in, outputs out; UI calls in, commands out. The
//! handlers live in the submodules, one concern each.

mod accounts;
mod messages;
mod search;
mod selection;

use std::collections::{BTreeSet, HashMap};

use envio_core::{Command, Event, FolderRole, ListRef, Service};

use crate::convert::{self, signal_of};
use crate::lists::ShownList;
use crate::mirror::Mirror;
use crate::models::{Model, ModelChange, Op};
use crate::rows::AccountRow;
use crate::signals::Signal;
use messages::Current;
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
    names: HashMap<String, String>,
    folders_of: Option<String>,
    /// Folder id to (account, role), from folder lists and Inbox rows.
    folder_roles: HashMap<i64, (String, FolderRole)>,
    current: Option<Current>,
    search: Option<SearchState>,
    found: ShownList,
    selection: BTreeSet<i64>,
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
        if let Some(signal) = signal_of(&event) {
            return vec![Output::Signal(signal)];
        }
        let outs = match event {
            Event::Started { accounts, .. } => self.started(&accounts),
            Event::AccountAdded { account } => self.account_added(&account),
            Event::AccountStatus {
                account,
                status,
                text,
            } => self.account_status(account, &status, text),
            Event::AccountRemoved { account } => self.account_removed(&account),
            Event::FoldersChanged { account } if self.folders_of.as_ref() == Some(&account) => {
                vec![Output::Send(Command::ListFolders { account })]
            }
            Event::Folders { account, folders } => self.folders(account, &folders),
            Event::MessagesChanged { folders } => self.messages_changed(&folders),
            Event::Messages {
                list,
                after,
                rows,
                end,
            } => self.messages((list, after), &rows, end),
            Event::SearchResults {
                scope,
                query,
                server,
                rows,
            } => self.search_results(&SearchState::new(scope, query, server), &rows),
            Event::SearchFailed { scope, query, text } => self.search_failed(&scope, &query, text),
            Event::OutboxChanged => vec![Output::Send(Command::ListOutbox)],
            Event::Outbox { items } => {
                let rows = items.iter().map(convert::outbox_row).collect();
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
        self.mirror.track(&outs);
        outs
    }

    /// Show a list: its first page replaces what is shown when it arrives.
    /// The outputs clear the selection marks of the rows being left.
    pub fn open_list(&mut self, list: ListRef) -> (Command, Vec<Output>) {
        let outs = self.clear_marks();
        self.mirror.track(&outs);
        self.current = Some(Current::new(list));
        (Command::ListMessages { list, after: None }, outs)
    }
}
