//! The Outbox for the app: the list, the user's decisions, and Edit.
//! Sending states are written by envio-sync; only `decide_outbox` below
//! changes an existing item, and only as `outbox::decide` allows.

use std::path::PathBuf;

use envio_store::{OutboxItem, SendState};
use envio_sync::schedule::Key;

use super::Worker;
use crate::api::command::{ComposeId, OutboxDecision};
use crate::api::event::Event;
use crate::api::view::{OutboxRow, OutboxStatus};
use crate::clock::now_ms;
use crate::outbox::{self, Decided};
use crate::text;

impl Worker<'_> {
    fn notice(&self, text: String) {
        self.emit(Event::Notice { text });
    }

    /// Every item, oldest first, then the ones that can not be read.
    fn outbox_rows(&self) -> Vec<OutboxRow> {
        let mut rows = Vec::new();
        match self.store.outbox.items() {
            Ok(items) => rows.extend(items.iter().map(|item| self.row(item))),
            Err(e) => log::warn!("listing the Outbox: {e}"),
        }
        match self.store.outbox.damaged() {
            Ok(ids) => rows.extend(ids.into_iter().map(damaged_row)),
            Err(e) => log::warn!("listing damaged Outbox items: {e}"),
        }
        rows
    }

    fn row(&self, item: &OutboxItem) -> OutboxRow {
        let asked = self.asked.contains(&item.id);
        let read = self
            .store
            .outbox
            .eml(&item.id)
            .ok()
            .and_then(|eml| envio_mime::envelope(&eml));
        let Some(envelope) = read else {
            return damaged_row(item.id.clone());
        };
        let status = outbox::status(&item.meta.state, asked);
        OutboxRow {
            id: item.id.clone(),
            account: item.meta.account.clone(),
            subject: envelope.subject,
            recipients: item.meta.recipients.clone(),
            queued_at_ms: item.meta.queued_at_ms,
            text: text::outbox::status(&status),
            status,
        }
    }

    pub(super) fn emit_outbox(&self) {
        self.emit(Event::Outbox {
            items: self.outbox_rows(),
        });
    }

    fn find_item(&self, id: &str) -> Option<OutboxItem> {
        match self.store.outbox.items() {
            Ok(items) => items.into_iter().find(|i| i.id == id),
            Err(e) => {
                log::warn!("reading the Outbox: {e}");
                None
            }
        }
    }

    /// The item changed or is gone: forget what the session knew of it.
    fn forget_item(&mut self, id: &str) {
        self.asked.remove(id);
        self.unfiled.remove(id);
        self.store.schedule.succeeded(&Key::Outbox(id.to_owned()));
    }

    /// An account is going: its items go with it.
    pub(super) fn drop_outbox_of(&mut self, account: &str) {
        let ids: Vec<String> = self
            .store
            .outbox
            .items()
            .unwrap_or_default()
            .into_iter()
            .filter(|i| i.meta.account == account)
            .map(|i| i.id)
            .collect();
        for id in ids {
            self.forget_item(&id);
        }
    }

    pub(super) fn decide_outbox(&mut self, id: &str, decision: OutboxDecision) {
        let item = self.find_item(id).filter(|i| self.readable(&i.id));
        let Some(item) = item else {
            return self.decide_unreadable(id, decision);
        };
        let asked = self.asked.contains(id);
        match outbox::decide(&item.meta.state, decision, asked) {
            Err(outbox::NotNow) => return self.notice(text::outbox::not_now()),
            Ok(Decided::Remove) => self.remove_item(id),
            Ok(Decided::SetState(state)) => {
                if let Err(e) = self.store.outbox.set_state(id, state.clone()) {
                    log::warn!("changing an Outbox item: {e}");
                    return self.notice(text::outbox::change_failed());
                }
                self.forget_item(id);
                if let Some(live) = self.live.get_mut(&item.meta.account) {
                    // Accepted is only filed in Sent; the rest is sent.
                    if state == SendState::Accepted {
                        live.wants.file = true;
                    } else {
                        live.wants.send = true;
                    }
                }
            }
        }
        self.emit_outbox();
        self.reshow_open_invite();
    }

    /// The item's message can be read (an unreadable one shows as damaged).
    fn readable(&self, id: &str) -> bool {
        self.store
            .outbox
            .eml(id)
            .ok()
            .is_some_and(|eml| envio_mime::envelope(&eml).is_some())
    }

    /// An item that can not be read (files damaged, or the message does not
    /// parse): only Delete is possible.
    fn decide_unreadable(&mut self, id: &str, decision: OutboxDecision) {
        let damaged = self.find_item(id).is_some()
            || self
                .store
                .outbox
                .damaged()
                .is_ok_and(|ids| ids.iter().any(|d| d == id));
        if damaged && decision == OutboxDecision::Delete {
            self.remove_item(id);
            self.emit_outbox();
        } else {
            self.notice(text::outbox::not_now());
        }
    }

    /// Delete, or Edit (which turns the item into a draft): the item goes
    /// and, if it was an invite reply, so does the remembered answer.
    fn remove_item(&mut self, id: &str) {
        let message_id = self.find_item(id).map(|i| i.meta.message_id);
        match self.store.outbox.remove(id) {
            Ok(()) => {
                self.forget_item(id);
                if let Some(message_id) = message_id {
                    self.forget_answer_of_reply(&message_id);
                }
            }
            Err(e) => {
                log::warn!("removing an Outbox item: {e}");
                self.notice(text::outbox::change_failed());
            }
        }
    }

    /// Edit: a refused message becomes a draft in `compose`. The item goes
    /// only after the draft is stored, so a crash leaves both, never none.
    pub(super) fn edit_outbox(&mut self, id: &str, compose: ComposeId) {
        let Some(item) = self.find_item(id) else {
            return self.notice(text::outbox::not_now());
        };
        let editable = matches!(item.meta.state, SendState::Failed { .. });
        let account_known = self.store.accounts.get(&item.meta.account).is_some();
        let free = !self.book.is_sent(compose) && self.book.id_of(compose).is_none();
        let eml = (editable && free && account_known)
            .then(|| self.store.outbox.eml(id).ok())
            .flatten();
        let Some(eml) = eml else {
            return self.notice(text::outbox::not_now());
        };
        if outbox::is_invite_reply(&eml) {
            return self.notice(text::outbox::edit_invite_reply());
        }
        let draft = outbox::draft_from_eml(&item.meta.account, &eml, &item.meta.recipients);
        let Some(mut draft) = draft else {
            return self.notice(text::outbox::not_now());
        };
        let stored = crate::compose::to_store(&draft, now_ms());
        let draft_id = match self.store.drafts.create(&stored) {
            Ok(draft_id) => draft_id,
            Err(e) => {
                log::warn!("saving a draft from the Outbox: {e}");
                return self.notice(text::compose::draft_not_saved());
            }
        };
        // The message's attachments go into the draft's own files folder
        // and the draft names them. Until that is stored the item stays,
        // and a failure deletes the half-made draft.
        match self.keep_attachments_of(&draft_id, &eml, stored) {
            Ok(paths) => draft.attachments = paths,
            Err(e) => {
                log::warn!("keeping an Outbox item's attachments: {e}");
                if let Err(e) = self.store.drafts.delete(&draft_id) {
                    log::warn!("deleting the half-made draft: {e}");
                }
                return self.notice(text::compose::draft_not_saved());
            }
        }
        self.book.adopt(compose, draft_id.clone());
        draft.draft_id = Some(draft_id.clone());
        self.compose_ready(compose, draft, String::new());
        self.emit(Event::DraftSaved { compose, draft_id });
        self.remove_item(id);
        self.emit_outbox();
        self.reshow_open_invite();
    }

    /// Writes the attachments of `eml` into the draft's files folder and
    /// saves the draft naming them; returns their paths.
    fn keep_attachments_of(
        &self,
        draft_id: &str,
        eml: &[u8],
        mut stored: envio_store::Draft,
    ) -> Result<Vec<PathBuf>, String> {
        let envio_mime::Message::Parsed { attachments, .. } = envio_mime::parse(eml) else {
            return Ok(Vec::new());
        };
        if attachments.is_empty() {
            return Ok(Vec::new());
        }
        let indexes: Vec<usize> = attachments.iter().map(|a| a.index).collect();
        let bytes = envio_mime::attachments_bytes(eml, &indexes);
        let mut paths = Vec::new();
        for (info, bytes) in attachments.iter().zip(bytes) {
            let bytes = bytes.ok_or("an attachment has no bytes")?;
            let name = info.filename.as_deref().unwrap_or("attachment");
            let path = self
                .store
                .drafts
                .keep_bytes(draft_id, name, &bytes)
                .map_err(|e| e.to_string())?;
            paths.push(path);
        }
        stored.attachments.clone_from(&paths);
        self.store
            .drafts
            .save(draft_id, &stored)
            .map_err(|e| e.to_string())?;
        Ok(paths)
    }
}

fn damaged_row(id: String) -> OutboxRow {
    OutboxRow {
        id,
        account: String::new(),
        subject: String::new(),
        recipients: Vec::new(),
        queued_at_ms: 0,
        text: text::outbox::status(&OutboxStatus::Damaged),
        status: OutboxStatus::Damaged,
    }
}

#[cfg(test)]
mod tests {
    use envio_store::{NewOutboxItem, Outbox};

    use super::super::testing::{Sink, open, seed};
    use super::*;

    const EML: &[u8] = b"From: me@example.org\r\nTo: ada@example.org\r\nSubject: s\r\n\r\nx\r\n";
    const WITH_FILE: &[u8] = b"From: me@example.org\r\nTo: ada@example.org\r\nSubject: s\r\n\
MIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=b\r\n\r\n--b\r\n\
Content-Type: text/plain\r\n\r\nhi\r\n--b\r\nContent-Type: application/pdf; name=a.pdf\r\n\
Content-Disposition: attachment; filename=a.pdf\r\n\
Content-Transfer-Encoding: base64\r\n\r\nAAAA\r\n--b--\r\n";
    const C: ComposeId = ComposeId(3);

    struct T {
        dir: tempfile::TempDir,
        sink: &'static Sink,
        account: String,
        config: crate::core::Config,
    }

    fn setup() -> T {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        T {
            dir,
            sink: Box::leak(Box::default()),
            account: seeded.account,
            config,
        }
    }

    /// Queues `eml` in `state`. (Seeding only: the state is set through a
    /// path, so the grep test's single call stays the only call.)
    fn item(w: &Worker<'_>, t: &T, eml: &[u8], state: SendState) -> String {
        let new = NewOutboxItem {
            send_after_ms: 0,
            account: t.account.clone(),
            message_id: "<a@example.org>".into(),
            queued_at_ms: 1,
            from: "me@example.org".into(),
            recipients: vec!["ada@example.org".into()],
        };
        let id = w.store.outbox.enqueue(new, eml).unwrap().id;
        let seed_state = Outbox::set_state;
        seed_state(&w.store.outbox, &id, state).unwrap();
        id
    }

    fn state(w: &Worker<'_>, id: &str) -> Option<SendState> {
        w.find_item(id).map(|i| i.meta.state)
    }

    fn failed() -> SendState {
        SendState::Failed { reason: "r".into() }
    }

    fn notices(t: &T) -> usize {
        let events = t.sink.events();
        events
            .iter()
            .filter(|e| matches!(e, Event::Notice { .. }))
            .count()
    }

    #[test]
    fn an_asked_item_can_be_sent_again_and_stops_being_asked() {
        let t = setup();
        let mut w = open(&t.config, t.sink);
        let id = item(&w, &t, EML, SendState::MayHaveBeenSent);
        w.asked.insert(id.clone());
        w.decide_outbox(&id, OutboxDecision::SendAgain);
        assert_eq!(state(&w, &id), Some(SendState::Queued));
        assert!(w.live[&t.account].wants.send);
        assert!(!w.live[&t.account].wants.file);
        assert!(!w.asked.contains(&id));
    }

    #[test]
    fn move_to_sent_files_and_never_sends() {
        let t = setup();
        let mut w = open(&t.config, t.sink);
        let id = item(&w, &t, EML, SendState::MayHaveBeenSent);
        w.asked.insert(id.clone());
        w.decide_outbox(&id, OutboxDecision::MoveToSent);
        assert_eq!(state(&w, &id), Some(SendState::Accepted));
        assert!(w.live[&t.account].wants.file);
        assert!(!w.live[&t.account].wants.send);
        assert!(w.asked.is_empty());
    }

    #[test]
    fn an_asked_item_can_be_deleted_and_is_forgotten() {
        let t = setup();
        let mut w = open(&t.config, t.sink);
        let id = item(&w, &t, EML, SendState::MayHaveBeenSent);
        w.asked.insert(id.clone());
        w.unfiled.insert(id.clone());
        w.decide_outbox(&id, OutboxDecision::Delete);
        assert_eq!(state(&w, &id), None);
        assert!(w.asked.is_empty() && w.unfiled.is_empty());
    }

    #[test]
    fn forgetting_an_item_clears_its_backoff() {
        let t = setup();
        let mut w = open(&t.config, t.sink);
        let id = item(&w, &t, EML, failed());
        let key = Key::Outbox(id.clone());
        w.store.schedule.failed(key.clone(), 0);
        w.decide_outbox(&id, OutboxDecision::Retry);
        assert_eq!(w.store.schedule.failures(&key), 0);
    }

    #[test]
    fn removing_an_account_forgets_its_asked_items() {
        let t = setup();
        let mut w = open(&t.config, t.sink);
        let id = item(&w, &t, EML, SendState::MayHaveBeenSent);
        w.asked.insert(id);
        w.drop_outbox_of("someone-else");
        assert_eq!(w.asked.len(), 1);
        w.drop_outbox_of(&t.account);
        assert!(w.asked.is_empty());
    }

    #[test]
    fn an_unparseable_item_accepts_only_delete() {
        let t = setup();
        let mut w = open(&t.config, t.sink);
        let id = item(&w, &t, b"", failed());
        assert!(!w.readable(&id));
        w.decide_outbox(&id, OutboxDecision::Retry);
        assert_eq!(state(&w, &id), Some(failed()));
        assert_eq!(notices(&t), 1);
        w.decide_outbox(&id, OutboxDecision::Delete);
        assert_eq!(state(&w, &id), None);
    }

    #[test]
    fn edit_stores_the_draft_before_it_removes_the_item() {
        let t = setup();
        let mut w = open(&t.config, t.sink);
        let id = item(&w, &t, EML, failed());
        // Storing the draft fails: the item must still be there.
        let drafts = t.dir.path().join("data").join("drafts");
        std::fs::remove_dir_all(&drafts).unwrap();
        std::fs::write(&drafts, b"in the way").unwrap();
        w.edit_outbox(&id, C);
        assert_eq!(state(&w, &id), Some(failed()));
        assert!(w.book.id_of(C).is_none());
        assert_eq!(notices(&t), 1);
    }

    #[test]
    fn edit_keeps_a_named_calendar_file_the_user_attached() {
        let t = setup();
        let mut w = open(&t.config, t.sink);
        let eml: &[u8] = b"From: me@example.org\r\nTo: ada@example.org\r\nSubject: s\r\n\
MIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=b\r\n\r\n--b\r\n\
Content-Type: text/plain\r\n\r\nhi\r\n--b\r\n\
Content-Type: text/calendar; name=meeting.ics\r\n\
Content-Disposition: attachment; filename=meeting.ics\r\n\r\n\
BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n--b--\r\n";
        let id = item(&w, &t, eml, failed());
        w.edit_outbox(&id, C);
        assert_eq!(state(&w, &id), None);
        let drafts = w.store.drafts.list().unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].1.attachments.len(), 1);
        assert_eq!(
            drafts[0].1.attachments[0].file_name().unwrap(),
            "meeting.ics"
        );
    }

    #[test]
    fn edit_keeps_a_forwarded_invite_that_quotes_method_reply() {
        let t = setup();
        let mut w = open(&t.config, t.sink);
        let eml: &[u8] = b"From: me@example.org\r\nTo: ada@example.org\r\nSubject: Fwd\r\n\
MIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary=b\r\n\r\n--b\r\n\
Content-Type: text/plain\r\n\r\nthe header said method=reply once\r\n--b\r\n\
Content-Type: text/calendar; method=REQUEST; charset=utf-8\r\n\r\n\
BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nEND:VCALENDAR\r\n--b--\r\n";
        let id = item(&w, &t, eml, failed());
        w.edit_outbox(&id, C);
        assert_eq!(state(&w, &id), None);
        assert_eq!(w.store.drafts.list().unwrap().len(), 1);
    }

    #[test]
    fn edit_refuses_a_failed_invite_reply() {
        let t = setup();
        let mut w = open(&t.config, t.sink);
        let reply: &[u8] = b"From: me@example.org\r\nTo: ada@example.org\r\nSubject: Accepted\r\n\
MIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary=b\r\n\r\n--b\r\n\
Content-Type: text/plain\r\n\r\nAccepted\r\n--b\r\n\
Content-Type: text/calendar; method=REPLY; charset=utf-8\r\n\r\n\
BEGIN:VCALENDAR\r\nMETHOD:REPLY\r\nEND:VCALENDAR\r\n--b--\r\n";
        let id = item(&w, &t, reply, failed());
        w.edit_outbox(&id, C);
        // The item stays (it holds the answer), and no draft is made.
        assert!(state(&w, &id).is_some());
        assert_eq!(notices(&t), 1);
        assert!(w.book.id_of(C).is_none());
        assert!(w.store.drafts.list().unwrap().is_empty());
    }

    #[test]
    fn edit_keeps_an_item_it_cannot_turn_into_a_draft() {
        let t = setup();
        let mut w = open(&t.config, t.sink);
        let raw = item(&w, &t, b"", failed());
        let queued = item(&w, &t, EML, SendState::Queued);
        for id in [&raw, &queued] {
            w.edit_outbox(id, C);
            assert!(state(&w, id).is_some());
        }
        assert_eq!(notices(&t), 2);
        assert!(w.book.id_of(C).is_none());
        assert!(w.store.drafts.list().unwrap().is_empty());
    }

    #[test]
    fn edit_keeps_an_item_whose_attachment_cannot_be_stored() {
        let t = setup();
        let mut w = open(&t.config, t.sink);
        let id = item(&w, &t, WITH_FILE, failed());
        // The files folder cannot be made: the item and no draft stay.
        let files = t.dir.path().join("data").join("drafts").join("files");
        std::fs::write(&files, b"in the way").unwrap();
        w.edit_outbox(&id, C);
        assert!(state(&w, &id).is_some());
        assert!(w.book.id_of(C).is_none());
        assert!(w.store.drafts.list().unwrap().is_empty());
        assert_eq!(notices(&t), 1);
    }

    #[test]
    fn edit_does_not_replace_a_compose_that_has_a_draft() {
        let t = setup();
        let mut w = open(&t.config, t.sink);
        let id = item(&w, &t, EML, failed());
        w.book.adopt(C, "existing".into());
        w.edit_outbox(&id, C);
        assert!(state(&w, &id).is_some());
        assert_eq!(w.book.id_of(C).as_deref(), Some("existing"));
        assert!(w.store.drafts.list().unwrap().is_empty());
    }
}
