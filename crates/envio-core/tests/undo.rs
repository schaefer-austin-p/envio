//! User actions and the Undo window.

mod support;

use std::time::{Duration, Instant};

use envio_core::api::view::MessageRow;
use envio_core::{Command, Event, FolderId, FolderRole, Page, UndoId, UserAction};
use envio_store::{Db, Flags, SyncState};
use support::Harness;

const ME: &str = "me@example.org";
const MAIL: &[u8] = b"From: Ada <ada@example.org>\r\nSubject: hi\r\n\r\nHello\r\n";

struct Seeded {
    h: Harness,
    inbox: FolderId,
    other: FolderId,
}

/// An Inbox with two messages and an empty folder named Stuff, which is
/// the Archive folder when `archive` says so.
fn seeded(archive: bool) -> Seeded {
    let mut h = Harness::unstarted();
    let key = h.seed_account(ME, Some("pw"));
    let inbox = h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[(1, MAIL), (2, MAIL)]);
    let role = if archive {
        FolderRole::Archive
    } else {
        FolderRole::Other
    };
    let other = h.seed_folder_with(&key, "Stuff", role, &[]);
    h.start_core();
    Seeded { h, inbox, other }
}

impl Seeded {
    fn rows(&self) -> Vec<MessageRow> {
        self.h.send(Command::ListMessages {
            folder: self.inbox,
            page: Page {
                offset: 0,
                limit: 50,
            },
        });
        self.h.wait(|e| match e {
            Event::Messages { rows, .. } => Some(rows.clone()),
            _ => None,
        })
    }

    fn act(&self, rows: &[MessageRow], action: UserAction) {
        self.h.send(Command::Act {
            ids: rows.iter().map(|r| r.id).collect(),
            action,
        });
    }

    fn offer(&self) -> (UndoId, String, i64) {
        self.h.wait(|e| match e {
            Event::UndoOffer {
                undo,
                text,
                expires_at_ms,
            } => Some((*undo, text.clone(), *expires_at_ms)),
            _ => None,
        })
    }
}

fn closed(h: &Harness, id: UndoId) -> bool {
    h.wait(|e| match e {
        Event::UndoClosed { undo, undone } if *undo == id => Some(*undone),
        _ => None,
    })
}

#[test]
fn delete_hides_at_once_and_offers_undo() {
    let s = seeded(true);
    let rows = s.rows();
    let before = support::now_ms();
    s.act(&rows[..1], UserAction::Delete);
    let (_, text, expires) = s.offer();
    assert!(text.contains("Deleted"), "{text}");
    assert!(expires >= before + 5_000 && expires < before + 10_000);
    assert_eq!(s.rows().len(), 1);
}

#[test]
fn undo_within_the_window_restores_the_row() {
    let s = seeded(true);
    let rows = s.rows();
    s.act(&rows[..1], UserAction::Delete);
    let (undo, ..) = s.offer();
    assert_eq!(s.rows().len(), 1);
    s.h.send(Command::Undo { undo });
    assert!(closed(&s.h, undo));
    assert_eq!(s.rows().len(), 2);
}

#[test]
fn move_undo_restores_the_row() {
    let s = seeded(true);
    let rows = s.rows();
    s.act(&rows[1..], UserAction::Move { to: s.other });
    let (undo, ..) = s.offer();
    assert_eq!(s.rows().len(), 1);
    s.h.send(Command::Undo { undo });
    assert!(closed(&s.h, undo));
    assert_eq!(s.rows().len(), 2);
}

#[test]
fn undo_after_expiry_is_too_late() {
    let s = seeded(true);
    let rows = s.rows();
    s.act(&rows[..1], UserAction::Delete);
    let (undo, _, expires) = s.offer();
    let wait = u64::try_from(expires - support::now_ms()).unwrap_or(0) + 300;
    std::thread::sleep(Duration::from_millis(wait));
    s.h.send(Command::Undo { undo });
    let notice = s.h.wait(|e| match e {
        Event::Notice { text } if text.contains("late") => Some(text.clone()),
        _ => None,
    });
    assert!(notice.contains("late"));
    // The row stays gone.
    assert_eq!(s.rows().len(), 1);
}

#[test]
fn the_offer_closes_after_five_seconds() {
    let s = seeded(true);
    let rows = s.rows();
    s.act(&rows[..1], UserAction::Delete);
    let (undo, _, expires) = s.offer();
    // `Harness::wait` gives up after 5 s: wait for the close by hand.
    let deadline = Instant::now() + Duration::from_secs(8);
    let undone = loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match s.h.events.recv_timeout(left) {
            Ok(Event::UndoClosed { undo: u, undone }) if u == undo => break undone,
            Ok(_) => {}
            Err(_) => panic!("the offer never closed"),
        }
    };
    assert!(!undone);
    assert!(support::now_ms() >= expires);
}

#[test]
fn archive_without_an_archive_folder_is_refused_with_a_notice() {
    let s = seeded(false);
    let rows = s.rows();
    s.act(&rows[..1], UserAction::Archive);
    let text = s.h.wait(|e| match e {
        Event::Notice { text } if text.contains("Archive") => Some(text.clone()),
        _ => None,
    });
    assert!(text.contains("Archive"));
    // Nothing moved and nothing was offered.
    assert_eq!(s.rows().len(), 2);
    while let Ok(event) = s.h.events.try_recv() {
        assert!(!matches!(event, Event::UndoOffer { .. }), "{event:?}");
    }
}

#[test]
fn a_flag_undo_queues_a_flag_check() {
    let mut h = Harness::unstarted();
    let key = h.seed_account(ME, Some("pw"));
    let inbox = h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[(1, MAIL)]);
    // The folder has a CONDSTORE position, which an undo must clear: the
    // cache may be ahead of the server's flags, and the next sync would
    // skip the message.
    let db = Db::open(&h.paths().db()).unwrap();
    db.set_sync_state(
        inbox,
        &SyncState {
            uidvalidity: 1,
            uidnext: 2,
            highestmodseq: Some(7),
        },
    )
    .unwrap();
    drop(db);
    h.start_core();
    let s = Seeded {
        h,
        inbox,
        other: inbox,
    };
    let rows = s.rows();
    s.act(&rows, UserAction::Flag(true));
    let (undo, ..) = s.offer();
    assert!(s.rows()[0].flags.contains(Flags::FLAGGED));
    s.h.send(Command::Undo { undo });
    assert!(closed(&s.h, undo));
    assert!(!s.rows()[0].flags.contains(Flags::FLAGGED));
    let db = Db::open(&s.h.paths().db()).unwrap();
    assert_eq!(db.folder(inbox).unwrap().sync.unwrap().highestmodseq, None);
}
