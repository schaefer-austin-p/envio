//! Accounts and folders.

use envio_core::{AccountInfo, AccountStatus, FolderRow};

use super::{Output, Ui, change};
use crate::convert;
use crate::models::{Model, Op};
use crate::rows::{AccountRow, Row};
use crate::signals::Signal;

fn status_text(info: &AccountInfo) -> String {
    match &info.status {
        AccountStatus::Broken(reason) => reason.clone(),
        _ => String::new(),
    }
}

impl Ui {
    fn row_of(&self, info: &AccountInfo) -> AccountRow {
        let mut row = convert::account_row(info, &status_text(info));
        row.notify = !self.notify_off.contains(&info.key);
        row
    }

    pub(super) fn started(&mut self, infos: &[AccountInfo]) -> Vec<Output> {
        self.accounts = infos.iter().map(|i| self.row_of(i)).collect();
        self.names = infos
            .iter()
            .map(|i| (i.key.clone(), i.name.clone()))
            .collect();
        let rows = self.accounts.iter().cloned().map(Row::Account).collect();
        vec![change(Model::Accounts, Op::Reset(rows))]
    }

    /// A row for the account, new or (if `Started` listed it already)
    /// replaced in place, so it never shows twice.
    pub(super) fn account_added(&mut self, info: &AccountInfo) -> Vec<Output> {
        let row = self.row_of(info);
        self.names.insert(info.key.clone(), info.name.clone());
        let op = match self.accounts.iter().position(|a| a.key == info.key) {
            Some(at) => {
                self.accounts[at] = row.clone();
                Op::Update {
                    at,
                    row: Row::Account(row),
                }
            }
            None => {
                self.accounts.push(row.clone());
                Op::Insert {
                    at: self.accounts.len() - 1,
                    rows: vec![Row::Account(row)],
                }
            }
        };
        vec![
            change(Model::Accounts, op),
            Output::Signal(Signal::AccountAdded {
                account: info.key.clone(),
                email: info.email.clone(),
            }),
        ]
    }

    pub(super) fn account_status(
        &mut self,
        key: String,
        status: &AccountStatus,
        text: String,
    ) -> Vec<Output> {
        let mut outs = Vec::new();
        if let Some(at) = self.accounts.iter().position(|a| a.key == key) {
            let row = &mut self.accounts[at];
            row.status.clone_from(&text);
            if row.state != "broken" {
                convert::state_word(status).clone_into(&mut row.state);
                convert::set_pin(row, status, &text);
                row.attention = convert::needs_attention(&row.state);
            }
            let row = Row::Account(row.clone());
            outs.push(change(Model::Accounts, Op::Update { at, row }));
        }
        outs.push(Output::Signal(Signal::Status { account: key, text }));
        outs.extend(self.outbox_reasons());
        outs
    }

    /// The Outbox rows again when a reason changed with an account's status.
    fn outbox_reasons(&mut self) -> Option<Output> {
        let rows: Vec<Row> = self
            .outbox
            .iter()
            .map(|r| convert::outbox_row(r, convert::outbox_reason(&self.accounts, r)))
            .collect();
        (rows != self.mirror.rows(Model::Outbox)).then(|| change(Model::Outbox, Op::Reset(rows)))
    }

    /// The account's Inbox count changed: its row updates in place.
    pub(super) fn account_unread(&mut self, key: &str, unread: u32) -> Vec<Output> {
        let Some(at) = self.accounts.iter().position(|a| a.key == key) else {
            return Vec::new();
        };
        self.accounts[at].unread = unread;
        self.accounts[at].unread_text = convert::unread_text(unread);
        let row = Row::Account(self.accounts[at].clone());
        vec![change(Model::Accounts, Op::Update { at, row })]
    }

    pub(super) fn account_removed(&mut self, key: &str) -> Vec<Output> {
        let mut outs = Vec::new();
        if let Some(at) = self.accounts.iter().position(|a| a.key == key) {
            self.accounts.remove(at);
            outs.push(change(Model::Accounts, Op::Remove { at, count: 1 }));
        }
        self.names.remove(key);
        self.folder_roles.retain(|_, (account, _)| account != key);
        if self.drawer_account.as_deref() == Some(key) {
            self.drawer_account = None;
        }
        if self.folders_pending.as_deref() == Some(key) {
            self.folders_pending = None;
            self.folders_again = false;
        }
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

    pub(super) fn folders(&mut self, account: String, folders: &[FolderRow]) -> Vec<Output> {
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
}
