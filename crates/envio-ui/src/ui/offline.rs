//! The offline banner of the tab being shown.

use envio_core::{offline_accounts, offline_here};

use super::Ui;

/// Which tab asks for the banner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BannerTab {
    Unified,
    Account(String),
    Folder(i64),
}

impl Ui {
    /// The offline text for `tab`, empty when nothing it shows is offline.
    /// `waiting` is the Outbox count added to the text.
    pub fn offline_banner(&self, tab: &BannerTab, waiting: u32) -> String {
        let waiting = u64::from(waiting);
        let key = match tab {
            BannerTab::Unified => {
                let emails: Vec<String> = self
                    .accounts
                    .iter()
                    .filter(|a| a.state == "offline")
                    .map(|a| a.email.clone())
                    .collect();
                return offline_accounts(&emails, waiting);
            }
            BannerTab::Account(key) => key,
            BannerTab::Folder(id) => match self.folder_roles.get(id) {
                Some((key, _)) => key,
                None => return String::new(),
            },
        };
        let offline = self
            .accounts
            .iter()
            .any(|a| a.key == *key && a.state == "offline");
        if offline {
            offline_here(waiting)
        } else {
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use envio_core::{
        AccountInfo, AccountStatus, CacheState, Event, FolderId, FolderRole, FolderRow,
    };

    use super::*;

    fn ui_with(offline: &[&str]) -> Ui {
        let mut ui = Ui::new();
        let accounts = ["a", "b", "c"]
            .iter()
            .map(|k| AccountInfo {
                key: (*k).into(),
                name: k.to_uppercase(),
                email: format!("{k}@example.org"),
                status: if offline.contains(k) {
                    AccountStatus::Offline { retry_at_ms: 1 }
                } else {
                    AccountStatus::Ready
                },
                unread: 0,
                broken: false,
            })
            .collect();
        let cache = CacheState::Existing;
        ui.apply(Event::Started { accounts, cache }, 0, 0);
        ui
    }

    fn inbox(id: i64) -> Vec<FolderRow> {
        vec![FolderRow {
            id: FolderId(id),
            path: "INBOX".into(),
            name: "Inbox".into(),
            role: FolderRole::Inbox,
            unread: 0,
        }]
    }

    #[test]
    fn an_account_tab_ignores_other_offline_accounts() {
        let ui = ui_with(&["b"]);
        assert_eq!(ui.offline_banner(&BannerTab::Account("a".into()), 0), "");
    }

    #[test]
    fn an_account_tab_shows_its_own_offline_state() {
        let ui = ui_with(&["b"]);
        assert_eq!(
            ui.offline_banner(&BannerTab::Account("b".into()), 1),
            "Offline. 1 message waiting to send"
        );
    }

    #[test]
    fn the_unified_tab_names_every_offline_account() {
        let ui = ui_with(&["a", "c"]);
        assert_eq!(
            ui.offline_banner(&BannerTab::Unified, 0),
            "2 accounts are offline: a@example.org, c@example.org"
        );
    }

    #[test]
    fn a_folder_tab_follows_its_account() {
        let mut ui = ui_with(&["b"]);
        for (account, id) in [("a", 1), ("b", 2)] {
            let folders = inbox(id);
            ui.apply(
                Event::Folders {
                    account: account.into(),
                    folders,
                },
                0,
                0,
            );
        }
        assert_eq!(ui.offline_banner(&BannerTab::Folder(1), 0), "");
        assert_eq!(ui.offline_banner(&BannerTab::Folder(2), 0), "Offline.");
        assert_eq!(ui.offline_banner(&BannerTab::Folder(9), 0), "");
    }

    #[test]
    fn no_banner_when_all_are_online() {
        let ui = ui_with(&[]);
        assert_eq!(ui.offline_banner(&BannerTab::Unified, 4), "");
        assert_eq!(ui.offline_banner(&BannerTab::Account("a".into()), 4), "");
    }
}
