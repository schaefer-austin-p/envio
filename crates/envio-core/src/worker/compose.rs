//! Compose: starting a window, saving drafts, closing, and Send.
//! A draft is saved through `Drafts` (outside the cache); Send writes the
//! Outbox first and deletes the draft only after that worked.

use std::path::Path;

use envio_mime::{Address, OutgoingAttachment};

use super::Worker;
use super::start::displayed;
use crate::api::command::{ComposeId, DraftInput, StartFrom};
use crate::api::event::Event;
use crate::clock::now_ms;
use crate::compose::{self, ComposeError};
use crate::problem::{self, Problem};
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
    Ok(OutgoingAttachment {
        filename: path
            .file_name()
            .map_or_else(|| "attachment".into(), |n| n.to_string_lossy().into_owned()),
        content_type: "application/octet-stream".into(),
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
                self.drafts_of.insert(compose, id.to_owned());
                self.compose_ready(compose, draft, String::new());
            }
            _ => self.compose_refused(compose, text::compose::draft_unavailable()),
        }
    }

    fn start_from_mailto(&mut self, compose: ComposeId, mailto: Option<&str>) {
        let Some(account) = self.store.accounts.list().first().map(|a| a.key.clone()) else {
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

    /// An account is going: its pending draft saves must not bring it back.
    pub(super) fn drop_composes_of(&mut self, account: &str) {
        self.throttle.drop_account(account);
    }

    pub(super) fn draft_changed(&mut self, compose: ComposeId, draft: DraftInput) {
        // A sent message must not come back as a draft, nor a removed
        // account's.
        if self.sent.contains(&compose) || self.store.accounts.get(&draft.account).is_none() {
            return;
        }
        self.throttle.changed(compose, draft, now_ms());
    }

    /// Saves the draft; when that fails it stays pending and is tried again
    /// a second later, so closing the window afterwards still saves it.
    fn persist(&mut self, compose: ComposeId, draft: DraftInput) -> bool {
        let saved = self.save_draft(compose, &draft);
        if saved {
            self.save_failed.remove(&compose);
        } else {
            self.throttle.retry(compose, draft, now_ms());
        }
        saved
    }

    /// Writes the draft: created the first time, replaced after. False when
    /// the write failed (the first failure of a run tells the user).
    fn save_draft(&mut self, compose: ComposeId, draft: &DraftInput) -> bool {
        if self.store.accounts.get(&draft.account).is_none() {
            return true;
        }
        let stored = compose::to_store(draft, now_ms());
        let known = self
            .drafts_of
            .get(&compose)
            .cloned()
            .or_else(|| draft.draft_id.clone());
        let result = match &known {
            Some(id) => self.store.drafts.save(id, &stored).map(|()| None),
            None => self.store.drafts.create(&stored).map(Some),
        };
        match result {
            Ok(created) => {
                if let Some(id) = known.or_else(|| created.clone()) {
                    self.drafts_of.insert(compose, id);
                }
                if let Some(draft_id) = created {
                    self.emit(Event::DraftSaved { compose, draft_id });
                }
                true
            }
            Err(error) => {
                // Once per run of failures: it is retried every second.
                if self.save_failed.insert(compose) {
                    log::warn!("saving a draft: {error}");
                    let text = match problem::from_store(&error) {
                        Problem::DiskFull => text::notice::disk_full(),
                        _ => text::compose::draft_not_saved(),
                    };
                    self.emit(Event::Notice { text });
                }
                false
            }
        }
    }

    /// Saves every draft still waiting for its second.
    pub(super) fn flush_drafts(&mut self) {
        for (compose, draft) in self.throttle.take_all() {
            self.persist(compose, draft);
        }
    }

    pub(super) fn save_due_drafts(&mut self, now: i64) {
        for (compose, draft) in self.throttle.due(now) {
            self.persist(compose, draft);
        }
    }

    pub(super) fn close_compose(&mut self, compose: ComposeId, discard: bool) {
        let pending = self.throttle.take(compose);
        self.composing.retain(|_, waiting| {
            waiting.retain(|(c, _)| *c != compose);
            !waiting.is_empty()
        });
        let was_sent = self.sent.contains(&compose);
        self.save_failed.remove(&compose);
        if discard {
            self.delete_draft(
                compose,
                pending.as_ref().and_then(|d| d.draft_id.as_deref()),
            );
        } else {
            let saved = match pending.filter(|_| !was_sent) {
                Some(draft) => self.persist(compose, draft),
                None => true,
            };
            if saved {
                self.drafts_of.remove(&compose);
            }
        }
    }

    fn delete_draft(&mut self, compose: ComposeId, fallback: Option<&str>) {
        let id = self.drafts_of.remove(&compose);
        if let Some(id) = id.as_deref().or(fallback)
            && let Err(e) = self.store.drafts.delete(id)
        {
            log::warn!("deleting a draft: {e}");
        }
    }

    pub(super) fn send(&mut self, compose: ComposeId, draft: &DraftInput) {
        if self.sent.contains(&compose) {
            return;
        }
        match self.try_send(compose, draft) {
            Ok(item) => {
                self.sent.insert(compose);
                self.emit(Event::Queued { compose, item });
                self.emit(Event::OutboxChanged);
            }
            Err(text) => {
                // The window stays open: keep what the user wrote.
                self.draft_changed(compose, draft.clone());
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
                // Only now is the draft not needed any more.
                self.throttle.take(compose);
                self.delete_draft(compose, draft.draft_id.as_deref());
                if let Some(live) = self.live.get_mut(&draft.account) {
                    live.wants.send = true;
                }
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

    /// What compose said (the worker also announces itself at start).
    fn said(sink: &Sink) -> Vec<Event> {
        let mut events = sink.events();
        events.retain(|e| !matches!(e, Event::Started { .. }));
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
        worker.close_compose(C, true);
        assert!(worker.composing.is_empty());
        worker.compose_body_fetched(id, None);
        assert!(said(&sink).is_empty());
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
