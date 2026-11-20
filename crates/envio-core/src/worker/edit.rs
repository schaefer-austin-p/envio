//! Edit: reopening a message of a Drafts-role folder in Compose (#228).
//! The local file wins over the server text (Decision 3); a message with no
//! local file is imported from the server (Decision 4). The import waits for
//! the whole body as Forward does, so attachments come along.

use envio_store::{FolderRole, MessageRowId};

use super::Worker;
use crate::api::command::{ComposeId, StartFrom};
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
        self.edit_checked(compose, id, (raw, None));
    }

    /// `edit_or_import`, with `live` the answer of the keyword check once
    /// it is made: `Some(None)` when the server could not say (see
    /// `edit_keywords`).
    pub(super) fn edit_checked(
        &mut self,
        compose: ComposeId,
        id: MessageRowId,
        (raw, live): (Option<&[u8]>, Option<Option<&[String]>>),
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
            Some(raw) => self.import_or_check(compose, (&message, &folder), (raw, live)),
            None => self.start_from_message(compose, id, StartFrom::Edit { id }),
        }
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
        let folder = worker.store.db.message(id).unwrap().folder;
        worker.store.db.set_keywords(folder, 1, &[]).unwrap();
        worker.compose_body_fetched(id, Some(RAW), true);
        worker.keywords_checked(id, Some(Vec::new()));
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

    /// An Edit whose body arrived, the cache knowing no keywords: parked
    /// for the live check.
    fn parked<'a>(dir: &std::path::Path, sink: &'a Sink) -> (Worker<'a>, MessageRowId, String) {
        let (mut worker, id) = editing(dir, sink, BodyState::Missing);
        let folder = worker.store.db.message(id).unwrap().folder;
        worker.store.db.set_keywords(folder, 1, &[]).unwrap();
        worker.compose_body_fetched(id, Some(RAW), true);
        let account = worker.account_of(id).unwrap();
        (worker, id, account)
    }

    fn linked(worker: &Worker<'_>) -> Option<bool> {
        let drafts = worker.store.drafts.list().unwrap();
        let (_, draft) = drafts.first()?;
        Some(draft.server.message_id.as_deref() == Some(MID))
    }

    #[test]
    fn an_edit_that_could_link_waits_for_the_live_keyword_check() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (worker, id, account) = parked(dir.path(), &sink);
        assert_eq!(worker.awaiting_keywords.get(&id).map(Vec::len), Some(1));
        assert!(worker.live[&account].wants.keywords.contains(&id));
        assert!(said(&sink).is_empty(), "{:?}", said(&sink));
        assert_eq!(linked(&worker), None, "nothing is imported yet");
    }

    #[test]
    fn unknown_keywords_detach_at_once_without_a_check() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, id) = editing(dir.path(), &sink, BodyState::Missing);
        worker.compose_body_fetched(id, Some(RAW), true);
        let account = worker.account_of(id).unwrap();
        assert!(worker.awaiting_keywords.is_empty());
        assert!(worker.live[&account].wants.keywords.is_empty());
        assert_eq!(linked(&worker), Some(false));
    }

    #[test]
    fn only_an_empty_fresh_list_links_the_parked_edit() {
        for (fresh, want) in [
            (Some(vec![]), true),
            (Some(vec!["$Forwarded".to_owned()]), false),
            (None, false),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let sink = Sink::default();
            let (mut worker, id, account) = parked(dir.path(), &sink);
            worker.keywords_checked(id, fresh.clone());
            assert_eq!(linked(&worker), Some(want), "{fresh:?}");
            assert!(worker.awaiting_keywords.is_empty());
            assert!(worker.live[&account].wants.keywords.is_empty());
            assert!(
                said(&sink)
                    .iter()
                    .any(|e| matches!(e, Event::ComposeReady { .. })),
                "{:?}",
                said(&sink)
            );
        }
    }

    #[test]
    fn a_failed_check_detaches_the_parked_edit_and_never_refuses_it() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, id, account) = parked(dir.path(), &sink);
        worker.keywords_failed(Some(&account));
        assert_eq!(linked(&worker), Some(false));
        assert!(worker.live[&account].wants.keywords.is_empty());
        let said = said(&sink);
        assert!(said.iter().any(|e| matches!(e, Event::ComposeReady { .. })));
        assert!(
            !said
                .iter()
                .any(|e| matches!(e, Event::ComposeRefused { .. }))
        );
        assert!(!worker.awaiting_keywords.contains_key(&id));
    }

    #[test]
    fn a_closed_window_stops_waiting_for_the_check() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, id, account) = parked(dir.path(), &sink);
        worker.forget_waiting(C);
        assert!(worker.awaiting_keywords.is_empty());
        assert!(!worker.live[&account].wants.keywords.contains(&id));
    }
}
