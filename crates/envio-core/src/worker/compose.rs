//! Compose: starting a window, saving drafts, closing, and Send.
//! A draft is saved through `Drafts` (outside the cache); Send writes the
//! Outbox first and deletes the draft only after that worked.

use std::path::Path;

use envio_mime::{Address, OutgoingAttachment, content_type_for};

use super::Worker;
use super::start::displayed;
use crate::api::command::{ComposeId, DraftInput, StartFrom};
use crate::api::event::Event;
use crate::clock::now_ms;
use crate::compose::{self, ComposeError};
use crate::text;

fn refusal(error: &ComposeError) -> String {
    match error {
        ComposeError::NoRecipients => text::compose::no_recipients(),
        ComposeError::BadAddress(address) => text::compose::bad_address(address),
        ComposeError::NoAccount => text::compose::no_account(),
    }
}

/// A file to attach: its name and bytes.
fn read_attachment(path: &Path) -> std::io::Result<OutgoingAttachment> {
    let filename = path
        .file_name()
        .map_or_else(|| "attachment".into(), |n| n.to_string_lossy().into_owned());
    Ok(OutgoingAttachment {
        content_type: content_type_for(&filename).into(),
        filename,
        bytes: std::fs::read(path)?,
    })
}

impl Worker<'_> {
    pub(super) fn compose_refused(&self, compose: ComposeId, text: String) {
        self.emit(Event::ComposeRefused { compose, text });
    }

    pub(super) fn start_compose(
        &mut self,
        compose: ComposeId,
        from: StartFrom,
        (draft_id, mailto): (Option<&str>, Option<&str>),
    ) {
        match from {
            StartFrom::New { account } => {
                if self.store.accounts.get(&account).is_none() {
                    return self.compose_refused(compose, text::compose::no_account());
                }
                let draft = DraftInput {
                    account,
                    ..DraftInput::default()
                };
                self.compose_ready(compose, draft, String::new());
            }
            StartFrom::Draft => self.start_from_draft(compose, draft_id),
            StartFrom::Mailto => self.start_from_mailto(compose, mailto),
            StartFrom::Reply { id, .. } | StartFrom::Forward { id } => {
                self.start_from_message(compose, id, from);
            }
        }
    }

    pub(super) fn compose_ready(&self, compose: ComposeId, draft: DraftInput, quoted: String) {
        self.book.opened(compose, &draft.account);
        self.emit(Event::ComposeReady {
            compose,
            draft,
            quoted,
        });
    }

    fn start_from_draft(&mut self, compose: ComposeId, draft_id: Option<&str>) {
        let loaded = draft_id.and_then(|id| match self.store.drafts.load(id) {
            Ok(draft) => Some(compose::from_store(id, &draft)),
            Err(e) => {
                log::warn!("opening a draft: {e}");
                None
            }
        });
        match (draft_id, loaded) {
            (Some(id), Some(draft)) => {
                self.book.adopt(compose, id.to_owned());
                self.compose_ready(compose, draft, String::new());
            }
            _ => self.compose_refused(compose, text::compose::draft_unavailable()),
        }
    }

    fn start_from_mailto(&mut self, compose: ComposeId, mailto: Option<&str>) {
        let first = self
            .store
            .accounts
            .list()
            .iter()
            .find(|a| a.broken.is_none());
        let Some(account) = first.map(|a| a.key.clone()) else {
            return self.compose_refused(compose, text::compose::no_account());
        };
        let Some(parsed) = mailto.and_then(envio_mime::parse_mailto) else {
            return self.compose_refused(compose, text::compose::bad_address(mailto.unwrap_or("")));
        };
        let draft = DraftInput {
            account,
            to: displayed(&parsed.to),
            cc: displayed(&parsed.cc),
            bcc: displayed(&parsed.bcc),
            subject: parsed.subject,
            body: parsed.body,
            in_reply_to: parsed.in_reply_to,
            ..DraftInput::default()
        };
        self.compose_ready(compose, draft, String::new());
    }

    /// The window closed: a reply still waiting for its body is forgotten.
    /// The draft itself is the draft thread's.
    pub(super) fn forget_waiting(&mut self, compose: ComposeId) {
        self.composing.retain(|_, waiting| {
            waiting.retain(|(c, _)| *c != compose);
            !waiting.is_empty()
        });
        let composing = &self.composing;
        self.whole_again.retain(|id| composing.contains_key(id));
        // A whole fetch queued for a forward nobody waits for any more is
        // withdrawn (one already running finishes, its result is dropped).
        for live in self.live.values_mut() {
            let gone: Vec<envio_store::MessageRowId> = live
                .wants
                .whole
                .iter()
                .copied()
                .filter(|id| !composing.contains_key(id))
                .collect();
            for id in gone {
                live.wants.whole.remove(&id);
                if !self.opening.contains_key(&id) {
                    live.wants.bodies.retain(|b| *b != id);
                }
            }
        }
    }

    pub(super) fn send(&mut self, compose: ComposeId, draft: &DraftInput) {
        if self.book.is_sent(compose) {
            return;
        }
        match self.try_send(compose, draft) {
            Ok(item) => {
                self.emit(Event::Queued {
                    compose,
                    item,
                    text: text::compose::queued(),
                });
                self.emit(Event::OutboxChanged);
                self.emit_outbox();
            }
            Err(text) => {
                // The window stays open: keep what the user wrote.
                self.book.requeue(compose, draft.clone(), now_ms());
                self.compose_refused(compose, text);
            }
        }
    }

    /// Queues the message; the item's id, or the text of the refusal.
    fn try_send(&mut self, compose: ComposeId, draft: &DraftInput) -> Result<String, String> {
        let Some(settings) = self
            .store
            .accounts
            .get(&draft.account)
            .map(|a| a.settings.clone())
        else {
            return Err(refusal(&ComposeError::NoAccount));
        };
        let from = Address::new(
            Some(&settings.name)
                .filter(|n| !n.is_empty())
                .map(String::as_str),
            &settings.email,
        );
        // What this Send carries is on disk before anything is built.
        self.book.changed(compose, draft.clone(), now_ms());
        self.book.flush(compose)?;
        let mut files = Vec::new();
        for path in &draft.attachments {
            match read_attachment(path) {
                Ok(file) => files.push(file),
                Err(e) => {
                    log::warn!("reading an attachment: {e}");
                    let name = path
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                    return Err(text::compose::attachment_unreadable(&name));
                }
            }
        }
        let outgoing = match compose::to_outgoing(draft, from, files) {
            Ok(outgoing) => outgoing,
            Err(error) => return Err(refusal(&error)),
        };
        let now = now_ms();
        let built = envio_mime::build(
            &outgoing,
            now / 1000,
            &envio_mime::new_message_id(&settings.email),
        );
        let new = envio_store::NewOutboxItem {
            account: draft.account.clone(),
            message_id: envio_sync::convert::bracketed(&built.message_id),
            queued_at_ms: now,
            from: settings.email,
            recipients: built.recipients,
        };
        match self.store.outbox.enqueue(new, &built.bytes) {
            Ok(item) => {
                // Only now is the draft not needed any more: no more saves,
                // then its file and attachment copies go.
                if let Some(id) = self.book.sent(compose).or_else(|| draft.draft_id.clone())
                    && let Err(e) = self.store.drafts.delete(&id)
                {
                    log::warn!("deleting a sent draft: {e}");
                }
                if let Some(live) = self.live.get_mut(&draft.account) {
                    live.wants.send = true;
                }
                self.remember_recipients(draft);
                Ok(item.id)
            }
            Err(error) => {
                log::warn!("queueing a message: {error}");
                Err(text::compose::queue_failed())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use envio_store::BodyState;

    use super::super::testing::{Sink, open, seed};
    use super::*;

    const C: ComposeId = ComposeId(7);

    #[test]
    fn an_attachment_goes_out_with_the_type_of_its_name() {
        let dir = tempfile::tempdir().unwrap();
        for (name, want) in [
            ("a.png", "image/png"),
            ("b.bin", "application/octet-stream"),
        ] {
            let path = dir.path().join(name);
            std::fs::write(&path, b"x").unwrap();
            let got = read_attachment(&path).unwrap();
            assert_eq!(got.filename, name);
            assert_eq!(got.content_type, want);
        }
    }

    /// What compose said (the worker also announces itself at start).
    fn said(sink: &Sink) -> Vec<Event> {
        let mut events = sink.events();
        events.retain(|e| !matches!(e, Event::Started { .. } | Event::Settings(_)));
        events
    }

    fn waiting<'a>(
        dir: &std::path::Path,
        sink: &'a Sink,
    ) -> (Worker<'a>, String, envio_store::MessageRowId) {
        let (config, seeded) = seed(dir);
        let mut worker = open(&config, sink);
        worker
            .store
            .db
            .set_body_state(seeded.message, BodyState::Missing)
            .unwrap();
        worker.start_compose(
            C,
            StartFrom::Reply {
                id: seeded.message,
                all: false,
            },
            (None, None),
        );
        (worker, seeded.account, seeded.message)
    }

    #[test]
    fn a_reply_waits_for_its_body_and_closing_forgets_it() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, account, id) = waiting(dir.path(), &sink);
        assert_eq!(worker.composing.get(&id).map(Vec::len), Some(1));
        assert!(worker.live[&account].wants.bodies.contains(&id));
        worker.forget_waiting(C);
        assert!(worker.composing.is_empty());
        worker.compose_body_fetched(id, None, false);
        assert!(said(&sink).is_empty());
    }

    const WITH_PDF: &[u8] = b"From: a@example.org\r\nSubject: s\r\nMIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=XX\r\n\r\n\
--XX\r\nContent-Type: text/plain\r\n\r\nhi\r\n\
--XX\r\nContent-Type: application/pdf; name=\"big.pdf\"\r\n\
Content-Disposition: attachment; filename=\"big.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\r\naGVsbG8=\r\n--XX--\r\n";

    /// A forward of the seeded (big) message waits for its body, which is
    /// `state` in the cache.
    fn forwarding<'a>(
        dir: &std::path::Path,
        sink: &'a Sink,
        state: BodyState,
    ) -> (Worker<'a>, envio_store::MessageRowId) {
        let (config, seeded) = seed(dir);
        let mut worker = open(&config, sink);
        let id = seeded.message;
        worker.store.db.set_body_state(id, state).unwrap();
        let big = envio_store::NewMessage {
            uid: 1,
            size: envio_sync::driver::PARTIAL_ABOVE + 1,
            envelope: envio_store::Envelope::default(),
            flags: envio_store::Flags::default(),
            preview: String::new(),
        };
        worker.store.db.upsert_message(seeded.folder, &big).unwrap();
        worker.start_compose(C, StartFrom::Forward { id }, (None, None));
        assert_eq!(worker.composing.get(&id).map(Vec::len), Some(1));
        (worker, id)
    }

    #[test]
    fn a_forward_of_a_big_message_keeps_its_attachments() {
        for state in [BodyState::Missing, BodyState::Partial] {
            let dir = tempfile::tempdir().unwrap();
            let sink = Sink::default();
            let (mut worker, id) = forwarding(dir.path(), &sink, state);
            if state == BodyState::Partial {
                // What the cache holds: the text and a stub of the file.
                let message = worker.store.db.message(id).unwrap();
                let stub =
                    b"From: a@example.org\r\nSubject: s\r\nContent-Type: text/plain\r\n\r\nhi\r\n";
                worker
                    .store
                    .bodies
                    .write(message.folder, message.uid, stub)
                    .unwrap();
            }
            worker.compose_body_fetched(id, Some(WITH_PDF), true);
            let events = said(&sink);
            assert!(
                events.iter().any(|e| matches!(e, Event::Attached { .. })),
                "{state:?}: {events:?}"
            );
            assert!(
                !events
                    .iter()
                    .any(|e| matches!(e, Event::AttachFailed { .. })),
                "{state:?}: {events:?}"
            );
        }
    }

    #[test]
    fn closing_a_forward_waiting_for_the_whole_message_forgets_the_ask() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, id) = forwarding(dir.path(), &sink, BodyState::Partial);
        worker.whole_again.insert(id);
        worker.forget_waiting(C);
        assert!(worker.whole_again.is_empty());
    }

    #[test]
    fn closing_a_forward_withdraws_a_whole_fetch_not_yet_started() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, id) = forwarding(dir.path(), &sink, BodyState::Missing);
        let account = worker.account_of(id).unwrap();
        assert!(worker.live[&account].wants.whole.contains(&id));
        worker.forget_waiting(C);
        let wants = &worker.live[&account].wants;
        assert!(!wants.whole.contains(&id), "the whole fetch stays queued");
        assert!(!wants.bodies.contains(&id));
    }

    #[test]
    fn closing_a_forward_keeps_the_fetch_an_open_reader_waits_for() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, id) = forwarding(dir.path(), &sink, BodyState::Missing);
        let account = worker.account_of(id).unwrap();
        worker
            .opening
            .insert(id, (crate::api::command::ImagesChoice::Default, 300));
        worker.forget_waiting(C);
        let wants = &worker.live[&account].wants;
        assert!(wants.bodies.contains(&id));
        assert!(!wants.whole.contains(&id));
    }

    #[test]
    fn a_failed_body_or_a_removed_account_answers_a_waiting_reply() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, account, id) = waiting(dir.path(), &sink);
        worker.compose_failed(Some(&account));
        assert!(worker.composing.is_empty());
        assert!(!worker.live[&account].wants.bodies.contains(&id));
        assert!(matches!(
            said(&sink).as_slice(),
            [Event::ComposeRefused { compose, .. }] if *compose == C
        ));
    }
}
