//! The worker's own replay step removes the local draft whose copy in the
//! server's Drafts folder the user deleted (#284). Unlike the unit and
//! driver tests, this goes through the real Undo window, journal replay and
//! `Worker::replay`, so removing the call there makes it fail.

mod support;

use std::time::{Duration, Instant};

use envio_core::{AccountStatus, Command, Event, FolderId, FolderRole, ListRef, UserAction};
use envio_store::{ActionKind, Db, Draft, Drafts, Envelope, Journal, MessageRef, NewMessage};
use support::Harness;
use support::fake::{DRAFT_ID, Kind, TlsServer, serve};

/// A cached Drafts folder with one message, and the uploaded local draft
/// that names it; the core is not started yet.
fn seeded() -> (Harness, TlsServer, FolderId, Drafts, String) {
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
    (h, server, folder, drafts, id)
}

#[test]
fn deleting_an_uploaded_draft_in_drafts_removes_its_local_file_once_replayed() {
    let (mut h, _server, folder, drafts, id) = seeded();
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

#[test]
fn a_replay_that_fails_after_the_commit_still_forgets_the_draft() {
    let (mut h, _server, folder, drafts, id) = seeded();
    // An intent that stays (its folder is unknown) makes the sidecar exist,
    // so the commit below needs no new file in the data folder.
    let key = drafts.load(&id).unwrap().account;
    let mut journal = Journal::open(&h.paths().journal()).unwrap();
    let other = MessageRef {
        account: key,
        folder: "Elsewhere".into(),
        uidvalidity: 1,
        uid: 1,
        message_id: Some("<z@x>".into()),
    };
    let seeded = journal.push(0, other, ActionKind::Delete).unwrap();
    journal.commit(seeded.seq).unwrap();
    drop(journal);
    h.start_core();
    let list = ListRef::Folder(folder);
    h.send(Command::ListMessages { list, after: None });
    let rows = h.wait(|e| match e {
        Event::Messages { rows, .. } => Some(rows.clone()),
        _ => None,
    });
    // The journal's compact fails once: the replay commits the delete and
    // then returns an error.
    envio_core::hooks::fail_next_journal_compact(h.paths().journal());
    h.send(Command::Act {
        ids: vec![rows[0].id],
        action: UserAction::Delete,
    });
    // The failed replay shows as a local error. The draft must be gone by
    // then, not only after the retry that follows.
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        let left = end.saturating_duration_since(Instant::now());
        match h.events.recv_timeout(left) {
            Ok(Event::AccountStatus {
                status: AccountStatus::LocalError { .. },
                ..
            }) => break,
            Ok(_) => {}
            Err(_) => panic!("the replay never failed"),
        }
    }
    assert!(
        drafts.load(&id).is_err(),
        "the failed replay left the draft"
    );
}
