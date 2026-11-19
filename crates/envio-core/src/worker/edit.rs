//! Edit: reopening a message of a Drafts-role folder in Compose (#228).
//! The local file wins over the server text (Decision 3); a message with no
//! local file is imported from the server (Decision 4). The import waits for
//! the whole body as Forward does, so attachments come along.

use envio_store::{Flags, FolderRole, MessageRowId};

use super::Worker;
use crate::api::command::{ComposeId, StartFrom};
use crate::clock::now_ms;
use crate::drafts::import::{Source, draft_from_raw};
use crate::drafts::lookup::draft_for;
use crate::text;

impl Worker<'_> {
    pub(super) fn start_edit(&mut self, compose: ComposeId, id: MessageRowId) {
        self.edit_or_import(compose, id, None);
    }

    /// `raw` is the message's whole body once it is at hand. Without it, a
    /// message that has no local file waits for it (`start_from_message`),
    /// and comes back here with it. The lookup is made again then: another
    /// Edit of the same message may have imported it meanwhile, and a
    /// second copy must not be made.
    pub(super) fn edit_or_import(
        &mut self,
        compose: ComposeId,
        id: MessageRowId,
        raw: Option<&[u8]>,
    ) {
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
        let message_id = message.envelope.message_id.as_deref();
        let found = message_id.and_then(|message_id| match self.store.drafts.list() {
            Ok(drafts) => draft_for(&drafts, &folder.account, message_id)
                .and_then(|id| {
                    // An import that never got its files in (a crash): it is
                    // not the user's work yet, so it is dropped (no note: the
                    // original it names must stay) and imported again.
                    if drafts
                        .iter()
                        .any(|(d, draft)| *d == id && draft.server.incomplete)
                    {
                        if let Err(e) = self.store.drafts.remove_imported(&id) {
                            log::warn!("discarding an incomplete import {id}: {e}");
                        }
                        return None;
                    }
                    Some(id)
                })
                .map(|id| {
                    let detached = drafts
                        .iter()
                        .any(|(d, draft)| *d == id && draft.server.is_detached_copy());
                    (id, detached)
                }),
            Err(e) => {
                log::warn!("listing drafts to edit one: {e}");
                None
            }
        });
        if let Some((draft_id, detached)) = found {
            if self.book.holder(&draft_id).is_some() {
                return self.compose_refused(compose, text::compose::draft_already_open());
            }
            // A copy of a draft the original of which stays says so every
            // time it is opened, not only the first.
            let note = if detached {
                text::compose::editing_a_copy()
            } else {
                String::new()
            };
            return self.start_from_draft(compose, Some(&draft_id), note);
        }
        // A file that cannot be read still holds the user's text and names
        // this message: importing beside it would make a second copy.
        let damaged = match message_id {
            Some(m) => self.store.drafts.damaged_naming(&folder.account, m),
            None => Ok(None),
        };
        match damaged {
            Ok(None) => {}
            Ok(Some(draft_id)) => {
                log::warn!("draft {draft_id} names this message but cannot be read; not importing");
                return self.compose_refused(compose, text::compose::draft_unavailable());
            }
            Err(e) => {
                log::warn!("looking for a damaged draft of this message: {e}");
                return self.compose_refused(compose, text::compose::draft_unavailable());
            }
        }
        match raw {
            Some(raw) => {
                let plain_flags =
                    message.flags.without(Flags::SEEN | Flags::DRAFT) == Flags::empty();
                self.import_draft(compose, (&folder.account, message_id, plain_flags), raw);
            }
            None => self.start_from_message(compose, id, StartFrom::Edit { id }),
        }
    }

    /// Makes a local draft of the server draft's bytes and opens it. The
    /// store does it all or nothing and links the copy to the original
    /// only after its files are in (see `Drafts::import`); a failure
    /// refuses, with the original untouched.
    fn import_draft(
        &mut self,
        compose: ComposeId,
        (account, message_id, plain_flags): (&str, Option<&str>, bool),
        raw: &[u8],
    ) {
        let Some((identity, name)) = self
            .store
            .accounts
            .get(account)
            .map(|a| (a.settings.email.clone(), a.settings.name.clone()))
        else {
            return self.compose_refused(compose, text::compose::no_account());
        };
        let source = Source {
            account,
            identity: &identity,
            name: &name,
            message_id,
            plain_flags,
            safe_name: super::read::safe_file_name,
        };
        let Some(imported) = draft_from_raw(raw, &source, now_ms()) else {
            return self.compose_refused(compose, text::read::message_unavailable());
        };
        let made = self.store.drafts.import(
            &imported.draft,
            imported.link.as_deref(),
            |drafts, id, draft| {
                for a in &imported.attachments {
                    let name =
                        super::read::safe_file_name(a.filename.as_deref().unwrap_or(""), a.index);
                    draft
                        .attachments
                        .push(drafts.keep_bytes(id, &name, &a.bytes)?);
                }
                Ok(())
            },
        );
        let (id, linked) = match made {
            Ok(made) => made,
            Err(e) => {
                log::warn!("importing a draft: {e}");
                return self.compose_refused(compose, text::compose::draft_unavailable());
            }
        };
        let note = if linked {
            String::new()
        } else {
            text::compose::editing_a_copy()
        };
        self.start_from_draft(compose, Some(&id), note);
    }
}

#[cfg(test)]
mod tests {
    use envio_store::{
        BodyState, Db, Envelope, Flags, FolderRole, MessageRowId, NewMessage, StorePaths,
    };

    use super::super::testing::{Sink, open, seed};
    use super::*;
    use crate::api::event::Event;

    const C: ComposeId = ComposeId(7);
    const MID: &str = "<d1@example.org>";
    const RAW: &[u8] = b"From: Test <me@example.org>\r\nTo: ada@example.org\r\n\
Subject: server text\r\nMessage-ID: <d1@example.org>\r\n\r\nserver body\r\n";

    /// A worker whose Drafts folder holds one message with `state` as its body.
    fn editing<'a>(
        dir: &std::path::Path,
        sink: &'a Sink,
        state: BodyState,
    ) -> (Worker<'a>, MessageRowId) {
        let (config, seeded) = seed(dir);
        let paths = StorePaths::new(&config.cache_dir, &config.data_dir);
        let db = Db::open(&paths.db()).unwrap();
        let folder = db
            .upsert_folder(&seeded.account, "Drafts", FolderRole::Drafts)
            .unwrap();
        let new = NewMessage {
            uid: 1,
            size: 10,
            envelope: Envelope {
                message_id: Some(MID.into()),
                ..Envelope::default()
            },
            flags: Flags::default(),
            preview: String::new(),
        };
        let id = db.upsert_message(folder, &new).unwrap();
        db.set_body_state(id, state).unwrap();
        drop(db);
        let mut worker = open(&config, sink);
        worker.start_compose(C, StartFrom::Edit { id }, (None, None));
        (worker, id)
    }

    fn said(sink: &Sink) -> Vec<Event> {
        sink.events()
            .into_iter()
            .filter(|e| !matches!(e, Event::Started { .. } | Event::Settings(_)))
            .collect()
    }

    #[test]
    fn an_edit_without_a_cached_body_waits_for_the_whole_message() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (worker, id) = editing(dir.path(), &sink, BodyState::Missing);
        let account = worker.account_of(id).unwrap();
        assert_eq!(worker.composing.get(&id).map(Vec::len), Some(1));
        let wants = &worker.live[&account].wants;
        assert!(wants.bodies.contains(&id));
        assert!(wants.whole.contains(&id), "attachments need the whole body");
        assert!(said(&sink).is_empty(), "{:?}", said(&sink));
        assert!(worker.store.drafts.list().unwrap().is_empty());
    }

    #[test]
    fn the_fetched_body_opens_the_import_and_a_failed_fetch_refuses() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, id) = editing(dir.path(), &sink, BodyState::Missing);
        worker.compose_body_fetched(id, Some(RAW), true);
        let ready = said(&sink).into_iter().find_map(|e| match e {
            Event::ComposeReady { draft, note, .. } => Some((draft, note)),
            _ => None,
        });
        let (draft, note) = ready.expect("ComposeReady");
        assert_eq!(draft.body.trim_end(), "server body");
        assert_eq!(note, "");
        assert_eq!(worker.store.drafts.list().unwrap().len(), 1);

        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, id) = editing(dir.path(), &sink, BodyState::Missing);
        worker.compose_body_fetched(id, None, false);
        assert!(
            said(&sink)
                .iter()
                .any(|e| matches!(e, Event::ComposeRefused { .. })),
            "{:?}",
            said(&sink)
        );
        assert!(worker.store.drafts.list().unwrap().is_empty());
    }

    #[test]
    fn a_partial_body_asks_again_for_the_whole_one() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, id) = editing(dir.path(), &sink, BodyState::Missing);
        worker
            .store
            .db
            .set_body_state(id, BodyState::Partial)
            .unwrap();
        worker.compose_body_fetched(id, Some(RAW), false);
        assert!(worker.whole_again.contains(&id));
        assert!(said(&sink).is_empty(), "{:?}", said(&sink));
        assert!(worker.store.drafts.list().unwrap().is_empty());
    }
}
