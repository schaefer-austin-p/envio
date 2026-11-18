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
        // A forward carries the attachments, which a partial body lacks.
        let whole = matches!(from, StartFrom::Forward { .. });
        let cached = match message.body {
            BodyState::Downloaded => true,
            BodyState::Partial => !whole,
            BodyState::Missing => false,
        };
        if cached {
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
        if whole {
            live.wants.whole.insert(id);
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
        if matches!(from, StartFrom::Forward { .. }) {
            self.attach_originals(compose, raw);
        }
    }

    /// Forward with attachments: each non-inline attachment of the original
    /// goes through the same path as a picked file (a copy under a safe name
    /// in the draft's folder, `Attached` or `AttachFailed`).
    fn attach_originals(&mut self, compose: ComposeId, raw: &[u8]) {
        let envio_mime::Message::Parsed { attachments, .. } = envio_mime::parse(raw) else {
            return;
        };
        let stage = self
            .store
            .paths
            .bodies()
            .with_file_name(format!("forward-{}", compose.0));
        for info in attachments.iter().filter(|a| !a.inline) {
            let staged = envio_store::create_private_dir(&stage)
                .and_then(|()| super::attachment::write_attachment_of(raw, info.index, &stage));
            match staged {
                Ok(path) => {
                    self.book
                        .attach_emit(compose, &path, &mut |event| self.emit(event));
                    let _ = std::fs::remove_file(&path);
                }
                Err(e) => {
                    log::warn!("reading an attachment to forward: {e}");
                    let name = notice_name(info.filename.as_deref(), info.index);
                    self.emit(Event::AttachFailed {
                        compose,
                        text: text::compose::attachment_unreadable(&name),
                    });
                }
            }
        }
        let _ = std::fs::remove_dir(&stage);
    }

    /// A body step ended: start the replies and forwards that waited for it.
    /// `whole` says `raw` is the whole message, not what a big one comes in.
    pub(super) fn compose_body_fetched(
        &mut self,
        id: MessageRowId,
        raw: Option<&[u8]>,
        whole: bool,
    ) {
        let partial = self
            .store
            .db
            .message(id)
            .is_ok_and(|m| m.body == BodyState::Partial);
        for (compose, from) in self.composing.remove(&id).unwrap_or_default() {
            match raw {
                // The open message's partial body came first: a forward
                // asks again for the whole message.
                Some(_) if !whole && partial && matches!(from, StartFrom::Forward { .. }) => {
                    self.wait_for_whole(compose, id, from);
                }
                Some(raw) => self.finish_start((compose, &from), id, raw),
                None => self.compose_refused(compose, text::read::message_unavailable()),
            }
        }
    }

    /// Asks for the whole body of `id` for a forward that got a partial one.
    /// The body step that found it out is still running and clears its wants
    /// when it ends, so the ask is kept in `whole_again` and made then (see
    /// `run::succeeded`).
    fn wait_for_whole(&mut self, compose: ComposeId, id: MessageRowId, from: StartFrom) {
        if self
            .account_of(id)
            .is_none_or(|a| !self.live.contains_key(&a))
        {
            return self.compose_refused(compose, text::read::message_unavailable());
        }
        self.whole_again.insert(id);
        self.composing.entry(id).or_default().push((compose, from));
    }

    /// The body step of `id` ended well: a forward that needs the whole
    /// message makes the want again.
    pub(super) fn ask_whole_again(&mut self, id: MessageRowId) {
        let open = self.open_again.remove(&id);
        if !self.whole_again.remove(&id) {
            if open {
                let live = self
                    .account_of(id)
                    .and_then(|account| self.live.get_mut(&account));
                if let Some(live) = live.filter(|l| !l.wants.bodies.contains(&id)) {
                    live.wants.bodies.push(id);
                }
            }
            return;
        }
        let live = self
            .account_of(id)
            .and_then(|account| self.live.get_mut(&account));
        if let Some(live) = live {
            if !live.wants.bodies.contains(&id) {
                live.wants.bodies.push(id);
            }
            live.wants.whole.insert(id);
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
                live.wants.whole.remove(&id);
            }
            self.whole_again.remove(&id);
            self.compose_body_fetched(id, None, false);
        }
    }
}

/// An attachment's name as a notice shows it: no path, no bidi or other
/// controls that could reorder the text, and `attachment-N` when unnamed.
fn notice_name(filename: Option<&str>, index: usize) -> String {
    super::read::safe_file_name(filename.unwrap_or(""), index)
}

#[cfg(test)]
mod tests {
    use super::notice_name;

    #[test]
    fn a_failed_forward_names_the_file_safely() {
        assert_eq!(notice_name(Some("inv\u{202E}fdp.exe"), 2), "invfdp.exe");
        assert_eq!(notice_name(None, 2), "attachment-3");
    }
}
