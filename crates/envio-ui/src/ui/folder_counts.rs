//! The drawer's folder counts: the drawer's account's folder list is asked
//! again when mail it holds changes, so the counts follow what is read.

use envio_core::{Command, FolderId};

use super::{Output, Ui};

impl Ui {
    /// The drawer is open on `account`'s folders, or closed (`None`).
    /// Only then do changes re-list them: the folder model is shared with
    /// screens that list folders themselves.
    pub fn set_drawer_account(&mut self, account: Option<String>) {
        self.drawer_account = account;
    }

    /// Mail changed in these folders: the drawer's account's list is stale if
    /// one of them is its own or not listed yet.
    pub(super) fn folders_stale(&mut self, folders: &[FolderId]) -> Vec<Output> {
        let Some(account) = self.drawer_account.clone() else {
            return Vec::new();
        };
        let touched = folders.iter().any(|f| {
            self.folder_roles
                .get(&f.0)
                .is_none_or(|(owner, _)| *owner == account)
        });
        if touched {
            self.list_folders(account)
        } else {
            Vec::new()
        }
    }

    /// An account's Inbox count changed: its folder counts may have too.
    pub(super) fn counts_changed(&mut self, account: &str) -> Vec<Output> {
        if self.drawer_account.as_deref() == Some(account) {
            self.list_folders(account.to_owned())
        } else {
            Vec::new()
        }
    }

    /// Ask for `account`'s folders. While its answer is outstanding, later
    /// asks are folded into one more list after the answer.
    pub(super) fn list_folders(&mut self, account: String) -> Vec<Output> {
        if self.folders_pending.as_deref() == Some(account.as_str()) {
            self.folders_again = true;
            return Vec::new();
        }
        self.folders_pending = Some(account.clone());
        self.folders_again = false;
        vec![Output::Send(Command::ListFolders { account })]
    }

    /// `account`'s folder list arrived: if it was the outstanding one, ask
    /// once more when changes came in meanwhile.
    pub(super) fn folders_answered(&mut self, account: &str) -> Vec<Output> {
        if self.folders_pending.as_deref() != Some(account) {
            return Vec::new();
        }
        self.folders_pending = None;
        if std::mem::take(&mut self.folders_again)
            && self.drawer_account.as_deref() == Some(account)
        {
            self.list_folders(account.to_owned())
        } else {
            Vec::new()
        }
    }
}
