//! Edit: reopening a cached message of a Drafts-role folder (#228, T1).

mod support;

use envio_core::{
    Command, ComposeId, DraftInput, Event, FolderId, FolderRole, ImagesChoice, ListRef,
    MessageRowId, MessageView, StartFrom,
};
use envio_store::{Draft, Drafts, Envelope, NewMessage};
use support::Harness;

const ME: &str = "me@example.org";
const MID: &str = "<d1@example.org>";
const RAW: &[u8] = b"From: me@example.org\r\nTo: ada@example.org\r\n\
Subject: server text\r\nMessage-ID: <d1@example.org>\r\n\r\nserver body\r\n";

struct Seeded {
    h: Harness,
    key: String,
    drafts: Drafts,
    /// The cached message in the Drafts folder, and one in the Inbox.
    in_drafts: MessageRowId,
    in_inbox: MessageRowId,
}

/// Sets the Message-ID of the cached message `uid` of `folder`.
fn give_message_id(h: &Harness, folder: FolderId, uid: u32, message_id: &str) {
    let db = envio_store::Db::open(&h.paths().db()).unwrap();
    let message = NewMessage {
        uid,
        size: u32::try_from(RAW.len()).unwrap(),
        envelope: Envelope {
            subject: format!("message {uid}"),
            // Recent, or the start-up prune drops the body.
            date: support::now_ms() / 1000,
            message_id: Some(message_id.into()),
            ..Envelope::default()
        },
        flags: envio_store::Flags::default(),
        preview: String::new(),
    };
    let id = db.upsert_message(folder, &message).unwrap();
    // The upsert starts the row's body again.
    db.set_body_state(id, envio_store::BodyState::Downloaded)
        .unwrap();
}

fn seeded() -> Seeded {
    let mut h = Harness::unstarted();
    let key = h.seed_account(ME, Some("pw"));
    let drafts_folder = h.seed_folder_with(&key, "Drafts", FolderRole::Drafts, &[(1, RAW)]);
    let inbox = h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[(1, RAW)]);
    give_message_id(&h, drafts_folder, 1, MID);
    give_message_id(&h, inbox, 1, MID);
    h.start_core();
    let drafts = Drafts::open(&h.paths().drafts()).unwrap();
    let id_in = |folder: FolderId| {
        h.send(Command::ListMessages {
            list: ListRef::Folder(folder),
            after: None,
        });
        h.wait(|e| match e {
            Event::Messages { rows, .. } => rows.first().map(|r| r.id),
            _ => None,
        })
    };
    let in_drafts = id_in(drafts_folder);
    let in_inbox = id_in(inbox);
    Seeded {
        h,
        key,
        drafts,
        in_drafts,
        in_inbox,
    }
}

impl Seeded {
    /// A local draft that is the file behind the server message `MID`.
    fn local(&self, message_id: &str, body: &str) -> String {
        let draft = Draft {
            account: self.key.clone(),
            to: vec!["ada@example.org".into()],
            subject: "local text".into(),
            body: body.into(),
            updated_at_ms: 5,
            ..Draft::default()
        };
        let id = self.drafts.create(&draft).unwrap();
        // `create` starts with no server copy: record the upload.
        self.drafts.mark_uploaded(&id, message_id, 5).unwrap();
        id
    }

    fn edit(&self, compose: ComposeId, id: MessageRowId) {
        self.h.send(Command::StartCompose {
            compose,
            from: StartFrom::Edit { id },
            draft_id: None,
            mailto: None,
        });
    }

    fn ready(&self, compose: ComposeId) -> DraftInput {
        self.h.wait(|e| match e {
            Event::ComposeReady {
                compose: c, draft, ..
            } if *c == compose => Some(draft.clone()),
            _ => None,
        })
    }

    fn refused(&self, compose: ComposeId) -> String {
        self.h.wait(|e| match e {
            Event::ComposeRefused { compose: c, text } if *c == compose => Some(text.clone()),
            _ => None,
        })
    }

    fn open(&self, id: MessageRowId) -> MessageView {
        self.h.send(Command::OpenMessage {
            id,
            images: ImagesChoice::Default,
            width_px: 300,
        });
        self.h.wait(|e| match e {
            Event::MessageOpened { id: i, view } if *i == id => Some(view.clone()),
            _ => None,
        })
    }
}

#[test]
fn edit_opens_the_local_file_found_by_message_id() {
    let s = seeded();
    s.local(MID, "local body");
    s.edit(ComposeId(1), s.in_drafts);
    let draft = s.ready(ComposeId(1));
    assert_eq!(draft.body, "local body");
    assert_eq!(draft.subject, "local text");
}

#[test]
fn edit_of_a_message_outside_drafts_is_refused() {
    let s = seeded();
    s.local(MID, "local body");
    s.edit(ComposeId(1), s.in_inbox);
    assert!(
        s.refused(ComposeId(1))
            .starts_with("This message is not available")
    );
    // Nothing was opened: the local draft is free to open from Drafts.
    s.edit(ComposeId(2), s.in_drafts);
    assert_eq!(s.ready(ComposeId(2)).body, "local body");
}

#[test]
fn a_second_edit_of_an_open_draft_is_refused() {
    let s = seeded();
    s.local(MID, "local body");
    s.edit(ComposeId(1), s.in_drafts);
    s.ready(ComposeId(1));
    s.edit(ComposeId(2), s.in_drafts);
    assert_eq!(s.refused(ComposeId(2)), "This draft is already open.");
}

#[test]
fn edit_never_takes_a_draft_of_another_account() {
    let s = seeded();
    let other = Draft {
        account: "someone-else".into(),
        updated_at_ms: 5,
        ..Draft::default()
    };
    let id = s.drafts.create(&other).unwrap();
    s.drafts.mark_uploaded(&id, MID, 5).unwrap();
    s.edit(ComposeId(1), s.in_drafts);
    // No match: refused until T2 imports from the server message.
    assert!(!s.refused(ComposeId(1)).is_empty());
}

#[test]
fn the_reader_says_when_the_message_is_a_draft() {
    let s = seeded();
    assert!(s.open(s.in_drafts).is_draft);
    assert!(!s.open(s.in_inbox).is_draft);
}
