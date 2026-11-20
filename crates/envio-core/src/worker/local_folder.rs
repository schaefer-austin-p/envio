//! The local-only Drafts entry (#303): an account whose server has no folder
//! with the Drafts role still gets a "Drafts" folder in the drawer that lists
//! its local drafts. It lives only in the core: nothing is created on the
//! server and the cache holds no row for it.

use std::collections::HashMap;

use envio_store::{Folder, FolderId, FolderRole};

/// The negative `FolderId` of each account's local Drafts entry, for this
/// session. Cached folders have positive SQLite rowids, so the two never
/// collide.
pub(super) struct LocalFolders {
    by_account: HashMap<String, i64>,
    by_id: HashMap<i64, String>,
    next: i64,
}

impl Default for LocalFolders {
    fn default() -> Self {
        Self {
            by_account: HashMap::new(),
            by_id: HashMap::new(),
            next: -1,
        }
    }
}

impl LocalFolders {
    /// The account's entry id: the same one every time.
    pub fn id_of(&mut self, account: &str) -> FolderId {
        if let Some(id) = self.by_account.get(account) {
            return FolderId(*id);
        }
        let id = self.next;
        self.next -= 1;
        self.by_account.insert(account.to_owned(), id);
        self.by_id.insert(id, account.to_owned());
        FolderId(id)
    }

    /// The account a local entry belongs to; `None` for a cached folder.
    pub fn account_of(&self, folder: FolderId) -> Option<&str> {
        self.by_id.get(&folder.0).map(String::as_str)
    }

    /// `folders` with the account's local Drafts entry added when none of
    /// them has the Drafts role.
    pub fn with_entry(&mut self, account: &str, mut folders: Vec<Folder>) -> Vec<Folder> {
        if folders.iter().all(|f| f.role != FolderRole::Drafts) {
            folders.push(Folder {
                id: self.id_of(account),
                account: account.to_owned(),
                path: "Drafts".to_owned(),
                role: FolderRole::Drafts,
                sync: None,
            });
        }
        folders
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: i64, path: &str, role: FolderRole) -> Folder {
        Folder {
            id: FolderId(id),
            account: "a".into(),
            path: path.into(),
            role,
            sync: None,
        }
    }

    #[test]
    fn entry_is_added_only_without_a_drafts_role() {
        let mut l = LocalFolders::default();
        let none = l.with_entry("a", vec![folder(1, "INBOX", FolderRole::Inbox)]);
        assert_eq!(none.len(), 2);
        assert!(none[1].id.0 < 0 && none[1].role == FolderRole::Drafts);
        let some = l.with_entry(
            "a",
            vec![
                folder(1, "INBOX", FolderRole::Inbox),
                folder(2, "Entwürfe", FolderRole::Drafts),
            ],
        );
        assert_eq!(some.len(), 2);
        assert!(some.iter().all(|f| f.id.0 > 0));
    }

    #[test]
    fn ids_are_stable_per_account_and_distinct() {
        let mut l = LocalFolders::default();
        let a = l.id_of("a");
        assert_eq!(l.id_of("a"), a);
        let b = l.id_of("b");
        assert_ne!(a, b);
        assert_eq!(l.account_of(b), Some("b"));
        assert_eq!(l.account_of(FolderId(3)), None);
    }
}
