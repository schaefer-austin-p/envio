//! The Outbox for the app: the list, the user's decisions, and Edit.
//! Sending states are written by envio-sync; only `decide_outbox` below
//! changes an existing item, and only as `outbox::decide` allows.

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
        let Some(item) = self.find_item(id) else {
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
    }

    /// An id with no readable item: only a damaged one can be deleted.
    fn decide_unreadable(&mut self, id: &str, decision: OutboxDecision) {
        let damaged = self
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

    fn remove_item(&mut self, id: &str) {
        match self.store.outbox.remove(id) {
            Ok(()) => self.forget_item(id),
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
        let draft = (editable && !self.sent.contains(&compose))
            .then(|| self.store.outbox.eml(id).ok())
            .flatten()
            .and_then(|eml| {
                outbox::draft_from_eml(&item.meta.account, &eml, &item.meta.recipients)
            });
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
        self.drafts_of.insert(compose, draft_id.clone());
        draft.draft_id = Some(draft_id.clone());
        self.compose_ready(compose, draft, String::new());
        self.emit(Event::DraftSaved { compose, draft_id });
        self.remove_item(id);
        self.emit_outbox();
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
