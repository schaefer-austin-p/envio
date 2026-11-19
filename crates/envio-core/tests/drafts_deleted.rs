//! The worker's own replay step removes the local draft whose copy in the
//! server's Drafts folder the user deleted (#284). Unlike the unit and
//! driver tests, this goes through the real Undo window, journal replay and
//! `Worker::replay`, so removing the call there makes it fail.

mod support;

use std::time::{Duration, Instant};

use envio_core::{Command, Event, FolderRole, ListRef, UserAction};
use envio_store::{Db, Draft, Drafts, Envelope, NewMessage};
use support::Harness;
use support::fake::{DRAFT_ID, Kind, serve};

#[test]
fn deleting_an_uploaded_draft_in_drafts_removes_its_local_file_once_replayed() {
    let mut h = Harness::unstarted();
    h.foreground = true;
    let server = serve(Kind::Drafts);
    let key = h.seed_account_at("me@example.org", Some("pw"), server.port);
    h.pin_imap(&key, &server.hex);
    let eml: &[u8] = b"From: Me <me@example.org>\r\nSubject: d\r\n\r\nbody\r\n";
    let folder = h.seed_folder_with(&key, "Drafts", FolderRole::Drafts, &[(1, eml)]);
    // The cached copy carries the Message-ID the draft file names.
    let db = Db::open(&h.paths().db()).unwrap();
    let row = db.message_at(folder, 1).unwrap().unwrap();
    let message = NewMessage {
        uid: 1,
        size: u32::try_from(eml.len()).unwrap(),
        envelope: Envelope {
            message_id: Some(DRAFT_ID.into()),
            subject: "d".into(),
            from: "Me <me@example.org>".into(),
            date: support::now_secs(),
            ..Envelope::default()
        },
        flags: row.flags,
        preview: String::new(),
    };
    db.upsert_message(folder, &message).unwrap();
    let drafts = Drafts::open(&h.paths().drafts()).unwrap();
    let id = drafts
        .create(&Draft {
            account: key.clone(),
            body: "body".into(),
            updated_at_ms: 5,
            ..Draft::default()
        })
        .unwrap();
    assert!(drafts.mark_uploaded(&id, DRAFT_ID, 5).unwrap());
    h.start_core();

    let list = ListRef::Folder(folder);
    h.send(Command::ListMessages { list, after: None });
    let rows = h.wait(|e| match e {
        Event::Messages { rows, .. } => Some(rows.clone()),
        _ => None,
    });
    assert_eq!(rows.len(), 1);
    h.send(Command::Act {
        ids: vec![rows[0].id],
        action: UserAction::Delete,
    });
    // The Undo window (5 s) holds the delete back, then the replay runs.
    let end = Instant::now() + Duration::from_secs(20);
    while drafts.load(&id).is_ok() {
        assert!(Instant::now() < end, "the local draft was never removed");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(drafts.deletes().unwrap().is_empty(), "no note either");
}
