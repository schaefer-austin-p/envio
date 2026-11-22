//! Folder paths and the folder list. The path rules are pure
//! (`plan_paths`); `GraphServer::list` and `commit_all` do the I/O.

use std::collections::HashMap;

use envio_net::Result;
use envio_net::graph::model::GFolder;
use envio_net::imap::{Folder, Role};
use envio_store::{Db, FolderId, Upserted};

use super::GraphServer;
use super::map::Entry;

/// Graph nests mail folders at most 10 deep; this only guards a parent loop.
const MAX_DEPTH: usize = 20;

/// A folder the cache should hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Listed {
    pub path: String,
    pub graph_id: String,
    pub role: Option<Role>,
}

fn role_of(well_known: Option<&str>) -> Option<Role> {
    match well_known? {
        "inbox" => Some(Role::Inbox),
        "sentitems" => Some(Role::Sent),
        "drafts" => Some(Role::Drafts),
        "deleteditems" => Some(Role::Trash),
        "junkemail" => Some(Role::Junk),
        "archive" => Some(Role::Archive),
        _ => None,
    }
}

/// `/` and `.` split paths elsewhere, so inside a name they become
/// look-alikes.
fn segment(name: &str) -> String {
    name.replace('/', "\u{2215}").replace('.', "\u{2024}")
}

fn path_of(f: &GFolder, by_id: &HashMap<&str, &GFolder>, depth: usize) -> String {
    if f.well_known.as_deref() == Some("inbox") {
        return "INBOX".into();
    }
    let own = segment(&f.display_name);
    match f.parent_id.as_deref().and_then(|p| by_id.get(p)) {
        Some(parent) if depth < MAX_DEPTH => {
            format!("{}/{own}", path_of(parent, by_id, depth + 1))
        }
        _ => own,
    }
}

/// The mail folders with their cache paths. Hidden folders and folders of
/// another class (contacts, calendar) are left out; so is a folder whose
/// path an earlier one already has, because the cache keys folders by path.
pub(super) fn plan_paths(all: &[GFolder]) -> Vec<Listed> {
    let by_id: HashMap<&str, &GFolder> = all.iter().map(|f| (f.id.as_str(), f)).collect();
    let mut out: Vec<Listed> = Vec::new();
    for f in all {
        let mail = f.class.as_deref().is_none_or(|c| c == "IPF.Note");
        if f.hidden || !mail {
            continue;
        }
        let path = path_of(f, &by_id, 0);
        if path.is_empty() || out.iter().any(|l| l.path == path) {
            continue;
        }
        out.push(Listed {
            path,
            graph_id: f.id.clone(),
            role: role_of(f.well_known.as_deref()),
        });
    }
    out
}

impl GraphServer {
    /// One fresh folder walk. A folder keeps the row id it had when its path
    /// and Graph id are unchanged.
    pub(super) async fn walk(&mut self) -> Result<Vec<Listed>> {
        let listed = plan_paths(&self.client.folders().await?);
        let old = std::mem::take(&mut self.paths);
        for l in &listed {
            let row = old
                .get(&l.path)
                .filter(|e| e.graph_id == l.graph_id)
                .and_then(|e| e.row);
            self.paths.insert(
                l.path.clone(),
                Entry {
                    graph_id: l.graph_id.clone(),
                    row,
                },
            );
        }
        Ok(listed)
    }

    pub(super) async fn list(&mut self) -> Result<Vec<Folder>> {
        Ok(self
            .walk()
            .await?
            .into_iter()
            .map(|l| Folder {
                name: l.path.rsplit('/').next().unwrap_or(&l.path).to_owned(),
                path: l.path,
                delimiter: Some("/".into()),
                role: l.role,
                selectable: true,
            })
            .collect())
    }

    /// Records each listed folder's Graph id on its folder row. A path whose
    /// Graph id changed is reset (`Upserted::Changed`), and its pending delta
    /// is dropped.
    pub(super) fn commit_all(&mut self, db: &Db, account: &str) -> envio_store::Result<()> {
        let rows = db.folders(account)?;
        for (path, entry) in &mut self.paths {
            let Some(row) = rows.iter().find(|r| r.path == *path) else {
                continue;
            };
            if let Upserted::Changed(_) = db.graph_upsert_folder(row.id, &entry.graph_id)? {
                self.pending.remove(&row.id);
            }
            entry.row = Some(row.id);
        }
        Ok(())
    }

    pub(super) fn commit_one(&mut self, db: &Db, id: FolderId) -> envio_store::Result<()> {
        if let Some(p) = self.pending.remove(&id) {
            db.graph_commit(id, &p.link, p.modseq)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(id: &str, name: &str, parent: &str, known: Option<&str>) -> GFolder {
        GFolder {
            id: id.into(),
            parent_id: Some(parent.into()),
            display_name: name.into(),
            well_known: known.map(Into::into),
            hidden: false,
            class: None,
            child_count: 0,
        }
    }

    #[test]
    fn names_with_separators_and_a_duplicate_path() {
        let all = [
            f("a", "Inbox", "ROOT", Some("inbox")),
            f("b", "x/y.z", "a", None),
            f("c", "Dup", "ROOT", None),
            f("d", "Dup", "ROOT", None),
        ];
        let paths: Vec<_> = plan_paths(&all).into_iter().map(|l| l.path).collect();
        assert_eq!(paths, ["INBOX", "INBOX/x\u{2215}y\u{2024}z", "Dup"]);
    }
}
