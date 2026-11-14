//! The Outbox: states, the user's decisions, Edit, and damaged items.

mod support;

use std::time::{Duration, Instant};

use envio_core::{Command, ComposeId, Event, OutboxDecision, OutboxRow, OutboxStatus};
use envio_store::{Drafts, NewOutboxItem, Outbox, SendState};
use support::Harness;

const ME: &str = "me@example.org";
const C: ComposeId = ComposeId(1);

const EML: &[u8] = b"From: me@example.org\r\nTo: Ada <ada@example.org>\r\n\
Subject: Plans\r\nMessage-ID: <q1@example.org>\r\nMIME-Version: 1.0\r\n\
Content-Type: text/plain; charset=utf-8\r\n\r\nSee you\r\n";

/// An unstarted harness with one account.
fn unstarted() -> (Harness, String) {
    let h = Harness::unstarted();
    let key = h.seed_account(ME, Some("pw"));
    (h, key)
}

/// Queues `EML` for `account` in the given state; returns the item id.
fn seed_item(h: &Harness, account: &str, state: SendState) -> String {
    let outbox = Outbox::open(&h.paths().outbox()).unwrap();
    let item = outbox
        .enqueue(
            NewOutboxItem {
                send_after_ms: 0,
                account: account.to_owned(),
                message_id: "<q1@example.org>".into(),
                queued_at_ms: 5_000,
                from: ME.into(),
                recipients: vec!["ada@example.org".into(), "hidden@example.org".into()],
            },
            EML,
        )
        .unwrap();
    if state != SendState::Queued {
        outbox.set_state(&item.id, state).unwrap();
    }
    item.id
}

fn rows(h: &Harness) -> Vec<OutboxRow> {
    h.send(Command::ListOutbox);
    h.wait(|e| match e {
        Event::Outbox { items } => Some(items.clone()),
        _ => None,
    })
}

enum Answer {
    Rows(Vec<OutboxRow>),
    Notice,
}

/// What a decision answered: the new list, or a notice.
fn decide(h: &Harness, item: &str, decision: OutboxDecision) -> Answer {
    h.send(Command::DecideOutbox {
        item: item.to_owned(),
        decision,
    });
    h.wait(|e| match e {
        Event::Outbox { items } => Some(Answer::Rows(items.clone())),
        Event::Notice { .. } => Some(Answer::Notice),
        _ => None,
    })
}

fn failed() -> SendState {
    SendState::Failed {
        reason: "Rejected".into(),
    }
}

#[test]
fn the_outbox_lists_states_and_damaged_items() {
    let (mut h, key) = unstarted();
    let queued = seed_item(&h, &key, SendState::Queued);
    let refused = seed_item(&h, &key, failed());
    let broken = seed_item(&h, &key, SendState::Queued);
    std::fs::remove_file(h.paths().outbox().join(format!("{broken}.eml"))).unwrap();
    h.start_core();
    let list = rows(&h);
    let status = |id: &str| list.iter().find(|r| r.id == id).map(|r| r.status.clone());
    assert_eq!(list.len(), 3);
    assert_eq!(status(&queued), Some(OutboxStatus::Queued));
    assert_eq!(
        status(&refused),
        Some(OutboxStatus::Failed {
            reason: "Rejected".into()
        })
    );
    assert_eq!(status(&broken), Some(OutboxStatus::Damaged));
    let row = list.iter().find(|r| r.id == queued).unwrap();
    assert_eq!(row.subject, "Plans");
    assert_eq!(row.recipients, ["ada@example.org", "hidden@example.org"]);
    assert_eq!(row.account, key);
    assert!(!row.text.is_empty());
}

#[test]
fn retry_requeues_a_failed_item() {
    let (mut h, key) = unstarted();
    let item = seed_item(&h, &key, failed());
    h.start_core();
    let Answer::Rows(list) = decide(&h, &item, OutboxDecision::Retry) else {
        panic!("a Failed item can be retried");
    };
    assert_eq!(list[0].status, OutboxStatus::Queued);
}

#[test]
fn send_again_on_an_unasked_item_is_refused() {
    let (mut h, key) = unstarted();
    let item = seed_item(&h, &key, SendState::MayHaveBeenSent);
    h.start_core();
    for decision in [
        OutboxDecision::SendAgain,
        OutboxDecision::MoveToSent,
        OutboxDecision::Delete,
        OutboxDecision::Retry,
    ] {
        assert!(matches!(decide(&h, &item, decision), Answer::Notice));
    }
    assert_eq!(
        rows(&h)[0].status,
        OutboxStatus::MayHaveBeenSent { asked: false }
    );
}

#[test]
fn delete_removes_a_failed_item() {
    let (mut h, key) = unstarted();
    let item = seed_item(&h, &key, failed());
    h.start_core();
    let Answer::Rows(list) = decide(&h, &item, OutboxDecision::Delete) else {
        panic!("a Failed item can be deleted");
    };
    assert!(list.is_empty());
    assert!(
        Outbox::open(&h.paths().outbox())
            .unwrap()
            .items()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_damaged_item_can_be_deleted() {
    let (mut h, key) = unstarted();
    let item = seed_item(&h, &key, SendState::Queued);
    std::fs::remove_file(h.paths().outbox().join(format!("{item}.eml"))).unwrap();
    h.start_core();
    assert_eq!(rows(&h)[0].status, OutboxStatus::Damaged);
    assert!(matches!(
        decide(&h, &item, OutboxDecision::Retry),
        Answer::Notice
    ));
    let Answer::Rows(list) = decide(&h, &item, OutboxDecision::Delete) else {
        panic!("a damaged item can be deleted");
    };
    assert!(list.is_empty());
    let outbox = Outbox::open(&h.paths().outbox()).unwrap();
    assert!(outbox.damaged().unwrap().is_empty());
}

#[test]
fn a_restart_never_requeues_a_maybe_sent_item() {
    let (mut h, key) = unstarted();
    let item = seed_item(&h, &key, SendState::MayHaveBeenSent);
    h.start_core();
    assert_eq!(
        rows(&h)[0].status,
        OutboxStatus::MayHaveBeenSent { asked: false }
    );
    h.restart();
    let list = rows(&h);
    assert_eq!(list[0].id, item);
    assert_eq!(
        list[0].status,
        OutboxStatus::MayHaveBeenSent { asked: false }
    );
    let stored = Outbox::open(&h.paths().outbox()).unwrap().items().unwrap();
    assert_eq!(stored[0].meta.state, SendState::MayHaveBeenSent);
}

#[test]
fn edit_turns_a_failed_item_into_a_draft_then_removes_it() {
    let (mut h, key) = unstarted();
    let item = seed_item(&h, &key, failed());
    h.start_core();
    h.send(Command::EditOutbox {
        item: item.clone(),
        compose: C,
    });
    let draft = h.wait(|e| match e {
        Event::ComposeReady { compose, draft, .. } if *compose == C => Some(draft.clone()),
        _ => None,
    });
    assert_eq!(draft.subject, "Plans");
    assert_eq!(draft.to, ["Ada <ada@example.org>"]);
    assert_eq!(draft.bcc, ["hidden@example.org"]);
    assert_eq!(draft.account, key);
    let draft_id = h.wait(|e| match e {
        Event::DraftSaved { compose, draft_id } if *compose == C => Some(draft_id.clone()),
        _ => None,
    });
    assert_eq!(draft.draft_id.as_deref(), Some(draft_id.as_str()));
    let items = h.wait(|e| match e {
        Event::Outbox { items } => Some(items.clone()),
        _ => None,
    });
    assert!(items.is_empty());
    let drafts = Drafts::open(&h.paths().drafts()).unwrap();
    let stored = drafts.list().unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].0, draft_id);
    // Later changes of that window go into the same draft.
    let mut changed = draft;
    changed.body = "Changed".into();
    h.send(Command::DraftChanged {
        compose: C,
        draft: changed,
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let now = drafts.list().unwrap();
        if now.len() == 1 && now[0].1.body == "Changed" {
            break;
        }
        assert!(Instant::now() < deadline, "the change was not saved");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn edit_refuses_an_item_that_is_not_failed() {
    let (mut h, key) = unstarted();
    let item = seed_item(&h, &key, SendState::Queued);
    h.start_core();
    h.send(Command::EditOutbox { item, compose: C });
    h.wait(|e| matches!(e, Event::Notice { .. }).then_some(()));
    assert_eq!(rows(&h).len(), 1);
    let drafts = Drafts::open(&h.paths().drafts()).unwrap();
    assert!(drafts.list().unwrap().is_empty());
}

#[test]
fn a_send_lists_the_outbox() {
    let (mut h, key) = unstarted();
    h.start_core();
    h.send(Command::Send {
        compose: C,
        draft: envio_core::DraftInput {
            account: key,
            to: vec!["ada@example.org".into()],
            subject: "Plans".into(),
            body: "See you".into(),
            ..envio_core::DraftInput::default()
        },
    });
    let items = h.wait(|e| match e {
        Event::Outbox { items } => Some(items.clone()),
        _ => None,
    });
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].subject, "Plans");
}

#[test]
fn edit_of_an_unknown_account_is_refused() {
    let (mut h, _) = unstarted();
    // Its account was removed (or never existed): no draft can be sent from it.
    let item = seed_item(&h, "9999999", failed());
    h.start_core();
    h.send(Command::EditOutbox { item, compose: C });
    h.wait(|e| matches!(e, Event::Notice { .. }).then_some(()));
    assert_eq!(rows(&h).len(), 1);
    let drafts = Drafts::open(&h.paths().drafts()).unwrap();
    assert!(drafts.list().unwrap().is_empty());
}
