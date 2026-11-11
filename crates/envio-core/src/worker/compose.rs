//! Compose: starting a window, saving drafts, closing, and Send.
//! A draft is saved through `Drafts` (outside the cache); Send writes the
//! Outbox first and deletes the draft only after that worked.

use std::path::Path;

use envio_mime::{Address, OutgoingAttachment, Quote, ReplyKind};
use envio_store::{BodyState, MessageRowId};

use super::Worker;
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

/// The user's text of a name, or the address when it has none.
fn shown_name(from: &[Address]) -> String {
    from.first()
        .map(|a| a.name.clone().unwrap_or_else(|| a.email.clone()))
        .unwrap_or_default()
}

fn displayed(list: &[Address]) -> Vec<String> {
    list.iter().map(Address::display).collect()
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
    fn compose_refused(&self, compose: ComposeId, text: String) {
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

    fn compose_ready(&self, compose: ComposeId, draft: DraftInput, quoted: String) {
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

    /// Reply and Forward quote the message, so its body comes first.
    fn start_from_message(&mut self, compose: ComposeId, id: MessageRowId, from: StartFrom) {
        let unavailable = text::read::message_unavailable();
        let Ok(message) = self.store.db.message(id) else {
            return self.compose_refused(compose, unavailable);
        };
        if message.body == BodyState::Downloaded {
            match self.store.bodies.read(message.folder, message.uid) {
                Ok(raw) => return self.finish_start((compose, &from), id, &raw),
                Err(e) => log::warn!("reading a cached body: {e}"),
            }
        }
        let now = now_ms();
        let full = self.disk_full(now);
        let live = self
            .account_of(id)
            .and_then(|account| self.live.get_mut(&account));
        let Some(live) = live else {
            return self.compose_refused(compose, unavailable);
        };
        if full || live.health.is_paused() || !live.health.can_run(now) {
            return self.compose_refused(compose, unavailable);
        }
        if !live.wants.bodies.contains(&id) {
            live.wants.bodies.push(id);
        }
        self.composing.entry(id).or_default().push((compose, from));
    }

    /// Builds the reply or forward from the message's raw bytes.
    fn finish_start(
        &mut self,
        (compose, from): (ComposeId, &StartFrom),
        id: MessageRowId,
        raw: &[u8],
    ) {
        let Some(account) = self.account_of(id) else {
            return self.compose_refused(compose, text::read::message_unavailable());
        };
        let Some(settings) = self
            .store
            .accounts
            .get(&account)
            .map(|a| a.settings.clone())
        else {
            return self.compose_refused(compose, text::compose::no_account());
        };
        let Some(original) = envio_mime::envelope(raw) else {
            return self.compose_refused(compose, text::read::message_unavailable());
        };
        let quote = envio_mime::quote_text(raw);
        let date = compose::date_text(original.date);
        let mut draft = DraftInput {
            account,
            ..DraftInput::default()
        };
        let quoted = match from {
            StartFrom::Forward { .. } => {
                let labels = text::compose::forward_labels();
                let fwd = envio_mime::forward(&original, &quote, &labels, &date);
                draft.subject = fwd.subject;
                fwd.forwarded
            }
            _ => {
                let all = matches!(from, StartFrom::Reply { all: true, .. });
                let kind = if all {
                    ReplyKind::All
                } else {
                    ReplyKind::Sender
                };
                let attribution = text::compose::attribution(&shown_name(&original.from), &date);
                let quote = Quote {
                    text: &quote,
                    attribution: &attribution,
                };
                let reply = envio_mime::reply(&original, &quote, &[settings.email], kind);
                draft.to = displayed(&reply.to);
                draft.cc = displayed(&reply.cc);
                draft.subject = reply.subject;
                draft.in_reply_to = reply
                    .in_reply_to
                    .map(|m| envio_sync::convert::bracketed(&m));
                draft.references = reply
                    .references
                    .iter()
                    .map(|m| envio_sync::convert::bracketed(m))
                    .collect();
                reply.quoted
            }
        };
        self.compose_ready(compose, draft, quoted);
    }

    /// A body step ended: start the replies and forwards that waited for it.
    pub(super) fn compose_body_fetched(&mut self, id: MessageRowId, raw: Option<&[u8]>) {
        for (compose, from) in self.composing.remove(&id).unwrap_or_default() {
            match raw {
                Some(raw) => self.finish_start((compose, &from), id, raw),
                None => self.compose_refused(compose, text::read::message_unavailable()),
            }
        }
    }

    /// A step failed: nobody waiting for a body of `account` (any account
    /// for `None`) will get it soon.
    pub(super) fn compose_failed(&mut self, account: Option<&str>) {
        let waiting: Vec<MessageRowId> = self
            .composing
            .keys()
            .copied()
            .filter(|id| account.is_none_or(|a| self.account_of(*id).is_none_or(|b| a == b)))
            .collect();
        for id in waiting {
            for live in self.live.values_mut() {
                live.wants.bodies.retain(|b| *b != id);
            }
            self.compose_body_fetched(id, None);
        }
    }

    /// An account is going: its pending draft saves must not bring it back.
    pub(super) fn drop_composes_of(&mut self, account: &str) {
        self.throttle.drop_account(account);
    }

    pub(super) fn draft_changed(&mut self, compose: ComposeId, draft: DraftInput) {
        self.throttle.changed(compose, draft, now_ms());
    }

    /// Writes the draft: created the first time, replaced after.
    pub(super) fn save_draft(&mut self, compose: ComposeId, draft: &DraftInput) {
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
            }
            Err(error) => {
                log::warn!("saving a draft: {error}");
                let text = match problem::from_store(&error) {
                    Problem::DiskFull => text::notice::disk_full(),
                    _ => text::compose::draft_not_saved(),
                };
                self.emit(Event::Notice { text });
            }
        }
    }

    /// Saves every draft still waiting for its second.
    pub(super) fn flush_drafts(&mut self) {
        for (compose, draft) in self.throttle.take_all() {
            self.save_draft(compose, &draft);
        }
    }

    pub(super) fn save_due_drafts(&mut self, now: i64) {
        for (compose, draft) in self.throttle.due(now) {
            self.save_draft(compose, &draft);
        }
    }

    pub(super) fn close_compose(&mut self, compose: ComposeId, discard: bool) {
        let pending = self.throttle.take(compose);
        if discard {
            self.delete_draft(
                compose,
                pending.as_ref().and_then(|d| d.draft_id.as_deref()),
            );
        } else {
            if let Some(draft) = pending {
                self.save_draft(compose, &draft);
            }
            self.drafts_of.remove(&compose);
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
        let Some(settings) = self
            .store
            .accounts
            .get(&draft.account)
            .map(|a| a.settings.clone())
        else {
            return self.compose_refused(compose, refusal(&ComposeError::NoAccount));
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
                    return self
                        .compose_refused(compose, text::compose::attachment_unreadable(&name));
                }
            }
        }
        let outgoing = match compose::to_outgoing(draft, from, files) {
            Ok(outgoing) => outgoing,
            Err(error) => return self.compose_refused(compose, refusal(&error)),
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
                self.emit(Event::Queued {
                    compose,
                    item: item.id,
                });
                self.emit(Event::OutboxChanged);
            }
            Err(error) => {
                log::warn!("queueing a message: {error}");
                self.compose_refused(compose, text::compose::queue_failed());
            }
        }
    }
}
