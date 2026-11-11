//! The screens' state: which list is shown, what is selected, what is
//! outstanding. Events in, outputs out; UI calls in, commands out.

use std::collections::{BTreeSet, HashMap};

use envio_core::{
    AccountStatus, Command, Cursor, Event, FolderRole, ListRef, MessageRow, MessageRowId,
    SearchScope, UserAction,
};

use crate::convert::{self, signal_of};
use crate::dates::bucket;
use crate::lists::{Shown, ShownList};
use crate::models::{Model, ModelChange, Op};
use crate::rows::{AccountRow, MessageRowUi, Row};
use crate::signals::Signal;

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

/// The first page of the shown list: not asked, asked for the first
/// time, or asked again to merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum First {
    Idle,
    Initial,
    Refresh,
}

#[derive(Debug)]
struct Current {
    list: ListRef,
    shown: ShownList,
    first: First,
    /// Mail changed while the first page was outstanding: ask again.
    refresh_again: bool,
    /// The cursor of the outstanding next page.
    more: Option<Cursor>,
    end: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SearchState {
    scope: SearchScope,
    query: String,
    server: bool,
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
    now: i64,
    utc_offset_s: i32,
}

impl Ui {
    pub fn new() -> Ui {
        Ui::default()
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
            } => self.search_results(
                &SearchState {
                    scope,
                    query,
                    server,
                },
                &rows,
            ),
            Event::SearchFailed { scope, query, text } => {
                let state = self.search.as_ref();
                if state.is_some_and(|s| s.scope == scope && s.query == query) {
                    vec![Output::Signal(Signal::SearchFailed(text))]
                } else {
                    Vec::new()
                }
            }
            Event::OutboxChanged => vec![Output::Send(Command::ListOutbox)],
            Event::Outbox { items } => {
                let rows = items.iter().map(convert::outbox_row).collect();
                vec![change(Model::Outbox, Op::Reset(rows))]
            }
            Event::Suggestions { items, .. } => {
                let rows = items.iter().map(convert::suggestion_row).collect();
                vec![change(Model::Suggestions, Op::Reset(rows))]
            }
            _ => Vec::new(),
        };
        self.prune_selection();
        outs
    }

    /// Show a list: its first page replaces what is shown when it arrives.
    pub fn open_list(&mut self, list: ListRef) -> Command {
        self.selection.clear();
        self.current = Some(Current {
            list,
            shown: ShownList::default(),
            first: First::Initial,
            refresh_again: false,
            more: None,
            end: false,
        });
        Command::ListMessages { list, after: None }
    }

    /// The next page, unless one is on its way or the list is at its end.
    pub fn more(&mut self) -> Option<Command> {
        let cur = self.current.as_mut()?;
        if cur.end || cur.more.is_some() || cur.first == First::Initial {
            return None;
        }
        let after = cur.shown.last_cursor()?;
        cur.more = Some(after);
        Some(Command::ListMessages {
            list: cur.list,
            after: Some(after),
        })
    }

    pub fn select(&mut self, id: i64, on: bool) -> Vec<Output> {
        let cur = self.current.as_mut().map(|c| &mut c.shown);
        let ops = [
            (Model::Messages, cur.and_then(|s| s.set_selected(id, on))),
            (Model::Search, self.found.set_selected(id, on)),
        ];
        let shown = ops.iter().any(|(_, op)| op.is_some()) || self.shows(id);
        if on && shown {
            self.selection.insert(id);
        } else {
            self.selection.remove(&id);
        }
        ops.into_iter()
            .filter_map(|(m, op)| op.map(|op| change(m, op)))
            .collect()
    }

    pub fn clear_selection(&mut self) -> Vec<Output> {
        let ids: Vec<i64> = std::mem::take(&mut self.selection).into_iter().collect();
        let mut outs = Vec::new();
        for id in ids {
            if let Some(op) = self
                .current
                .as_mut()
                .and_then(|c| c.shown.set_selected(id, false))
            {
                outs.push(change(Model::Messages, op));
            }
            if let Some(op) = self.found.set_selected(id, false) {
                outs.push(change(Model::Search, op));
            }
        }
        outs
    }

    /// `Act` on the selected messages. The selection clears at once; the
    /// outputs are the row updates that show it.
    pub fn act_on_selection(&mut self, action: UserAction) -> Option<(Command, Vec<Output>)> {
        if self.selection.is_empty() {
            return None;
        }
        let ids = self.selection.iter().map(|&id| MessageRowId(id)).collect();
        let outs = self.clear_selection();
        Some((Command::Act { ids, action }, outs))
    }

    pub fn search(&mut self, scope: SearchScope, query: String, server: bool) -> Command {
        self.search = Some(SearchState {
            scope: scope.clone(),
            query: query.clone(),
            server,
        });
        if server {
            Command::SearchServer { scope, query }
        } else {
            Command::Search { scope, query }
        }
    }

    fn shows(&self, id: i64) -> bool {
        self.found.contains(id) || self.current.as_ref().is_some_and(|c| c.shown.contains(id))
    }

    fn prune_selection(&mut self) {
        let keep: BTreeSet<i64> = self
            .selection
            .iter()
            .copied()
            .filter(|&i| self.shows(i))
            .collect();
        self.selection = keep;
    }

    fn started(&mut self, infos: &[envio_core::AccountInfo]) -> Vec<Output> {
        self.accounts = infos
            .iter()
            .map(|i| convert::account_row(i, &status_text(i)))
            .collect();
        self.names = infos
            .iter()
            .map(|i| (i.key.clone(), i.name.clone()))
            .collect();
        let rows = self.accounts.iter().cloned().map(Row::Account).collect();
        vec![change(Model::Accounts, Op::Reset(rows))]
    }

    fn account_added(&mut self, info: &envio_core::AccountInfo) -> Vec<Output> {
        let row = convert::account_row(info, &status_text(info));
        self.names.insert(info.key.clone(), info.name.clone());
        let at = self.accounts.len();
        self.accounts.push(row.clone());
        vec![
            change(
                Model::Accounts,
                Op::Insert {
                    at,
                    rows: vec![Row::Account(row)],
                },
            ),
            Output::Signal(Signal::AccountAdded(info.key.clone())),
        ]
    }

    fn account_status(&mut self, key: String, status: &AccountStatus, text: String) -> Vec<Output> {
        let mut outs = Vec::new();
        if let Some(at) = self.accounts.iter().position(|a| a.key == key) {
            let row = &mut self.accounts[at];
            row.status.clone_from(&text);
            if row.state != "broken" {
                convert::state_word(status).clone_into(&mut row.state);
            }
            let row = Row::Account(row.clone());
            outs.push(change(Model::Accounts, Op::Update { at, row }));
        }
        outs.push(Output::Signal(Signal::Status { account: key, text }));
        outs
    }

    fn account_removed(&mut self, key: &str) -> Vec<Output> {
        let mut outs = Vec::new();
        if let Some(at) = self.accounts.iter().position(|a| a.key == key) {
            self.accounts.remove(at);
            outs.push(change(Model::Accounts, Op::Remove { at, count: 1 }));
        }
        self.names.remove(key);
        self.folder_roles.retain(|_, (account, _)| account != key);
        if self.folders_of.as_deref() == Some(key) {
            self.folders_of = None;
            outs.push(change(Model::Folders, Op::Reset(Vec::new())));
        }
        if let Some(cur) = self.current.as_mut() {
            outs.extend(
                cur.shown
                    .remove_account(key)
                    .into_iter()
                    .map(|op| change(Model::Messages, op)),
            );
        }
        outs.extend(
            self.found
                .remove_account(key)
                .into_iter()
                .map(|op| change(Model::Search, op)),
        );
        outs
    }

    fn folders(&mut self, account: String, folders: &[envio_core::FolderRow]) -> Vec<Output> {
        self.folder_roles.retain(|_, (a, _)| *a != account);
        for f in folders {
            self.folder_roles.insert(f.id.0, (account.clone(), f.role));
        }
        let rows = folders
            .iter()
            .map(|f| convert::folder_row(&account, f))
            .collect();
        self.folders_of = Some(account);
        vec![change(Model::Folders, Op::Reset(rows))]
    }

    fn messages_changed(&mut self, folders: &[envio_core::FolderId]) -> Vec<Output> {
        let Some(cur) = self.current.as_mut() else {
            return Vec::new();
        };
        let affected = match cur.list {
            ListRef::Folder(id) => folders.contains(&id),
            ListRef::Inboxes => folders.iter().any(|f| {
                self.folder_roles
                    .get(&f.0)
                    .is_some_and(|(_, r)| *r == FolderRole::Inbox)
            }),
        };
        if !affected {
            return Vec::new();
        }
        if cur.first == First::Idle {
            cur.first = First::Refresh;
            vec![Output::Send(Command::ListMessages {
                list: cur.list,
                after: None,
            })]
        } else {
            cur.refresh_again = true;
            Vec::new()
        }
    }

    fn shown(&self, r: &MessageRow) -> Shown {
        let name = self.names.get(&r.account).cloned().unwrap_or_default();
        Shown {
            cursor: r.cursor,
            row: MessageRowUi {
                id: r.id.0,
                account: r.account.clone(),
                account_name: name,
                from: r.from.clone(),
                subject: r.subject.clone(),
                preview: r.preview.clone(),
                date: r.date.unwrap_or(0),
                date_bucket: bucket(self.now, r.date, self.utc_offset_s),
                unread: !r.flags.contains(envio_core::Flags::SEEN),
                flagged: r.flags.contains(envio_core::Flags::FLAGGED),
                has_attachments: r.has_attachments,
                selected: self.selection.contains(&r.id.0),
            },
        }
    }

    /// A page of `(list, after)`.
    fn messages(
        &mut self,
        (list, after): (ListRef, Option<Cursor>),
        rows: &[MessageRow],
        end: bool,
    ) -> Vec<Output> {
        let page: Vec<Shown> = rows.iter().map(|r| self.shown(r)).collect();
        if list == ListRef::Inboxes {
            for r in rows {
                self.folder_roles
                    .insert(r.folder.0, (r.account.clone(), FolderRole::Inbox));
            }
        }
        let Some(cur) = self.current.as_mut().filter(|c| c.list == list) else {
            return Vec::new();
        };
        let mut outs = Vec::new();
        match after {
            None => {
                let ops = match cur.first {
                    First::Idle => return outs,
                    First::Initial => {
                        cur.end = end;
                        vec![cur.shown.reset(page)]
                    }
                    First::Refresh => {
                        cur.end |= end;
                        cur.shown.merge_first(page, end)
                    }
                };
                outs.extend(ops.into_iter().map(|op| change(Model::Messages, op)));
                if std::mem::take(&mut cur.refresh_again) {
                    cur.first = First::Refresh;
                    outs.push(Output::Send(Command::ListMessages { list, after: None }));
                } else {
                    cur.first = First::Idle;
                }
            }
            Some(c) if cur.more == Some(c) => {
                cur.more = None;
                cur.end = end;
                outs.extend(cur.shown.append(page).map(|op| change(Model::Messages, op)));
            }
            Some(_) => {}
        }
        outs
    }

    fn search_results(&mut self, got: &SearchState, rows: &[MessageRow]) -> Vec<Output> {
        if self.search.as_ref() != Some(got) {
            return Vec::new();
        }
        let page = rows.iter().map(|r| self.shown(r)).collect();
        vec![change(Model::Search, self.found.reset(page))]
    }
}

fn status_text(info: &envio_core::AccountInfo) -> String {
    match &info.status {
        AccountStatus::Broken(reason) => reason.clone(),
        _ => String::new(),
    }
}
