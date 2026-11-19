//! Edit: reopening a message of a Drafts-role folder in Compose (#228).
//! The local file wins over the server text (Decision 3); a message with no
//! local file is imported from the server (T2).

use envio_store::{FolderRole, MessageRowId};

use super::Worker;
use crate::api::command::ComposeId;
use crate::drafts::lookup::draft_for;
use crate::text;

impl Worker<'_> {
    pub(super) fn start_edit(&mut self, compose: ComposeId, id: MessageRowId) {
        let unavailable = text::read::message_unavailable();
        let Ok(message) = self.store.db.message(id) else {
            return self.compose_refused(compose, unavailable);
        };
        let Ok(folder) = self.store.db.folder(message.folder) else {
            return self.compose_refused(compose, unavailable);
        };
        if folder.role != FolderRole::Drafts {
            return self.compose_refused(compose, unavailable);
        }
        let found = message
            .envelope
            .message_id
            .as_deref()
            .and_then(|message_id| match self.store.drafts.list() {
                Ok(drafts) => draft_for(&drafts, &folder.account, message_id),
                Err(e) => {
                    log::warn!("listing drafts to edit one: {e}");
                    None
                }
            });
        let Some(draft_id) = found else {
            // Until T2 imports the server message.
            return self.compose_refused(compose, text::compose::draft_unavailable());
        };
        if self.book.holder(&draft_id).is_some() {
            return self.compose_refused(compose, text::compose::draft_already_open());
        }
        self.start_from_draft(compose, Some(&draft_id));
    }
}
