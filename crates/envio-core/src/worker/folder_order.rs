//! The order folders appear in: the special ones first, then the rest.

use envio_store::{Folder, FolderRole};

fn rank(role: FolderRole) -> u8 {
    match role {
        FolderRole::Inbox => 0,
        FolderRole::Drafts => 1,
        FolderRole::Sent => 2,
        FolderRole::Archive => 3,
        FolderRole::Junk => 4,
        FolderRole::Trash => 5,
        FolderRole::Other => 6,
    }
}

/// Inbox, Drafts, Sent, Archive, Junk, Trash, then the rest by path without
/// regard to case, so a subfolder follows its parent.
pub(super) fn drawer_order(folders: &mut [Folder]) {
    folders.sort_by_cached_key(|f| (rank(f.role), f.path.to_lowercase(), f.path.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use envio_store::FolderId;

    fn folder(id: i64, path: &str, role: FolderRole) -> Folder {
        Folder {
            id: FolderId(id),
            account: "a".into(),
            path: path.into(),
            role,
            sync: None,
        }
    }

    fn paths(folders: &[Folder]) -> Vec<&str> {
        folders.iter().map(|f| f.path.as_str()).collect()
    }

    #[test]
    fn roles_come_first_in_the_design_order() {
        // As the store lists them: by path.
        let mut folders = vec![
            folder(1, "Archive", FolderRole::Archive),
            folder(2, "Drafts", FolderRole::Drafts),
            folder(3, "INBOX", FolderRole::Inbox),
            folder(4, "Junk", FolderRole::Junk),
            folder(5, "Sent", FolderRole::Sent),
            folder(6, "Trash", FolderRole::Trash),
        ];
        drawer_order(&mut folders);
        assert_eq!(
            paths(&folders),
            ["INBOX", "Drafts", "Sent", "Archive", "Junk", "Trash"]
        );
    }

    #[test]
    fn other_folders_follow_by_name_and_subfolders_follow_their_parent() {
        let mut folders = vec![
            folder(1, "Banking", FolderRole::Other),
            folder(2, "INBOX", FolderRole::Inbox),
            folder(3, "Work/Sub", FolderRole::Other),
            folder(4, "Trash", FolderRole::Trash),
            folder(5, "apples", FolderRole::Other),
            folder(6, "Work", FolderRole::Other),
        ];
        drawer_order(&mut folders);
        assert_eq!(
            paths(&folders),
            ["INBOX", "Trash", "apples", "Banking", "Work", "Work/Sub"]
        );
    }
}
