//! The Drafts list's pseudo-rows: the I/O half (files, cache, journal) that
//! feeds the pure merge in `local_rows`.

use std::collections::HashSet;

use envio_store::{Cursor, FolderId, FolderRole};

use super::Worker;
use super::local_rows::{leaving, local_rows, merge_page};
use crate::api::view::MessageRow;

impl Worker<'_> {
    /// The page of a Drafts-role folder with the local drafts that have no
    /// server copy merged in. Any other folder, or a read that fails, gives
    /// the page as it is.
    pub(super) fn with_local_drafts(
        &self,
        folder: FolderId,
        page: Vec<MessageRow>,
        (after, end): (Option<Cursor>, bool),
    ) -> Vec<MessageRow> {
        match self.local_draft_rows(folder) {
            Ok(local) => merge_page(page, local, after, end),
            Err(e) => {
                log::warn!("reading the local drafts: {e}");
                page
            }
        }
    }

    fn local_draft_rows(&self, folder: FolderId) -> envio_store::Result<Vec<MessageRow>> {
        let db = &self.store.db;
        // The local-only entry of an account with no Drafts folder (#303):
        // nothing is on the server, so nothing is trimmed against it. Once
        // the account has a Drafts folder the entry is gone and lists nothing.
        let local = self
            .local_folders
            .borrow()
            .account_of(folder)
            .map(str::to_owned);
        let (account, on_server) = match local {
            Some(account) => {
                let has_drafts = db
                    .folders(&account)?
                    .iter()
                    .any(|f| f.role == FolderRole::Drafts);
                if has_drafts {
                    return Ok(Vec::new());
                }
                (account, HashSet::new())
            }
            None => {
                let folder = db.folder(folder)?;
                if folder.role != FolderRole::Drafts {
                    return Ok(Vec::new());
                }
                (folder.account, db.message_ids_in(folder.id)?)
            }
        };
        let drafts = self.store.drafts.list()?;
        let folders = db.folders(&account)?;
        let leaving = leaving(self.store.journal.pending(), &account, &folders);
        let open: HashSet<String> = self.book.open_drafts().into_iter().collect();
        let mut ids = self.local_ids.borrow_mut();
        Ok(local_rows(
            &drafts,
            (&account, folder),
            (&on_server, &leaving, &open),
            &mut ids,
        ))
    }
}

#[cfg(test)]
mod tests;
