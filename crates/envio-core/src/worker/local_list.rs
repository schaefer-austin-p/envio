//! The Drafts list's pseudo-rows: the I/O half (files, cache, journal) that
//! feeds the pure merge in `local_rows`.

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
        let folder = db.folder(folder)?;
        if folder.role != FolderRole::Drafts {
            return Ok(Vec::new());
        }
        let drafts = self.store.drafts.list()?;
        let folders = db.folders(&folder.account)?;
        let on_server = db.message_ids_in(folder.id)?;
        let leaving = leaving(self.store.journal.pending(), &folder.account, &folders);
        let mut ids = self.local_ids.borrow_mut();
        Ok(local_rows(
            &drafts,
            (&folder.account, folder.id),
            (&on_server, &leaving),
            &mut ids,
        ))
    }
}
