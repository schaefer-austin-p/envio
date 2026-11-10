//! Which folders the cache should hold: the server's selectable folders,
//! with their roles. Pure.

use envio_net::imap::Folder as ServerFolder;
use envio_store::{Folder, FolderId, FolderRole};

use crate::convert::folder_role;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct FolderChanges {
    /// New folders, and known ones whose role changed.
    pub upsert: Vec<(String, FolderRole)>,
    /// Cached folders the server no longer has.
    pub remove: Vec<FolderId>,
}

/// Two folder paths name the same folder if they are byte-equal, or both are
/// INBOX in any ASCII case (RFC 3501 makes INBOX case-insensitive). Other
/// paths stay case-sensitive.
fn same_path(a: &str, b: &str) -> bool {
    a == b || (a.eq_ignore_ascii_case("INBOX") && b.eq_ignore_ascii_case("INBOX"))
}

/// Plans the cache against the server's list. A list with no selectable
/// INBOX is untrusted (RFC 3501 says INBOX always exists, so it is partial
/// or empty), so nothing is removed from it; additions are still planned.
pub fn plan_folders(server: &[ServerFolder], cached: &[Folder]) -> FolderChanges {
    let wanted: Vec<(&str, FolderRole)> = server
        .iter()
        .filter(|folder| folder.selectable)
        .map(|folder| (folder.path.as_str(), folder_role(folder.role)))
        .collect();
    let trusted = wanted
        .iter()
        .any(|(path, _)| path.eq_ignore_ascii_case("INBOX"));
    let upsert = wanted
        .iter()
        .filter(|(path, role)| {
            !cached
                .iter()
                .any(|folder| same_path(&folder.path, path) && folder.role == *role)
        })
        .map(|(path, role)| ((*path).to_owned(), *role))
        .collect();
    let remove = if trusted {
        cached
            .iter()
            .filter(|folder| !wanted.iter().any(|(path, _)| same_path(&folder.path, path)))
            .map(|folder| folder.id)
            .collect()
    } else {
        Vec::new()
    };
    FolderChanges { upsert, remove }
}

/// The order folders are synced in: what the user looks at first.
pub fn sync_order(folders: &[Folder]) -> Vec<FolderId> {
    let rank = |role: FolderRole| match role {
        FolderRole::Inbox => 0,
        FolderRole::Sent => 1,
        FolderRole::Drafts => 2,
        FolderRole::Archive | FolderRole::Other => 3,
        FolderRole::Junk => 4,
        FolderRole::Trash => 5,
    };
    let mut ordered: Vec<&Folder> = folders.iter().collect();
    ordered.sort_by(|a, b| rank(a.role).cmp(&rank(b.role)).then(a.path.cmp(&b.path)));
    ordered.into_iter().map(|folder| folder.id).collect()
}

/// The account's folder with `role`, e.g. Trash for Delete or Sent for the
/// Outbox.
pub fn with_role(folders: &[Folder], role: FolderRole) -> Option<&Folder> {
    folders.iter().find(|folder| folder.role == role)
}

#[cfg(test)]
mod tests {
    use envio_net::imap::Role;

    use super::*;

    fn server(path: &str, role: Option<Role>, selectable: bool) -> ServerFolder {
        ServerFolder {
            path: path.into(),
            name: path.into(),
            delimiter: Some("/".into()),
            role,
            selectable,
        }
    }

    fn cached(id: i64, path: &str, role: FolderRole) -> Folder {
        Folder {
            id: FolderId(id),
            account: "1700".into(),
            path: path.into(),
            role,
            sync: None,
        }
    }

    #[test]
    fn new_folders_are_added_gone_ones_removed_and_roles_updated() {
        let server = [
            server("INBOX", Some(Role::Inbox), true),
            server("Archive", Some(Role::Archive), true),
            server("Bin", Some(Role::Trash), true),
            server("[Gmail]", None, false),
        ];
        let cached = [
            cached(1, "INBOX", FolderRole::Inbox),
            cached(2, "Bin", FolderRole::Other),
            cached(3, "Old", FolderRole::Other),
        ];
        let changes = plan_folders(&server, &cached);
        assert_eq!(
            changes.upsert,
            [
                ("Archive".to_owned(), FolderRole::Archive),
                ("Bin".to_owned(), FolderRole::Trash),
            ]
        );
        assert_eq!(changes.remove, [FolderId(3)]);
    }

    #[test]
    fn nothing_changes_when_the_cache_matches() {
        let server = [server("INBOX", Some(Role::Inbox), true)];
        let cached = [cached(1, "INBOX", FolderRole::Inbox)];
        assert_eq!(plan_folders(&server, &cached), FolderChanges::default());
    }

    #[test]
    fn the_inbox_syncs_first_and_trash_last() {
        let folders = [
            cached(1, "Trash", FolderRole::Trash),
            cached(2, "Lists", FolderRole::Other),
            cached(3, "Sent", FolderRole::Sent),
            cached(4, "INBOX", FolderRole::Inbox),
            cached(5, "Archive", FolderRole::Archive),
        ];
        assert_eq!(
            sync_order(&folders),
            [
                FolderId(4),
                FolderId(3),
                FolderId(5),
                FolderId(2),
                FolderId(1)
            ]
        );
        assert_eq!(
            with_role(&folders, FolderRole::Sent).map(|f| f.id),
            Some(FolderId(3))
        );
        assert_eq!(with_role(&folders, FolderRole::Junk), None);
    }

    #[test]
    fn a_list_without_inbox_removes_nothing_but_still_adds() {
        let cached = [
            cached(1, "INBOX", FolderRole::Inbox),
            cached(2, "Archive", FolderRole::Archive),
        ];
        let lists = [
            vec![],
            vec![server("Archive", Some(Role::Archive), true)],
            vec![
                server("Archive", Some(Role::Archive), true),
                server("Lists", None, true),
            ],
        ];
        for list in &lists {
            assert!(plan_folders(list, &cached).remove.is_empty());
        }
        let changes = plan_folders(&lists[2], &cached);
        assert_eq!(changes.upsert, [("Lists".to_owned(), FolderRole::Other)]);
    }

    #[test]
    fn only_inbox_ignores_case() {
        // (cached path, server path): both name INBOX, so nothing changes.
        let cases = [("INBOX", "Inbox"), ("inbox", "INBOX"), ("Inbox", "iNbOx")];
        for (cached_path, server_path) in cases {
            let server = [server(server_path, Some(Role::Inbox), true)];
            let cached = [cached(1, cached_path, FolderRole::Inbox)];
            assert_eq!(
                plan_folders(&server, &cached),
                FolderChanges::default(),
                "{cached_path} vs {server_path}"
            );
        }
    }

    #[test]
    fn other_paths_stay_case_sensitive() {
        let server = [
            server("INBOX", Some(Role::Inbox), true),
            server("archive", Some(Role::Archive), true),
        ];
        let cached = [
            cached(1, "INBOX", FolderRole::Inbox),
            cached(2, "Archive", FolderRole::Archive),
        ];
        let changes = plan_folders(&server, &cached);
        assert_eq!(
            changes.upsert,
            [("archive".to_owned(), FolderRole::Archive)]
        );
        assert_eq!(changes.remove, [FolderId(2)]);
    }

    #[test]
    fn a_folder_that_turns_noselect_is_removed() {
        let server = [
            server("INBOX", Some(Role::Inbox), true),
            server("Lists", None, false),
        ];
        let cached = [
            cached(1, "INBOX", FolderRole::Inbox),
            cached(2, "Lists", FolderRole::Other),
        ];
        let changes = plan_folders(&server, &cached);
        assert!(changes.upsert.is_empty());
        assert_eq!(changes.remove, [FolderId(2)]);
    }

    #[test]
    fn other_folders_sort_by_path_and_junk_comes_before_trash() {
        let folders = [
            cached(1, "Trash", FolderRole::Trash),
            cached(2, "Zeta", FolderRole::Other),
            cached(3, "Junk", FolderRole::Junk),
            cached(4, "Alpha", FolderRole::Other),
        ];
        assert_eq!(
            sync_order(&folders),
            [FolderId(4), FolderId(2), FolderId(3), FolderId(1)]
        );
    }
}
