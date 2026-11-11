//! What the user is told when the journal cannot be sure of a write, and
//! when it skipped records at start.

mod support;

use envio_core::hooks::fail_next_journal_sync;
use envio_core::{Command, Event, FolderId, FolderRole, UserAction};
use std::time::Duration;
use support::Harness;

const ME: &str = "me@example.org";
const MAIL: &[u8] = b"From: Ada <ada@example.org>\r\nSubject: hi\r\n\r\nHello\r\n";

fn seeded() -> (Harness, FolderId) {
    let mut h = Harness::unstarted();
    let key = h.seed_account(ME, Some("pw"));
    let inbox = h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[(1, MAIL), (2, MAIL)]);
    h.start_core();
    (h, inbox)
}

fn rows(h: &Harness, inbox: FolderId) -> Vec<envio_core::api::view::MessageRow> {
    h.send(Command::ListMessages {
        list: envio_core::ListRef::Folder(inbox),
        after: None,
    });
    h.wait(|e| match e {
        Event::Messages { rows, .. } => Some(rows.clone()),
        _ => None,
    })
}

fn notice(h: &Harness) -> String {
    h.wait(|e| match e {
        Event::Notice { text } => Some(text.clone()),
        _ => None,
    })
}

#[test]
fn a_journal_write_that_may_still_apply_says_so() {
    let (h, inbox) = seeded();
    let rows = rows(&h, inbox);
    fail_next_journal_sync(h.paths().journal());
    h.send(Command::Act {
        ids: vec![rows[0].id],
        action: UserAction::Delete,
    });
    assert_eq!(
        notice(&h),
        "This change couldn't be saved on the phone. It may still be applied."
    );
}

#[test]
fn an_undo_that_may_still_apply_says_so() {
    let (h, inbox) = seeded();
    let rows = rows(&h, inbox);
    h.send(Command::Act {
        ids: vec![rows[0].id],
        action: UserAction::Delete,
    });
    let undo = h.wait(|e| match e {
        Event::UndoOffer { undo, .. } => Some(*undo),
        _ => None,
    });
    fail_next_journal_sync(h.paths().journal());
    h.send(Command::Undo { undo });
    assert_eq!(
        notice(&h),
        "Undo couldn't be saved on the phone. The change may still be undone."
    );
}

const NEWER: &str = r#"{"v":7,"op":"add","action":{"seq":3,"at_ms":7,"anything":"new"}}"#;

#[test]
fn skipped_journal_records_are_noticed_at_start() {
    let mut h = Harness::unstarted();
    h.seed_account(ME, Some("pw"));
    let path = h.paths().journal();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("{NEWER}\n{NEWER}\n")).unwrap();
    h.start_core();
    assert_eq!(
        notice(&h),
        "2 pending changes could not be read and were skipped"
    );
}

#[test]
fn no_notice_when_nothing_was_skipped() {
    let (h, _) = seeded();
    h.wait(|e| matches!(e, Event::Started { .. }).then_some(()));
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !h.events
            .try_iter()
            .any(|e| matches!(e, Event::Notice { .. }))
    );
}
