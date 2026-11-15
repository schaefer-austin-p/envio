//! Answering meeting invites through the held Outbox, with Undo.

mod support;

use std::time::Duration;

use envio_core::{
    Answer, CardState, Command, Event, FolderRole, ImagesChoice, InviteCard, MessageRowId, UndoId,
    UndoScope, UserAction,
};
use envio_store::{Outbox, OutboxItem, SendState, load_invites};
use support::Harness;

const ME: &str = "me@example.org";

/// An iMIP mail: plain text plus a `text/calendar` alternative.
fn invite_mail(method: &str, sequence: u32, organizer: bool, attendee: &str) -> Vec<u8> {
    let organizer = if organizer {
        "ORGANIZER;CN=Ada:mailto:ada@example.org\r\n"
    } else {
        ""
    };
    format!(
        "From: Ada <ada@example.org>\r\nTo: me@example.org\r\n\
Subject: Planning\r\nMessage-ID: <inv{sequence}@example.org>\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/alternative; boundary=\"b\"\r\n\r\n\
--b\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nYou are invited.\r\n\
--b\r\nContent-Type: text/calendar; method={method}; charset=utf-8\r\n\r\n\
BEGIN:VCALENDAR\r\nVERSION:2.0\r\nMETHOD:{method}\r\nBEGIN:VEVENT\r\n\
UID:u1@example.org\r\nSEQUENCE:{sequence}\r\nDTSTAMP:20270101T000000Z\r\n\
DTSTART:20270115T080000Z\r\nDTEND:20270115T090000Z\r\nSUMMARY:Planning\r\n\
LOCATION:Room 4\r\n{organizer}{attendee}\
END:VEVENT\r\nEND:VCALENDAR\r\n--b--\r\n"
    )
    .into_bytes()
}

fn request(sequence: u32) -> Vec<u8> {
    invite_mail(
        "REQUEST",
        sequence,
        true,
        "ATTENDEE;CN=Me;PARTSTAT=NEEDS-ACTION:mailto:me@example.org\r\n",
    )
}

struct Seeded {
    h: Harness,
    inbox: envio_core::FolderId,
}

/// An Inbox holding `mails` as uids 1, 2, ...
fn seeded(mails: &[Vec<u8>]) -> Seeded {
    let mut h = Harness::unstarted();
    let key = h.seed_account(ME, Some("pw"));
    let numbered: Vec<(u32, &[u8])> = (1..)
        .zip(mails)
        .map(|(uid, mail)| (uid, mail.as_slice()))
        .collect();
    let inbox = h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &numbered);
    h.start_core();
    Seeded { h, inbox }
}

struct Offer {
    undo: UndoId,
    text: String,
    expires_at_ms: i64,
    scope: UndoScope,
}

impl Seeded {
    fn id_of(&self, uid: u32) -> MessageRowId {
        self.h.send(Command::ListMessages {
            list: envio_core::ListRef::Folder(self.inbox),
            after: None,
        });
        let subject = format!("message {uid}");
        self.h.wait(|e| match e {
            Event::Messages { rows, .. } => {
                rows.iter().find(|r| r.subject == subject).map(|r| r.id)
            }
            _ => None,
        })
    }

    fn open(&self, id: MessageRowId) -> Option<InviteCard> {
        self.h.send(Command::OpenMessage {
            id,
            images: ImagesChoice::Default,
            width_px: 300,
        });
        self.card_of(id)
    }

    /// The card of the next `MessageOpened` of `id`.
    fn card_of(&self, id: MessageRowId) -> Option<InviteCard> {
        self.h.wait(|e| match e {
            Event::MessageOpened { id: i, view } if *i == id => Some(view.invite.clone()),
            _ => None,
        })
    }

    fn answer(&self, id: MessageRowId, answer: Answer, note: &str) {
        self.h.send(Command::AnswerInvite {
            id,
            answer,
            note: note.into(),
        });
    }

    fn offer(&self) -> Offer {
        self.h.wait(|e| match e {
            Event::UndoOffer {
                undo,
                text,
                expires_at_ms,
                scope,
            } => Some(Offer {
                undo: *undo,
                text: text.clone(),
                expires_at_ms: *expires_at_ms,
                scope: *scope,
            }),
            _ => None,
        })
    }

    fn outbox(&self) -> Outbox {
        Outbox::open(&self.h.paths().outbox()).unwrap()
    }

    fn items(&self) -> Vec<OutboxItem> {
        self.outbox().items().unwrap()
    }

    fn closed(&self, undo: UndoId) -> bool {
        self.h.wait(|e| match e {
            Event::UndoClosed { undo: u, undone } if *u == undo => Some(*undone),
            _ => None,
        })
    }

    fn notice(&self) -> String {
        self.h.wait(|e| match e {
            Event::Notice { text } => Some(text.clone()),
            _ => None,
        })
    }
}

#[test]
fn answer_enqueues_a_held_reply_to_the_organizer() {
    let s = seeded(&[request(0)]);
    let id = s.id_of(1);
    s.open(id);
    let before = support::now_ms();
    s.answer(id, Answer::Accepted, "See you there");
    let offer = s.offer();
    assert_eq!(offer.scope, UndoScope::Reader);
    assert!(offer.text.contains("Accepting"), "{}", offer.text);
    let items = s.items();
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item.meta.recipients, vec!["ada@example.org".to_owned()]);
    assert_eq!(item.meta.state, SendState::Queued);
    assert!(item.meta.send_after_ms >= before + envio_sync::UNDO_MS);
    assert_eq!(offer.expires_at_ms, item.meta.send_after_ms);
    let eml = String::from_utf8(s.outbox().eml(&item.id).unwrap()).unwrap();
    assert!(eml.contains("Subject: Accepted: Planning"), "{eml}");
    assert!(eml.contains("method=\"REPLY\""), "{eml}");
    let (reply, _) = envio_mime::invite_of(eml.as_bytes()).unwrap();
    assert_eq!(reply.method, envio_mime::InviteMethod::Reply);
    assert_eq!(reply.attendees.len(), 1);
    assert_eq!(
        reply.attendees[0].partstat,
        Some(envio_mime::PartStat::Accepted)
    );
    assert!(eml.contains("See you there"), "{eml}");
}

#[test]
fn answer_remembers_and_card_says_you_accepted() {
    let s = seeded(&[request(0)]);
    let id = s.id_of(1);
    let card = s.open(id).unwrap();
    assert_eq!(card.state, CardState::Open);
    assert!(card.can_answer && card.answer.is_none());
    assert_eq!(card.summary, "Planning");
    s.answer(id, Answer::Accepted, "");
    let card = s.card_of(id).unwrap();
    assert_eq!(card.state, CardState::Answered);
    assert_eq!(card.answer, Some(Answer::Accepted));
    assert_eq!(card.state_text, "You accepted");
    assert!(card.sending && !card.send_failed);
    let remembered = load_invites(&s.h.paths().invites()).unwrap();
    assert_eq!(remembered.len(), 1);
    assert_eq!(remembered[0].answer, "accepted");
    assert_eq!(remembered[0].reply_message_id, s.items()[0].meta.message_id);
    assert!(remembered[0].event_end_unix > 0);
    // Reopening shows the same.
    let again = s.open(id).unwrap();
    assert_eq!(again.state_text, "You accepted");
}

#[test]
fn undo_within_hold_deletes_reply_and_answer() {
    let s = seeded(&[request(0)]);
    let id = s.id_of(1);
    s.open(id);
    s.answer(id, Answer::Declined, "");
    let offer = s.offer();
    s.card_of(id);
    s.h.send(Command::Undo { undo: offer.undo });
    assert!(s.closed(offer.undo));
    let card = s.card_of(id).unwrap();
    assert_eq!(card.state, CardState::Open);
    assert!(s.items().is_empty());
    assert!(load_invites(&s.h.paths().invites()).unwrap().is_empty());
}

#[test]
fn undo_of_a_changed_answer_restores_the_previous_one() {
    let s = seeded(&[request(0)]);
    let id = s.id_of(1);
    s.open(id);
    s.answer(id, Answer::Accepted, "");
    let first = s.offer();
    // The first answer's window ends: it is final.
    let wait = u64::try_from(first.expires_at_ms - support::now_ms()).unwrap_or(0) + 300;
    std::thread::sleep(Duration::from_millis(wait));
    assert!(!s.closed(first.undo));
    s.answer(id, Answer::Declined, "");
    let second = s.offer();
    let card = s.card_of(id).unwrap();
    assert_eq!(card.answer, Some(Answer::Declined));
    s.h.send(Command::Undo { undo: second.undo });
    assert!(s.closed(second.undo));
    let card = s.card_of(id).unwrap();
    assert_eq!(card.state, CardState::Answered);
    assert_eq!(card.answer, Some(Answer::Accepted));
    let remembered = load_invites(&s.h.paths().invites()).unwrap();
    assert_eq!(remembered.len(), 1);
    assert_eq!(remembered[0].answer, "accepted");
}

#[test]
fn window_end_closes_the_offer_and_sends() {
    let s = seeded(&[request(0)]);
    let id = s.id_of(1);
    s.open(id);
    s.answer(id, Answer::Tentative, "");
    let offer = s.offer();
    let held = s.items().remove(0).meta;
    assert!(envio_sync::plan::outbox::held(&held, support::now_ms()));
    let wait = u64::try_from(offer.expires_at_ms - support::now_ms()).unwrap_or(0) + 300;
    std::thread::sleep(Duration::from_millis(wait));
    assert!(!s.closed(offer.undo), "the window ended: not undone");
    assert!(!envio_sync::plan::outbox::held(&held, support::now_ms()));
    // Undo now is too late and changes nothing.
    s.h.send(Command::Undo { undo: offer.undo });
    assert!(s.notice().contains("late"));
    assert_eq!(s.items().len(), 1);
    assert_eq!(load_invites(&s.h.paths().invites()).unwrap().len(), 1);
}

#[test]
fn a_second_answer_replaces_the_held_reply() {
    let s = seeded(&[request(0)]);
    let id = s.id_of(1);
    s.open(id);
    s.answer(id, Answer::Accepted, "");
    let first = s.offer();
    s.card_of(id);
    s.answer(id, Answer::Declined, "");
    assert!(
        !s.closed(first.undo),
        "the replaced offer closes, not undone"
    );
    s.offer();
    let items = s.items();
    assert_eq!(items.len(), 1);
    let eml = String::from_utf8(s.outbox().eml(&items[0].id).unwrap()).unwrap();
    assert!(eml.contains("Subject: Declined: Planning"), "{eml}");
    let remembered = load_invites(&s.h.paths().invites()).unwrap();
    assert_eq!(remembered.len(), 1);
    assert_eq!(remembered[0].reply_message_id, items[0].meta.message_id);
}

#[test]
fn undo_of_a_double_answer_goes_back_to_before_the_first() {
    let s = seeded(&[request(0)]);
    let id = s.id_of(1);
    s.open(id);
    s.answer(id, Answer::Accepted, "");
    s.offer();
    s.answer(id, Answer::Declined, "");
    let second = s.offer();
    s.h.send(Command::Undo { undo: second.undo });
    assert!(s.closed(second.undo));
    assert!(s.items().is_empty());
    assert!(load_invites(&s.h.paths().invites()).unwrap().is_empty());
    let card = s.open(id).unwrap();
    assert_eq!(card.state, CardState::Open);
}

#[test]
fn undo_is_refused_when_the_reply_is_no_longer_queued() {
    let s = seeded(&[request(0)]);
    let id = s.id_of(1);
    s.open(id);
    s.answer(id, Answer::Accepted, "");
    let offer = s.offer();
    let item = s.items().remove(0);
    s.outbox().set_state(&item.id, SendState::Sending).unwrap();
    s.h.send(Command::Undo { undo: offer.undo });
    assert!(!s.closed(offer.undo));
    assert_eq!(s.items().len(), 1);
    assert_eq!(load_invites(&s.h.paths().invites()).unwrap().len(), 1);
}

#[test]
fn undo_is_refused_when_the_reply_is_gone() {
    let s = seeded(&[request(0)]);
    let id = s.id_of(1);
    s.open(id);
    s.answer(id, Answer::Accepted, "");
    let offer = s.offer();
    let item = s.items().remove(0);
    s.outbox().remove(&item.id).unwrap();
    s.h.send(Command::Undo { undo: offer.undo });
    assert!(!s.closed(offer.undo));
    assert_eq!(load_invites(&s.h.paths().invites()).unwrap().len(), 1);
}

#[test]
fn undo_of_b_after_a_was_released_goes_back_to_a() {
    let s = seeded(&[request(0)]);
    let id = s.id_of(1);
    s.open(id);
    s.answer(id, Answer::Accepted, "");
    let _first = s.offer();
    let a = s.items().remove(0);
    // A is released and sending, though its offer is still in the book.
    s.outbox().set_state(&a.id, SendState::Sending).unwrap();
    s.answer(id, Answer::Declined, "");
    let second = s.offer();
    assert_eq!(s.items().len(), 2, "A's item is untouched");
    s.h.send(Command::Undo { undo: second.undo });
    assert!(s.closed(second.undo));
    let items = s.items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, a.id);
    let remembered = load_invites(&s.h.paths().invites()).unwrap();
    assert_eq!(remembered.len(), 1);
    assert_eq!(remembered[0].reply_message_id, a.meta.message_id);
    let card = s.open(id).unwrap();
    assert_eq!(card.answer, Some(Answer::Accepted));
}

#[test]
fn answer_offer_has_reader_scope_and_actions_have_list_scope() {
    let s = seeded(&[request(0)]);
    let id = s.id_of(1);
    s.open(id);
    s.answer(id, Answer::Accepted, "");
    assert_eq!(s.offer().scope, UndoScope::Reader);
    s.h.send(Command::Act {
        ids: vec![id],
        action: UserAction::Flag(true),
    });
    assert_eq!(s.offer().scope, UndoScope::List);
}

#[test]
fn failed_reply_shows_send_failed_beside_the_answer() {
    let s = seeded(&[request(0)]);
    let id = s.id_of(1);
    s.open(id);
    s.answer(id, Answer::Accepted, "");
    s.card_of(id);
    let item = s.items().remove(0);
    s.outbox()
        .set_state(
            &item.id,
            SendState::Failed {
                reason: "no route".into(),
            },
        )
        .unwrap();
    let card = s.open(id).unwrap();
    assert_eq!(card.state, CardState::Answered);
    assert_eq!(card.state_text, "You accepted");
    assert!(card.send_failed && !card.sending);
}

#[test]
fn higher_sequence_shows_updated() {
    let s = seeded(&[request(0), request(1)]);
    let first = s.id_of(1);
    let second = s.id_of(2);
    s.open(first);
    s.answer(first, Answer::Accepted, "");
    s.card_of(first);
    let card = s.open(second).unwrap();
    assert_eq!(card.state, CardState::Updated);
    assert_eq!(card.state_text, "Updated");
    assert!(card.can_answer && card.answer.is_none() && !card.sending);
}

#[test]
fn cancel_card_has_no_buttons() {
    let mail = invite_mail("CANCEL", 1, true, "");
    let s = seeded(&[mail]);
    let id = s.id_of(1);
    let card = s.open(id).unwrap();
    assert_eq!(card.state, CardState::Cancelled);
    assert_eq!(card.state_text, "Cancelled");
    assert!(!card.can_answer);
    s.answer(id, Answer::Accepted, "");
    assert!(s.notice().contains("can't be answered"));
    assert!(s.items().is_empty());
}

#[test]
fn reply_card_names_the_attendee() {
    let mail = invite_mail(
        "REPLY",
        0,
        true,
        "ATTENDEE;CN=Grace;PARTSTAT=ACCEPTED:mailto:grace@example.org\r\n",
    );
    let s = seeded(&[mail]);
    let card = s.open(s.id_of(1)).unwrap();
    assert_eq!(card.state, CardState::Replied);
    assert_eq!(card.state_text, "Grace accepted");
    assert!(!card.can_answer);
}

#[test]
fn no_organizer_cannot_answer() {
    let mail = invite_mail("REQUEST", 0, false, "");
    let s = seeded(&[mail]);
    let id = s.id_of(1);
    let card = s.open(id).unwrap();
    assert_eq!(card.state, CardState::Open);
    assert!(!card.can_answer);
    s.answer(id, Answer::Declined, "");
    assert!(s.notice().contains("can't be answered"));
    assert!(s.items().is_empty());
    assert!(load_invites(&s.h.paths().invites()).unwrap().is_empty());
}

/// Undo when `invites.json` cannot be written: the reply stays held and the
/// card keeps its answer.
#[cfg(unix)]
#[test]
fn undo_keeps_the_reply_when_the_answer_cannot_be_restored() {
    use std::os::unix::fs::PermissionsExt;
    let s = seeded(&[request(0)]);
    let id = s.id_of(1);
    s.open(id);
    s.answer(id, Answer::Declined, "");
    let offer = s.offer();
    s.card_of(id);
    let invites = s.h.paths().invites();
    let dir = invites.parent().unwrap().to_owned();
    let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    // Root (CI's container) writes through file modes, and nothing else
    // makes the store's write fail without also making it read as damaged.
    let probe = dir.join(".probe");
    if std::fs::File::create(&probe).is_ok() {
        let _ = std::fs::remove_file(&probe);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(mode)).unwrap();
        eprintln!("skipped: a read-only folder is still writable here (root)");
        return;
    }
    s.h.send(Command::Undo { undo: offer.undo });
    let undone = s.closed(offer.undo);
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(mode)).unwrap();
    assert!(!undone);
    assert_eq!(s.items().len(), 1, "the reply is still held");
    assert_eq!(load_invites(&invites).unwrap()[0].answer, "declined");
}

#[test]
fn export_writes_the_invite_as_a_calendar_file() {
    let s = seeded(&[request(0)]);
    let id = s.id_of(1);
    let out = tempfile::tempdir().unwrap();
    let dir = out.path().join("attachments");
    let mut saved = Vec::new();
    for _ in 0..2 {
        s.h.send(Command::ExportInvite {
            id,
            dir: dir.clone(),
        });
        saved.push(s.h.wait(|e| match e {
            Event::InviteExported { id: i, path } if *i == id => Some(path.clone()),
            _ => None,
        }));
    }
    assert_eq!(saved[0], dir.join("Planning.ics"));
    assert_eq!(saved[1], dir.join("Planning (2).ics"));
    let ics = std::fs::read_to_string(&saved[0]).unwrap();
    assert!(ics.starts_with("BEGIN:VCALENDAR"), "{ics}");
    assert!(ics.contains("UID:u1@example.org"), "{ics}");
    assert!(ics.contains("DTSTART:20270115T080000Z"), "{ics}");
}

#[test]
fn export_of_mail_without_an_invite_fails_with_a_notice() {
    let plain = b"From: Ada <ada@example.org>\r\nSubject: hi\r\n\r\nno invite\r\n".to_vec();
    let s = seeded(&[plain]);
    let id = s.id_of(1);
    let out = tempfile::tempdir().unwrap();
    s.h.send(Command::ExportInvite {
        id,
        dir: out.path().to_owned(),
    });
    s.h.wait(|e| match e {
        Event::InviteExportFailed { id: i } if *i == id => Some(()),
        Event::InviteExported { .. } => panic!("exported mail without an invite"),
        _ => None,
    });
    assert_eq!(s.notice(), "Could not add the invite to the calendar");
}
