//! Answering a meeting invite: the reply goes into the Outbox held for the
//! Undo window, the answer is remembered, and the reader gets the new card.

use envio_mime::{Address, Invite, When};
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

/// Whether `raw` holds an invite, checked without rendering the message.
pub(crate) fn holds_invite(raw: &[u8]) -> bool {
    mentions_calendar(raw) && envio_mime::invite_of(raw).is_some()
}

/// The card of the invite in `raw`, if it holds one.
pub(crate) fn card_of(raw: &[u8], account: &str, env: &InviteEnv<'_>) -> Option<InviteCard> {
    let (invite, ics) = envio_mime::invite_of(raw)?;
    let remembered = env.answers.iter().find(|a| {
        a.account == account && a.uid == invite.uid && a.recurrence_id == invite.recurrence_id
    });
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

/// When the remembered answer of `invite` may be dropped (before the 30
/// days of keeping): the event's end; for a series its RRULE UNTIL, or
/// never (`i64::MAX`) when it has none, so a running series keeps its
/// answer.
fn keep_until(invite: &Invite, now_secs: i64) -> i64 {
    let end = event_end_unix(&invite.when, now_secs);
    match (invite.repeats, invite.repeats_until) {
        (false, _) => end,
        (true, Some(until)) => end.max(until),
        (true, None) => i64::MAX,
    }
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

    /// The remembered answer for (account, uid, recurrence id), if any.
    fn remembered_answer(&self, account: &str, uid: &str, rid: &str) -> Option<InviteAnswer> {
        match load_invites(&self.store.paths.invites()) {
            Ok(all) => all
                .into_iter()
                .find(|a| a.account == account && a.uid == uid && a.recurrence_id == rid),
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
        // A partial body keeps a small calendar part whole: `invite_of` below
        // is the test, the same one the card passes.
        if message.body == BodyState::Missing {
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
        if built.recipients.is_empty() {
            // Nobody to send to: nothing is queued or remembered.
            return Err(text::invite::cannot_answer());
        }
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
        let current = self.remembered_answer(&account, &invite.uid, &invite.recurrence_id);
        let withdrawing = self.withdrawable_reply(&account, &invite.uid, &invite.recurrence_id);
        let sent_before = self.last_sent(current.clone(), withdrawing.as_deref());
        let remembered = InviteAnswer {
            account: account.clone(),
            uid: invite.uid.clone(),
            recurrence_id: invite.recurrence_id.clone(),
            sequence: invite.sequence,
            answer: answer.as_str().to_owned(),
            answered_at_ms: now,
            event_end_unix: keep_until(&invite, now / 1000),
            reply_message_id: message_id,
            previous: sent_before.map(Box::new),
        };
        if let Err(e) = record_invite(&self.store.paths.invites(), remembered.clone(), now) {
            log::warn!("remembering an invite answer: {e}");
            // The answer is not kept, so the reply must not go.
            if let Err(e) = self.store.outbox.remove(&item.id) {
                log::warn!("removing an unremembered reply: {e}");
            }
            return Err(not_queued());
        }
        // One held reply per invite: a new answer replaces an earlier one
        // that is still held, and Undo goes back to before the first.
        let replaced = self.replace_held_answer(&account, &invite.uid, &invite.recurrence_id);
        if replaced.is_none() && withdrawing.is_some() {
            // The held reply could not be taken back, so it goes out after
            // all: it is the answer that was sent before this one.
            let fixed = InviteAnswer {
                previous: self.last_sent(current.clone(), None).map(Box::new),
                ..remembered
            };
            if let Err(e) = record_invite(&self.store.paths.invites(), fixed, now) {
                log::warn!("remembering the answer sent before: {e}");
            }
        }
        let previous = replaced.unwrap_or(current);
        let undo = self.undo.offer(Offer {
            what: Offered::Answer {
                message: id,
                outbox_item: item.id,
                account,
                uid: invite.uid,
                recurrence_id: invite.recurrence_id,
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

    /// The Message-ID of the held reply that `replace_held_answer` will try
    /// to take back: the one an open Undo offer holds, still queued in its
    /// window. A held reply with no offer (after a restart) will go out.
    fn withdrawable_reply(&self, account: &str, uid: &str, rid: &str) -> Option<String> {
        let item = self.undo.answer_item(account, uid, rid)?;
        let now = now_ms();
        self.store
            .outbox
            .items()
            .ok()?
            .into_iter()
            .find(|i| i.id == item && i.meta.state == SendState::Queued && held(&i.meta, now))
            .map(|i| i.meta.message_id)
    }

    /// The newest answer in the chain from `answer` back whose reply went
    /// out or still will: one whose reply failed, or is `withdrawn` (being
    /// taken back by `replace_held_answer`), was not sent, and its own
    /// `previous` is looked at instead. A reply still queued counts as sent.
    fn last_sent(
        &self,
        mut answer: Option<InviteAnswer>,
        withdrawn: Option<&str>,
    ) -> Option<InviteAnswer> {
        let outbox = self.store.outbox.items().unwrap_or_default();
        while let Some(a) = answer {
            let unsent = withdrawn == Some(a.reply_message_id.as_str())
                || outbox.iter().any(|i| {
                    i.meta.message_id == a.reply_message_id
                        && matches!(i.meta.state, SendState::Failed { .. })
                });
            if !unsent {
                return Some(InviteAnswer {
                    previous: None,
                    ..a
                });
            }
            answer = a.previous.map(|p| *p);
        }
        None
    }

    /// Withdraws the live answer offer for (account, uid, recurrence id), if any: its held
    /// reply leaves the Outbox and its offer closes. Returns the answer that
    /// offer would have restored.
    fn replace_held_answer(
        &mut self,
        account: &str,
        uid: &str,
        rid: &str,
    ) -> Option<Option<InviteAnswer>> {
        let (old, offered) = self.undo.take_answer(account, uid, rid)?;
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
            recurrence_id,
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
        // Restore the card's answer first: if that cannot be written the
        // reply stays held and the card still says what it says.
        let path = self.store.paths.invites();
        let current = self.remembered_answer(&account, &uid, &recurrence_id);
        let restored = match previous {
            Some(answer) => record_invite(&path, answer, now_ms()),
            None => forget_invite(&path, &account, &uid, &recurrence_id),
        };
        if let Err(e) = restored {
            log::warn!("restoring an invite answer: {e}");
            self.emit(Event::UndoClosed {
                undo,
                undone: false,
            });
            return self.emit(Event::Notice {
                text: text::undo::cannot_undo(),
            });
        }
        let mut undone = true;
        if let Err(e) = self.store.outbox.remove(&outbox_item) {
            log::warn!("deleting a held reply: {e}");
            // The reply stays and will go: put its answer back.
            if let Some(answer) = current
                && let Err(e) = record_invite(&path, answer, now_ms())
            {
                log::warn!("putting an invite answer back: {e}");
            }
            undone = false;
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
        self.reshow_where(Some(id), false);
    }

    /// After the Outbox changed: the open message is shown again when it
    /// holds an invite, so its card follows the reply (sent, or not sent).
    pub(super) fn reshow_open_invite(&mut self) {
        self.reshow_where(None, true);
    }

    fn reshow_where(&mut self, only: Option<MessageRowId>, invite_only: bool) {
        let Some(shown) = self
            .store
            .shown
            .as_ref()
            .filter(|s| only.is_none_or(|id| s.id == id))
        else {
            return;
        };
        let (id, choice): (MessageRowId, (ImagesChoice, u32)) = (shown.id, shown.choice);
        let Ok(message) = self.store.db.message(id) else {
            return;
        };
        match self.store.bodies.read(message.folder, message.uid) {
            Ok(raw) => {
                if invite_only && !holds_invite(&raw) {
                    return;
                }
                let view = self.view(&message, &raw, choice);
                self.show(id, choice, view);
            }
            Err(e) => log::warn!("reading a cached body: {e}"),
        }
    }

    /// An Outbox item was deleted or edited away: a reply that never went
    /// out is not remembered as an answer, so its card is open again.
    pub(super) fn forget_answer_of_reply(&mut self, message_id: &str) {
        let path = self.store.paths.invites();
        let found = load_invites(&path)
            .ok()
            .and_then(|all| all.into_iter().find(|a| a.reply_message_id == message_id));
        let Some(a) = found else {
            return;
        };
        // An answer that really went out earlier stays; otherwise the card
        // is open again.
        let result = match a.previous {
            Some(sent) => record_invite(&path, *sent, now_ms()),
            None => forget_invite(&path, &a.account, &a.uid, &a.recurrence_id),
        };
        if let Err(e) = result {
            log::warn!("forgetting an invite answer: {e}");
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

#[cfg(test)]
mod reply_follow_tests {
    use super::super::testing::{Sink, open, seed};
    use super::*;
    use crate::api::command::OutboxDecision;
    use crate::api::view::CardState;

    const MAIL: &[u8] = b"From: Ada <ada@example.org>\r\nTo: me@example.org\r\n\
Subject: Planning\r\nMessage-ID: <i@example.org>\r\nMIME-Version: 1.0\r\n\
Content-Type: text/calendar; method=REQUEST; charset=utf-8\r\n\r\n\
BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:u1@example.org\r\n\
DTSTART:20270115T080000Z\r\nDTEND:20270115T090000Z\r\nSUMMARY:Planning\r\n\
ORGANIZER:mailto:ada@example.org\r\nATTENDEE:mailto:me@example.org\r\n\
END:VEVENT\r\nEND:VCALENDAR\r\n";

    /// A worker with the invite open and answered; the reply is the one
    /// Outbox item returned.
    fn answered<'s>(sink: &'s Sink, dir: &std::path::Path) -> (Worker<'s>, OutboxItem) {
        let (config, seeded) = seed(dir);
        let mut worker = open(&config, sink);
        worker.store.bodies.write(seeded.folder, 1, MAIL).unwrap();
        worker
            .store
            .db
            .set_body_state(seeded.message, BodyState::Downloaded)
            .unwrap();
        worker.open_message(seeded.message, ImagesChoice::Default, 300);
        worker.answer_invite(seeded.message, Answer::Accepted, "");
        let item = worker.store.outbox.items().unwrap().remove(0);
        (worker, item)
    }

    fn failed() -> SendState {
        SendState::Failed {
            reason: "no route".into(),
        }
    }

    /// What the Outbox run writes when SMTP refuses the reply. Not spelled as
    /// a call: only the worker's Outbox code may set a state.
    fn mark_failed(worker: &Worker<'_>, item: &OutboxItem) {
        let set = envio_store::Outbox::set_state;
        set(&worker.store.outbox, &item.id, failed()).unwrap();
    }

    fn last_card(sink: &Sink) -> InviteCard {
        sink.events()
            .into_iter()
            .rev()
            .find_map(|e| match e {
                Event::MessageOpened { view, .. } => view.invite,
                _ => None,
            })
            .expect("a card was shown")
    }

    #[test]
    fn a_sent_reply_turns_the_open_card_to_answered() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, item) = answered(&sink, dir.path());
        assert!(last_card(&sink).sending);
        // The Outbox run sent it and removed the item.
        worker.store.outbox.remove(&item.id).unwrap();
        worker.reshow_open_invite();
        let card = last_card(&sink);
        assert_eq!(card.state, CardState::Answered);
        assert!(!card.sending && !card.send_failed);
    }

    #[test]
    fn a_failed_reply_turns_the_open_card_to_not_sent() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, item) = answered(&sink, dir.path());
        mark_failed(&worker, &item);
        worker.reshow_open_invite();
        let card = last_card(&sink);
        assert!(card.send_failed && !card.sending);
    }

    #[test]
    fn deleting_the_failed_reply_opens_the_card_again() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, item) = answered(&sink, dir.path());
        mark_failed(&worker, &item);
        worker.decide_outbox(&item.id, OutboxDecision::Delete);
        assert_eq!(last_card(&sink).state, CardState::Open);
        let kept = load_invites(&worker.store.paths.invites()).unwrap();
        assert!(kept.is_empty());
    }

    #[test]
    fn a_sent_reply_leaving_the_outbox_keeps_the_answer() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, item) = answered(&sink, dir.path());
        worker.store.outbox.remove(&item.id).unwrap();
        worker.reshow_open_invite();
        let kept = load_invites(&worker.store.paths.invites()).unwrap();
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn an_occurrence_card_does_not_show_the_series_answer() {
        let series = InviteAnswer {
            account: "a".into(),
            uid: "u1@example.org".into(),
            recurrence_id: String::new(),
            sequence: 0,
            answer: "accepted".into(),
            answered_at_ms: 1,
            event_end_unix: 2_000_000_000,
            reply_message_id: "<r@example.org>".into(),
            previous: None,
        };
        let env = InviteEnv {
            answers: std::slice::from_ref(&series),
            outbox: &[],
            me: "me@example.org",
        };
        let text = String::from_utf8(MAIL.to_vec()).unwrap();
        let one = text.replace(
            "UID:u1@example.org\r\n",
            "UID:u1@example.org\r\nRECURRENCE-ID:20270115T080000Z\r\n",
        );
        assert_eq!(card_of(MAIL, "a", &env).unwrap().state, CardState::Answered);
        assert_eq!(
            card_of(one.as_bytes(), "a", &env).unwrap().state,
            CardState::Open
        );
    }

    fn notices(sink: &Sink) -> Vec<String> {
        sink.events()
            .into_iter()
            .filter_map(|e| match e {
                Event::Notice { text } => Some(text),
                _ => None,
            })
            .collect()
    }

    /// A worker with `mail` cached under `body` and open, nothing answered.
    fn opened<'s>(
        sink: &'s Sink,
        dir: &std::path::Path,
        mail: &[u8],
        body: BodyState,
    ) -> Worker<'s> {
        let (config, seeded) = seed(dir);
        let mut worker = open(&config, sink);
        worker.store.bodies.write(seeded.folder, 1, mail).unwrap();
        worker
            .store
            .db
            .set_body_state(seeded.message, body)
            .unwrap();
        worker.open_message(seeded.message, ImagesChoice::Default, 300);
        worker
    }

    #[test]
    fn a_partial_body_with_its_calendar_part_can_be_answered() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let mut worker = opened(&sink, dir.path(), MAIL, BodyState::Partial);
        let id = worker.store.shown.as_ref().unwrap().id;
        assert!(last_card(&sink).can_answer);
        worker.answer_invite(id, Answer::Accepted, "");
        assert_eq!(worker.store.outbox.items().unwrap().len(), 1);
        assert!(notices(&sink).is_empty());
    }

    #[test]
    fn a_partial_body_without_its_calendar_part_offers_no_card() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        // The calendar part was left out of the cached body.
        let stub = String::from_utf8(MAIL.to_vec())
            .unwrap()
            .split("BEGIN:VCALENDAR")
            .next()
            .unwrap()
            .to_owned();
        let mut worker = opened(&sink, dir.path(), stub.as_bytes(), BodyState::Partial);
        let id = worker.store.shown.as_ref().unwrap().id;
        let opened_with_card = sink
            .events()
            .into_iter()
            .any(|e| matches!(e, Event::MessageOpened { view, .. } if view.invite.is_some()));
        assert!(!opened_with_card);
        worker.answer_invite(id, Answer::Accepted, "");
        assert!(worker.store.outbox.items().unwrap().is_empty());
        assert_eq!(notices(&sink), [text::invite::cannot_answer()]);
    }

    #[test]
    fn an_organizer_the_reply_cannot_reach_is_not_answered() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let text = String::from_utf8(MAIL.to_vec()).unwrap().replace(
            "mailto:ada@example.org",
            "mailto:ada@example.org>,eve@example.org",
        );
        let mut worker = opened(&sink, dir.path(), text.as_bytes(), BodyState::Downloaded);
        let id = worker.store.shown.as_ref().unwrap().id;
        assert!(!last_card(&sink).can_answer);
        worker.answer_invite(id, Answer::Accepted, "");
        assert!(worker.store.outbox.items().unwrap().is_empty());
        assert!(
            load_invites(&worker.store.paths.invites())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_running_series_keeps_its_answer_past_the_first_occurrence() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let series = String::from_utf8(MAIL.to_vec())
            .unwrap()
            .replace("SUMMARY:Planning", "SUMMARY:Planning\r\nRRULE:FREQ=WEEKLY");
        let mut worker = opened(&sink, dir.path(), series.as_bytes(), BodyState::Downloaded);
        let id = worker.store.shown.as_ref().unwrap().id;
        worker.answer_invite(id, Answer::Accepted, "");
        let kept = load_invites(&worker.store.paths.invites()).unwrap();
        assert_eq!(kept[0].event_end_unix, i64::MAX);
    }

    #[test]
    fn a_series_with_an_until_is_kept_until_then() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let series = String::from_utf8(MAIL.to_vec()).unwrap().replace(
            "SUMMARY:Planning",
            "SUMMARY:Planning\r\nRRULE:FREQ=WEEKLY;UNTIL=20270301T000000Z",
        );
        let mut worker = opened(&sink, dir.path(), series.as_bytes(), BodyState::Downloaded);
        let id = worker.store.shown.as_ref().unwrap().id;
        worker.answer_invite(id, Answer::Accepted, "");
        let kept = load_invites(&worker.store.paths.invites()).unwrap();
        assert_eq!(kept[0].event_end_unix, 1_803_859_200);
    }

    #[test]
    fn deleting_a_failed_second_reply_restores_the_answer_that_was_sent() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, first) = answered(&sink, dir.path());
        let id = worker.store.shown.as_ref().unwrap().id;
        // The Accept went out and left the Outbox, which closed its offer
        // (else the next reply, queued in the same millisecond, could get
        // the freed id and be withdrawn as the held one).
        worker.undo.expire(i64::MAX);
        worker.store.outbox.remove(&first.id).unwrap();
        worker.answer_invite(id, Answer::Declined, "");
        let second = worker.store.outbox.items().unwrap().remove(0);
        mark_failed(&worker, &second);
        worker.decide_outbox(&second.id, OutboxDecision::Delete);
        let kept = load_invites(&worker.store.paths.invites()).unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].answer, "accepted");
        let card = last_card(&sink);
        assert_eq!(card.state, CardState::Answered);
        assert_eq!(card.answer, Some(Answer::Accepted));
    }

    #[test]
    fn deleting_a_failed_reply_keeps_an_earlier_one_still_queued_to_go_out() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, first) = answered(&sink, dir.path());
        let id = worker.store.shown.as_ref().unwrap().id;
        // The Undo window of the Accept ended; offline, it is still queued.
        worker.undo.expire(i64::MAX);
        let meta = worker
            .store
            .paths
            .outbox()
            .join(format!("{}.json", first.id));
        let json = std::fs::read_to_string(&meta).unwrap();
        let past = json.replace(
            &format!("\"send_after_ms\": {}", first.meta.send_after_ms),
            "\"send_after_ms\": 1",
        );
        assert_ne!(json, past, "the send-after time was rewritten");
        std::fs::write(&meta, past).unwrap();
        worker.answer_invite(id, Answer::Declined, "");
        let items = worker.store.outbox.items().unwrap();
        assert_eq!(items.len(), 2);
        let second = items.into_iter().find(|i| i.id != first.id).unwrap();
        mark_failed(&worker, &second);
        worker.decide_outbox(&second.id, OutboxDecision::Delete);
        let kept = load_invites(&worker.store.paths.invites()).unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].answer, "accepted");
    }

    #[test]
    fn deleting_a_failed_reply_does_not_bring_back_one_withdrawn_in_its_window() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, _) = answered(&sink, dir.path());
        let id = worker.store.shown.as_ref().unwrap().id;
        // Decline inside the Accept's Undo window: the Accept never goes out.
        worker.answer_invite(id, Answer::Declined, "");
        let items = worker.store.outbox.items().unwrap();
        assert_eq!(items.len(), 1);
        mark_failed(&worker, &items[0]);
        worker.decide_outbox(&items[0].id, OutboxDecision::Delete);
        assert!(
            load_invites(&worker.store.paths.invites())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_held_reply_whose_offer_was_lost_counts_as_sent() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, first) = answered(&sink, dir.path());
        let id = worker.store.shown.as_ref().unwrap().id;
        // A restart inside the Accept's Undo window: the offer is gone, the
        // held Accept stays in the Outbox and will go out.
        worker.undo = crate::undo::UndoBook::default();
        worker.answer_invite(id, Answer::Declined, "");
        let items = worker.store.outbox.items().unwrap();
        assert_eq!(items.len(), 2);
        let second = items.into_iter().find(|i| i.id != first.id).unwrap();
        mark_failed(&worker, &second);
        worker.decide_outbox(&second.id, OutboxDecision::Delete);
        let kept = load_invites(&worker.store.paths.invites()).unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].answer, "accepted");
    }

    #[test]
    fn only_mail_with_an_invite_is_rendered_again_after_the_outbox_changes() {
        assert!(holds_invite(MAIL));
        let plain = b"From: a@example.org\r\nSubject: hi\r\n\r\nthe calendar is full\r\n";
        assert!(!holds_invite(plain));
        let named = b"From: a@example.org\r\nContent-Type: text/calendar\r\n\r\nnot a calendar\r\n";
        assert!(!holds_invite(named));
    }
}
