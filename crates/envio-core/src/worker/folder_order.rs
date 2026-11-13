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

/// A path's segments, split as the folder list indents them (on `/` and `.`).
fn segments(path: &str) -> Vec<String> {
    path.split(['/', '.']).map(str::to_owned).collect()
}

fn lower(segments: &[String]) -> Vec<String> {
    segments.iter().map(|s| s.to_lowercase()).collect()
}

/// Inbox, Drafts, Sent, Archive, Junk, Trash, then the rest. A folder under
/// one of those (`INBOX/Work`, `Archive.2023`) goes with it and ranks by its
/// role. Within a group, paths compare segment by segment without regard to
/// case, so a subtree follows its parent.
pub(super) fn drawer_order(folders: Vec<Folder>) -> Vec<Folder> {
    let parts: Vec<Vec<String>> = folders.iter().map(|f| segments(&f.path)).collect();
    let anchors: Vec<(FolderRole, &Vec<String>)> = folders
        .iter()
        .zip(&parts)
        .filter(|(f, _)| f.role != FolderRole::Other)
        .map(|(f, p)| (f.role, p))
        .collect();
    let keys: Vec<_> = parts
        .iter()
        .map(|mine| {
            // The longest path that is this one or an ancestor, with a role.
            let (role, group) = anchors
                .iter()
                .filter(|(_, theirs)| mine.starts_with(theirs))
                .max_by_key(|(_, theirs)| theirs.len())
                .map_or((FolderRole::Other, Vec::new()), |(role, theirs)| {
                    (*role, lower(theirs))
                });
            (rank(role), group, lower(mine), mine.clone())
        })
        .collect();
    let mut keyed: Vec<_> = keys.into_iter().zip(folders).collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    keyed.into_iter().map(|(_, f)| f).collect()
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
        let folders = vec![
            folder(1, "Archive", FolderRole::Archive),
            folder(2, "Drafts", FolderRole::Drafts),
            folder(3, "INBOX", FolderRole::Inbox),
            folder(4, "Junk", FolderRole::Junk),
            folder(5, "Sent", FolderRole::Sent),
            folder(6, "Trash", FolderRole::Trash),
        ];
        let folders = drawer_order(folders);
        assert_eq!(
            paths(&folders),
            ["INBOX", "Drafts", "Sent", "Archive", "Junk", "Trash"]
        );
    }

    #[test]
    fn other_folders_follow_by_name_and_subfolders_follow_their_parent() {
        let folders = vec![
            folder(1, "Banking", FolderRole::Other),
            folder(2, "INBOX", FolderRole::Inbox),
            folder(3, "Work/Sub", FolderRole::Other),
            folder(4, "Trash", FolderRole::Trash),
            folder(5, "apples", FolderRole::Other),
            folder(6, "Work", FolderRole::Other),
        ];
        let folders = drawer_order(folders);
        assert_eq!(
            paths(&folders),
            ["INBOX", "Trash", "apples", "Banking", "Work", "Work/Sub"]
        );
    }

    #[test]
    fn children_follow_their_role_parent_and_subtrees_stay_together() {
        for sep in ['/', '.'] {
            let p = |a: &str, b: &str| format!("{a}{sep}{b}");
            let folders = vec![
                folder(1, "Archive", FolderRole::Archive),
                folder(2, &p("Archive", "2023"), FolderRole::Other),
                folder(3, "INBOX", FolderRole::Inbox),
                folder(4, &p("INBOX", "Work"), FolderRole::Other),
                folder(5, "Trash", FolderRole::Trash),
                folder(6, "Work", FolderRole::Other),
                folder(7, "Work Stuff", FolderRole::Other),
                folder(8, &p("Work", "Sub"), FolderRole::Other),
                folder(9, "Work-2", FolderRole::Other),
            ];
            let folders = drawer_order(folders);
            let want = [
                "INBOX".to_owned(),
                p("INBOX", "Work"),
                "Archive".to_owned(),
                p("Archive", "2023"),
                "Trash".to_owned(),
                "Work".to_owned(),
                p("Work", "Sub"),
                "Work Stuff".to_owned(),
                "Work-2".to_owned(),
            ];
            assert_eq!(paths(&folders), want, "delimiter {sep}");
        }
    }
}
