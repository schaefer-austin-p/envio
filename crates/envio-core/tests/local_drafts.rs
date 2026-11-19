//! The Drafts list shows local drafts that have no server copy as
//! pseudo-rows (#228): merged in per page, never twice, never for a draft
//! the user is deleting (#299's window), and the list is told when drafts
//! change. The account has no server, so journal actions stay pending.

mod support;

use envio_core::api::view::MessageRow;
use envio_core::{
    Command, ComposeId, DraftInput, Event, FolderId, FolderRole, ListRef, StartFrom, UndoId,
    UserAction,
};
use envio_store::{Db, Draft, Drafts, Envelope, NewMessage};
use support::Harness;

const ME: &str = "me@example.org";
const SERVER_ID: &str = "<server@example.org>";
const EML: &[u8] = b"From: Me <me@example.org>\r\nSubject: s\r\n\r\nbody\r\n";

struct S {
    h: Harness,
    key: String,
    drafts: Drafts,
    drafts_folder: FolderId,
    inbox: FolderId,
}

fn local(key: &str, subject: &str, at_ms: i64) -> Draft {
    Draft {
        account: key.into(),
        to: vec!["Bob <bob@example.org>".into()],
        subject: subject.into(),
        body: "body".into(),
        updated_at_ms: at_ms,
        ..Draft::default()
    }
}

/// A cached Drafts folder holding one server draft (Message-ID `SERVER_ID`)
/// and an Inbox; the core is started.
fn seeded() -> S {
    let h = Harness::unstarted();
    let key = h.seed_account(ME, Some("pw"));
    let drafts_folder = h.seed_folder_with(&key, "Drafts", FolderRole::Drafts, &[(1, EML)]);
    let inbox = h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[]);
    let db = Db::open(&h.paths().db()).unwrap();
    let row = db.message_at(drafts_folder, 1).unwrap().unwrap();
    let message = NewMessage {
        uid: 1,
        size: u32::try_from(EML.len()).unwrap(),
        envelope: Envelope {
            message_id: Some(SERVER_ID.into()),
            subject: "server".into(),
            from: "Me <me@example.org>".into(),
            to: "Cy <cy@example.org>".into(),
            date: 100,
            ..Envelope::default()
        },
        flags: row.flags,
        preview: String::new(),
    };
    db.upsert_message(drafts_folder, &message).unwrap();
    let drafts = Drafts::open(&h.paths().drafts()).unwrap();
    S {
        h,
        key,
        drafts,
        drafts_folder,
        inbox,
    }
}

impl S {
    fn start(&mut self) {
        self.h.start_core();
    }

    /// An uploaded local draft that names the server row.
    fn linked(&self) -> String {
        let id = self.drafts.create(&local(&self.key, "linked", 5)).unwrap();
        assert!(self.drafts.mark_uploaded(&id, SERVER_ID, 5).unwrap());
        id
    }

    fn rows(&self, folder: FolderId) -> Vec<MessageRow> {
        self.h.send(Command::ListMessages {
            list: ListRef::Folder(folder),
            after: None,
        });
        self.h.wait(|e| match e {
            Event::Messages { rows, .. } => Some(rows.clone()),
            _ => None,
        })
    }

    fn delete_server_row(&self, action: UserAction) -> UndoId {
        let rows = self.rows(self.drafts_folder);
        let server = rows.iter().find(|r| r.local_draft.is_none()).unwrap();
        self.h.send(Command::Act {
            ids: vec![server.id],
            action,
        });
        self.h.wait(|e| match e {
            Event::UndoOffer { undo, .. } => Some(*undo),
            _ => None,
        })
    }
}

fn subjects(rows: &[MessageRow]) -> Vec<&str> {
    rows.iter().map(|r| r.subject.as_str()).collect()
}

#[test]
fn a_draft_without_a_server_copy_shows_next_to_the_server_row_and_never_twice() {
    let mut s = seeded();
    s.linked();
    let unsent = s.drafts.create(&local(&s.key, "unsent", 200_000)).unwrap();
    s.start();
    let rows = s.rows(s.drafts_folder);
    assert_eq!(subjects(&rows), ["unsent", "server"]);
    assert_eq!(rows[0].local_draft.as_deref(), Some(unsent.as_str()));
    assert!(rows[0].id.0 < 0);
    assert_eq!(rows[0].from, "To: Bob");
    assert_eq!(rows[1].local_draft, None);
    assert_eq!(rows[1].from, "To: Cy");
    assert_eq!(rows[1].from_address, "cy@example.org");
}

#[test]
fn the_inbox_has_no_pseudo_rows() {
    let mut s = seeded();
    s.drafts.create(&local(&s.key, "unsent", 5_000)).unwrap();
    s.start();
    assert!(s.rows(s.inbox).is_empty());
}

#[test]
fn closing_a_compose_tells_the_list() {
    let mut s = seeded();
    s.start();
    let c = ComposeId(1);
    s.h.send(Command::StartCompose {
        compose: c,
        from: StartFrom::New {
            account: s.key.clone(),
        },
        draft_id: None,
        mailto: None,
    });
    s.h.send(Command::DraftChanged {
        compose: c,
        draft: DraftInput {
            account: s.key.clone(),
            to: vec!["ada@example.org".into()],
            body: "hi".into(),
            ..DraftInput::default()
        },
    });
    let account = s.h.wait(|e| match e {
        Event::LocalDraftsChanged { account } => Some(account.clone()),
        _ => None,
    });
    assert_eq!(account, s.key);
    s.h.send(Command::CloseCompose {
        compose: c,
        discard: true,
    });
    s.h.wait(|e| matches!(e, Event::LocalDraftsChanged { .. }).then_some(()));
}

#[test]
fn a_throttled_text_save_does_not_tell_the_list() {
    let mut s = seeded();
    s.start();
    let c = ComposeId(1);
    let typed = |body: &str| Command::DraftChanged {
        compose: c,
        draft: DraftInput {
            account: s.key.clone(),
            to: vec!["ada@example.org".into()],
            body: body.into(),
            ..DraftInput::default()
        },
    };
    s.h.send(Command::StartCompose {
        compose: c,
        from: StartFrom::New {
            account: s.key.clone(),
        },
        draft_id: None,
        mailto: None,
    });
    s.h.send(typed("one"));
    s.h.wait(|e| matches!(e, Event::LocalDraftsChanged { .. }).then_some(()));
    s.h.send(typed("two"));
    let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !s
        .drafts
        .list()
        .unwrap()
        .iter()
        .any(|(_, d)| d.body == "two")
    {
        assert!(std::time::Instant::now() < end, "the save never landed");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(
        !s.h.events
            .try_iter()
            .any(|e| matches!(e, Event::LocalDraftsChanged { .. }))
    );
}

#[test]
fn a_draft_being_deleted_shows_no_row_in_the_window_offline_or_after_undo() {
    for to_trash in [false, true] {
        let mut s = seeded();
        let trash =
            s.h.seed_folder_with(&s.key, "Trash", FolderRole::Trash, &[]);
        s.linked();
        s.start();
        let action = if to_trash {
            UserAction::Move { to: trash }
        } else {
            UserAction::Delete
        };
        s.delete_server_row(action);
        // In the Undo window: neither the server row nor a pseudo-row.
        assert!(s.rows(s.drafts_folder).is_empty());
        // Offline the action stays pending, and the draft stays hidden.
        std::thread::sleep(std::time::Duration::from_millis(5_500));
        assert!(s.rows(s.drafts_folder).is_empty());
    }
}

#[test]
fn undo_brings_the_server_row_back_with_no_pseudo_row() {
    let mut s = seeded();
    s.linked();
    s.start();
    let undo = s.delete_server_row(UserAction::Delete);
    assert!(s.rows(s.drafts_folder).is_empty());
    s.h.send(Command::Undo { undo });
    s.h.wait(|e| matches!(e, Event::UndoClosed { undone: true, .. }).then_some(()));
    let rows = s.rows(s.drafts_folder);
    assert_eq!(subjects(&rows), ["server"]);
    assert_eq!(rows[0].local_draft, None);
}

#[test]
fn a_draft_with_edits_the_server_copy_lacks_keeps_its_row_while_leaving() {
    let mut s = seeded();
    let id = s.linked();
    // A save after the upload: #299 keeps the file and uploads it again.
    s.drafts.save(&id, &local(&s.key, "edited", 9)).unwrap();
    s.start();
    s.delete_server_row(UserAction::Delete);
    assert_eq!(subjects(&s.rows(s.drafts_folder)), ["edited"]);
}

#[test]
fn a_remote_delete_leaves_the_draft_showing_as_not_uploaded() {
    let mut s = seeded();
    s.linked();
    // The uploaded copy is gone from the cache and nothing is pending here.
    Db::open(&s.h.paths().db())
        .unwrap()
        .delete_messages(s.drafts_folder, &[1])
        .unwrap();
    s.start();
    assert_eq!(subjects(&s.rows(s.drafts_folder)), ["linked"]);
}

#[test]
fn sending_a_draft_tells_the_list_its_file_is_gone() {
    let mut s = seeded();
    s.start();
    let c = ComposeId(1);
    let draft = DraftInput {
        account: s.key.clone(),
        to: vec!["ada@example.org".into()],
        subject: "out".into(),
        body: "hi".into(),
        ..DraftInput::default()
    };
    s.h.send(Command::StartCompose {
        compose: c,
        from: StartFrom::New {
            account: s.key.clone(),
        },
        draft_id: None,
        mailto: None,
    });
    s.h.send(Command::DraftChanged {
        compose: c,
        draft: draft.clone(),
    });
    // The first save creates the file and tells the list.
    s.h.wait(|e| matches!(e, Event::LocalDraftsChanged { .. }).then_some(()));
    s.h.send(Command::Send { compose: c, draft });
    // The send's own deletion tells it again, after the file is gone.
    s.h.wait(|e| matches!(e, Event::LocalDraftsChanged { .. }).then_some(()));
    assert!(s.drafts.list().unwrap().is_empty());
}
