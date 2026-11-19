//! A message in a Drafts-role folder can only be deleted (or moved to
//! Trash): any other way out would leave its local draft file behind
//! (#228 Decision 2). Core refuses it even where the UI does not offer it.

mod support;

use envio_core::api::view::MessageRow;
use envio_core::{Command, Event, FolderId, FolderRole, ListRef, UserAction};
use envio_store::Db;
use support::Harness;

const MAIL: &[u8] = b"From: Ada <ada@example.org>\r\nSubject: s\r\n\r\nbody\r\n";

fn rows_of(h: &Harness, folder: FolderId) -> Vec<MessageRow> {
    h.send(Command::ListMessages {
        list: ListRef::Folder(folder),
        after: None,
    });
    h.wait(|e| match e {
        Event::Messages {
            list: ListRef::Folder(f),
            rows,
            ..
        } if *f == folder => Some(rows.clone()),
        _ => None,
    })
}

fn journal_adds(h: &Harness) -> usize {
    std::fs::read_to_string(h.paths().journal())
        .unwrap_or_default()
        .lines()
        .filter(|l| l.contains("\"Add\"") || l.contains("\"add\""))
        .count()
}

fn notice(h: &Harness) -> String {
    h.wait(|e| match e {
        Event::Notice { text } => Some(text.clone()),
        _ => None,
    })
}

#[test]
fn a_move_out_of_drafts_is_refused_while_an_inbox_message_in_the_same_act_moves() {
    let mut h = Harness::unstarted();
    let key = h.seed_account("me@example.org", Some("pw"));
    let drafts = h.seed_folder_with(&key, "Drafts", FolderRole::Drafts, &[(1, MAIL)]);
    let inbox = h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[(1, MAIL)]);
    let db = Db::open(&h.paths().db()).unwrap();
    let misc = db.upsert_folder(&key, "Misc", FolderRole::Other).unwrap();
    drop(db);
    h.start_core();
    let draft_row = rows_of(&h, drafts)[0].clone();
    let inbox_row = rows_of(&h, inbox)[0].clone();
    h.send(Command::Act {
        ids: vec![draft_row.id, inbox_row.id],
        action: UserAction::Move { to: misc },
    });
    assert_eq!(notice(&h), "A draft can only be deleted.");
    assert_eq!(rows_of(&h, drafts).len(), 1, "the cache row stays");
    assert!(rows_of(&h, inbox).is_empty(), "the Inbox message moved");
    assert_eq!(journal_adds(&h), 1, "one pending action, not two");
}

#[test]
fn archive_from_drafts_records_nothing() {
    let mut h = Harness::unstarted();
    let key = h.seed_account("me@example.org", Some("pw"));
    let drafts = h.seed_folder_with(&key, "Drafts", FolderRole::Drafts, &[(1, MAIL)]);
    h.seed_folder_with(&key, "Archive", FolderRole::Archive, &[]);
    h.start_core();
    let row = rows_of(&h, drafts)[0].clone();
    h.send(Command::Act {
        ids: vec![row.id],
        action: UserAction::Archive,
    });
    assert_eq!(notice(&h), "A draft can only be deleted.");
    assert_eq!(rows_of(&h, drafts).len(), 1);
    assert_eq!(journal_adds(&h), 0);
}
