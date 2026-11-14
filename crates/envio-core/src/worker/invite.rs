//! Answering a meeting invite: the reply goes into the Outbox held for the
//! Undo window, the answer is remembered, and the reader gets the new card.

use envio_mime::{Address, When};
use envio_store::{
    BodyState, InviteAnswer, MessageRowId, NewOutboxItem, OutboxItem, SendState, forget_invite,
    load_invites, record_invite,
};
use envio_sync::UNDO_MS;
use envio_sync::plan::outbox::held;

use super::Worker;
use crate::api::command::{Answer, ImagesChoice};
use crate::api::event::{Event, UndoScope};
use crate::api::view::{InviteCard, ReplyState};
use crate::clock::now_ms;
use crate::invite_card::{can_answer, card, part_stat};
use crate::text;
use crate::undo::{Offer, Offered};

/// What the card needs besides the message: the remembered answers, the
/// Outbox, and the address of the account that got the mail.
#[derive(Default)]
pub(crate) struct InviteEnv<'a> {
    pub answers: &'a [InviteAnswer],
    pub outbox: &'a [OutboxItem],
    pub me: &'a str,
}

/// The owned side of `InviteEnv`.
#[derive(Default)]
pub(crate) struct InviteData {
    answers: Vec<InviteAnswer>,
    outbox: Vec<OutboxItem>,
    me: String,
}

impl InviteData {
    pub fn env(&self) -> InviteEnv<'_> {
        InviteEnv {
            answers: &self.answers,
            outbox: &self.outbox,
            me: &self.me,
        }
    }
}

/// Whether `raw` could hold a calendar part (a header names it in plain
/// text), so mail without one costs no file reads.
fn mentions_calendar(raw: &[u8]) -> bool {
    raw.windows(b"calendar".len())
        .any(|w| w.eq_ignore_ascii_case(b"calendar"))
}

/// The card of the invite in `raw`, if it holds one.
pub(crate) fn card_of(raw: &[u8], account: &str, env: &InviteEnv<'_>) -> Option<InviteCard> {
    let (invite, ics) = envio_mime::invite_of(raw)?;
    let remembered = env
        .answers
        .iter()
        .find(|a| a.account == account && a.uid == invite.uid);
    let reply = remembered
        .and_then(|r| {
            env.outbox
                .iter()
                .find(|i| i.meta.message_id == r.reply_message_id)
        })
        .map_or(ReplyState::Gone, |item| match item.meta.state {
            SendState::Failed { .. } => ReplyState::Failed,
            _ => ReplyState::Sending,
        });
    let mut shown = card(&invite, remembered, reply, env.me);
    shown.ics_index = ics.and_then(|i| i64::try_from(i).ok()).unwrap_or(-1);
    Some(shown)
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(y: i32, m: u8, d: u8) -> i64 {
    let y = i64::from(y) - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (i64::from(m) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// When the event ends, as Unix seconds, for pruning the remembered answer:
/// its end, else its start, else `now_secs`; never earlier than `now_secs`,
/// so an answer to an event long past is not dropped on the spot.
fn event_end_unix(when: &When, now_secs: i64) -> i64 {
    let end = match when {
        When::Timed { start, end, .. } => {
            if *end > 0 {
                *end
            } else {
                *start
            }
        }
        When::AllDay { end: (y, m, d), .. } => days_from_civil(*y, *m, *d) * 86_400,
        When::Floating { end, .. } => {
            days_from_civil(end.year, end.month, end.day) * 86_400
                + i64::from(end.hour) * 3600
                + i64::from(end.minute) * 60
                + i64::from(end.second)
        }
    };
    end.max(now_secs)
}

impl Worker<'_> {
    /// What the card of `raw` needs from disk; nothing for mail that names
    /// no calendar part.
    pub(super) fn invite_data(&self, account: &str, raw: &[u8]) -> InviteData {
        if !mentions_calendar(raw) {
            return InviteData::default();
        }
        let answers = load_invites(&self.store.paths.invites()).unwrap_or_else(|e| {
            log::warn!("reading remembered invite answers: {e}");
            Vec::new()
        });
        let outbox = self.store.outbox.items().unwrap_or_else(|e| {
            log::warn!("listing the Outbox: {e}");
            Vec::new()
        });
        let me = self
            .store
            .accounts
            .get(account)
            .map(|a| a.settings.email.clone())
            .unwrap_or_default();
        InviteData {
            answers,
            outbox,
            me,
        }
    }

    pub(super) fn answer_invite(&mut self, id: MessageRowId, answer: Answer, note: &str) {
        if let Err(text) = self.try_answer(id, answer, note.trim()) {
            self.emit(Event::Notice { text });
        }
    }

    /// The remembered answer for (account, uid), if any.
    fn remembered_answer(&self, account: &str, uid: &str) -> Option<InviteAnswer> {
        match load_invites(&self.store.paths.invites()) {
            Ok(all) => all
                .into_iter()
                .find(|a| a.account == account && a.uid == uid),
            Err(e) => {
                log::warn!("reading remembered invite answers: {e}");
                None
            }
        }
    }

    fn try_answer(&mut self, id: MessageRowId, answer: Answer, note: &str) -> Result<(), String> {
        let not_queued = text::invite::not_queued;
        let message = self.store.db.message(id).map_err(|e| {
            log::warn!("finding an invite to answer: {e}");
            not_queued()
        })?;
        if message.body != BodyState::Downloaded {
            return Err(text::invite::cannot_answer());
        }
        let raw = self
            .store
            .bodies
            .read(message.folder, message.uid)
            .map_err(|e| {
                log::warn!("reading an invite to answer: {e}");
                not_queued()
            })?;
        let (invite, _) = envio_mime::invite_of(&raw).ok_or_else(text::invite::cannot_answer)?;
        let original = envio_mime::envelope(&raw).ok_or_else(text::invite::cannot_answer)?;
        if !can_answer(&invite) {
            return Err(text::invite::cannot_answer());
        }
        let account = self.account_of(id).ok_or_else(not_queued)?;
        let settings = self
            .store
            .accounts
            .get(&account)
            .map(|a| a.settings.clone())
            .ok_or_else(not_queued)?;
        let name = if settings.name.is_empty() {
            settings.email.clone()
        } else {
            settings.name.clone()
        };
        let from = Address::new(Some(&name), &settings.email);
        let now = now_ms();
        let outgoing = envio_mime::reply_outgoing(
            &original,
            &invite,
            from,
            part_stat(answer),
            note,
            text::invite::subject(answer, &invite.summary),
            text::invite::reply_text(answer, &name),
            now / 1000,
        );
        let built = envio_mime::build(
            &outgoing,
            now / 1000,
            &envio_mime::new_message_id(&settings.email),
        );
        let message_id = envio_sync::convert::bracketed(&built.message_id);
        let send_after_ms = now + UNDO_MS;
        let item = self
            .store
            .outbox
            .enqueue(
                NewOutboxItem {
                    send_after_ms,
                    account: account.clone(),
                    message_id: message_id.clone(),
                    queued_at_ms: now,
                    from: settings.email,
                    recipients: built.recipients,
                },
                &built.bytes,
            )
            .map_err(|e| {
                log::warn!("queueing an invite answer: {e}");
                not_queued()
            })?;
        let current = self.remembered_answer(&account, &invite.uid);
        let remembered = InviteAnswer {
            account: account.clone(),
            uid: invite.uid.clone(),
            sequence: invite.sequence,
            answer: answer.as_str().to_owned(),
            answered_at_ms: now,
            event_end_unix: event_end_unix(&invite.when, now / 1000),
            reply_message_id: message_id,
        };
        if let Err(e) = record_invite(&self.store.paths.invites(), remembered, now) {
            log::warn!("remembering an invite answer: {e}");
            // The answer is not kept, so the reply must not go.
            if let Err(e) = self.store.outbox.remove(&item.id) {
                log::warn!("removing an unremembered reply: {e}");
            }
            return Err(not_queued());
        }
        // One held reply per invite: a new answer replaces an earlier one
        // that is still held, and Undo goes back to before the first.
        let previous = self
            .replace_held_answer(&account, &invite.uid)
            .unwrap_or(current);
        let undo = self.undo.offer(Offer {
            what: Offered::Answer {
                message: id,
                outbox_item: item.id,
                account,
                uid: invite.uid,
                previous,
            },
            expires_at_ms: send_after_ms,
        });
        self.emit(Event::OutboxChanged);
        self.emit_outbox();
        self.emit(Event::UndoOffer {
            undo,
            text: text::invite::answer_offer(answer),
            expires_at_ms: send_after_ms,
            scope: UndoScope::Reader,
        });
        self.reshow(id);
        Ok(())
    }

    /// Withdraws the live answer offer for (account, uid), if any: its held
    /// reply leaves the Outbox and its offer closes. Returns the answer that
    /// offer would have restored.
    fn replace_held_answer(&mut self, account: &str, uid: &str) -> Option<Option<InviteAnswer>> {
        let (old, offered) = self.undo.take_answer(account, uid)?;
        let Offered::Answer {
            outbox_item,
            previous,
            ..
        } = offered
        else {
            return None;
        };
        let now = now_ms();
        let still_held = self
            .store
            .outbox
            .items()
            .ok()
            .and_then(|items| items.into_iter().find(|i| i.id == outbox_item))
            .is_some_and(|i| i.meta.state == SendState::Queued && held(&i.meta, now));
        let removed = still_held
            && match self.store.outbox.remove(&outbox_item) {
                Ok(()) => true,
                Err(e) => {
                    log::warn!("replacing a held reply: {e}");
                    false
                }
            };
        self.emit(Event::UndoClosed {
            undo: old,
            undone: false,
        });
        // Only a reply that was really taken back may be skipped over.
        removed.then_some(previous)
    }

    /// `Command::Undo` on an answer: the held reply is deleted and the
    /// earlier answer comes back.
    pub(super) fn undo_answer(&mut self, undo: crate::api::command::UndoId, offered: Offered) {
        let Offered::Answer {
            message,
            outbox_item,
            account,
            uid,
            previous,
        } = offered
        else {
            return;
        };
        let item = self
            .store
            .outbox
            .items()
            .ok()
            .and_then(|items| items.into_iter().find(|i| i.id == outbox_item));
        // Gone (sent and removed) or past Queued: too late.
        if item
            .as_ref()
            .is_none_or(|i| i.meta.state != SendState::Queued)
        {
            self.emit(Event::UndoClosed {
                undo,
                undone: false,
            });
            return self.emit(Event::Notice {
                text: text::undo::too_late(),
            });
        }
        if let Err(e) = self.store.outbox.remove(&outbox_item) {
            log::warn!("deleting a held reply: {e}");
            self.emit(Event::UndoClosed {
                undo,
                undone: false,
            });
            return self.emit(Event::Notice {
                text: text::undo::cannot_undo(),
            });
        }
        let path = self.store.paths.invites();
        let restored = match previous {
            Some(answer) => record_invite(&path, answer, now_ms()),
            None => forget_invite(&path, &account, &uid),
        };
        let undone = restored.is_ok();
        if let Err(e) = restored {
            log::warn!("restoring an invite answer: {e}");
            self.emit(Event::Notice {
                text: text::undo::cannot_undo(),
            });
        }
        self.emit(Event::UndoClosed { undo, undone });
        self.emit(Event::OutboxChanged);
        self.emit_outbox();
        self.reshow(message);
    }

    /// Shows the open message again, for a card that changed. Nothing is
    /// sent when the reader is not on `id`.
    fn reshow(&mut self, id: MessageRowId) {
        let Some(shown) = self.store.shown.as_ref().filter(|s| s.id == id) else {
            return;
        };
        let choice: (ImagesChoice, u32) = shown.choice;
        let Ok(message) = self.store.db.message(id) else {
            return;
        };
        match self.store.bodies.read(message.folder, message.uid) {
            Ok(raw) => {
                let view = self.view(&message, &raw, choice);
                self.show(id, choice, view);
            }
            Err(e) => log::warn!("reading a cached body: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use envio_mime::NaiveParts;

    #[test]
    fn an_event_end_is_never_before_now() {
        let timed = |start, end| When::Timed {
            start,
            end,
            zone_unknown: false,
        };
        assert_eq!(event_end_unix(&timed(100, 200), 50), 200);
        assert_eq!(event_end_unix(&timed(100, 0), 50), 100);
        assert_eq!(event_end_unix(&timed(0, 0), 50), 50);
        assert_eq!(event_end_unix(&timed(100, 200), 900), 900);
    }

    #[test]
    fn all_day_and_floating_ends_are_read_as_utc() {
        let all_day = When::AllDay {
            start: (1970, 1, 1),
            end: (1970, 1, 3),
        };
        assert_eq!(event_end_unix(&all_day, 0), 2 * 86_400);
        let at = NaiveParts {
            year: 1970,
            month: 1,
            day: 2,
            hour: 1,
            minute: 2,
            second: 3,
        };
        let floating = When::Floating { start: at, end: at };
        assert_eq!(event_end_unix(&floating, 0), 86_400 + 3723);
    }
}
