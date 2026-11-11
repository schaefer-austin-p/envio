//! Starting a compose window from a message: Reply and Forward quote the
//! message, so they wait for its body first.

use envio_mime::{Address, Quote, ReplyKind};
use envio_store::{BodyState, MessageRowId};

use super::Worker;
use crate::api::command::{ComposeId, DraftInput, StartFrom};
use crate::api::event::Event;
use crate::clock::now_ms;
use crate::compose;
use crate::text;

/// The user's text of a name, or the address when it has none.
fn shown_name(from: &[Address]) -> String {
    from.first()
        .map(|a| a.name.clone().unwrap_or_else(|| a.email.clone()))
        .unwrap_or_default()
}

pub(super) fn displayed(list: &[Address]) -> Vec<String> {
    list.iter().map(Address::display).collect()
}

impl Worker<'_> {
    /// Reply and Forward quote the message, so its body comes first.
    pub(super) fn start_from_message(
        &mut self,
        compose: ComposeId,
        id: MessageRowId,
        from: StartFrom,
    ) {
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
        if matches!(
            from,
            StartFrom::Forward {
                attachments: true,
                ..
            }
        ) {
            self.attach_originals(compose, id, raw);
        }
    }

    /// Forward with attachments: each non-inline attachment of the original
    /// goes through the same path as a picked file (a copy under a safe name
    /// in the draft's folder, `Attached` or `AttachFailed`).
    fn attach_originals(&mut self, compose: ComposeId, id: MessageRowId, raw: &[u8]) {
        let envio_mime::Message::Parsed { attachments, .. } = envio_mime::parse(raw) else {
            return;
        };
        let stage = self
            .store
            .paths
            .bodies()
            .with_file_name(format!("forward-{}", compose.0));
        for info in attachments.iter().filter(|a| !a.inline) {
            let staged = std::fs::create_dir_all(&stage)
                .and_then(|()| self.write_attachment(id, info.index, &stage));
            let events = match staged {
                Ok(path) => {
                    let events = self.book.attach(compose, &path);
                    let _ = std::fs::remove_file(&path);
                    events
                }
                Err(e) => {
                    log::warn!("reading an attachment to forward: {e}");
                    let name = info.filename.clone().unwrap_or_default();
                    vec![Event::AttachFailed {
                        compose,
                        text: text::compose::attachment_unreadable(&name),
                    }]
                }
            };
            for event in events {
                self.emit(event);
            }
        }
        let _ = std::fs::remove_dir(&stage);
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
}
