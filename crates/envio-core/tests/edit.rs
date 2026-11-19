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
const RAW: &[u8] = b"From: Test <me@example.org>\r\nTo: ada@example.org\r\n\
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
fn give_message_id(h: &Harness, folder: FolderId, uid: u32, message_id: Option<&str>) {
    let db = envio_store::Db::open(&h.paths().db()).unwrap();
    let message = NewMessage {
        uid,
        size: u32::try_from(RAW.len()).unwrap(),
        envelope: Envelope {
            subject: format!("message {uid}"),
            // Recent, or the start-up prune drops the body.
            date: support::now_ms() / 1000,
            message_id: message_id.map(Into::into),
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
    seeded_with(RAW, Some(MID))
}

/// Both folders hold `raw`, cached as the message with `message_id`.
fn seeded_with(raw: &[u8], message_id: Option<&str>) -> Seeded {
    let mut h = Harness::unstarted();
    let key = h.seed_account(ME, Some("pw"));
    let drafts_folder = h.seed_folder_with(&key, "Drafts", FolderRole::Drafts, &[(1, raw)]);
    let inbox = h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[(1, raw)]);
    give_message_id(&h, drafts_folder, 1, message_id);
    give_message_id(&h, inbox, 1, message_id);
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

    /// `ComposeReady`'s draft and note.
    fn ready_with_note(&self, compose: ComposeId) -> (DraftInput, String) {
        self.h.wait(|e| match e {
            Event::ComposeReady {
                compose: c,
                draft,
                note,
                ..
            } if *c == compose => Some((draft.clone(), note.clone())),
            _ => None,
        })
    }

    fn close(&self, compose: ComposeId) {
        self.h.send(Command::CloseCompose {
            compose,
            discard: false,
        });
    }

    /// The draft files, newest first.
    fn files(&self) -> Vec<(String, Draft)> {
        self.drafts.list().unwrap()
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
    // No match in this account: the server message is imported as a new
    // draft, and the other account's file is left alone.
    let opened = s.ready(ComposeId(1));
    assert_ne!(opened.draft_id.as_deref(), Some(id.as_str()));
    assert_eq!(opened.account, s.key);
    assert_eq!(s.drafts.load(&id).unwrap().account, "someone-else");
}

#[test]
fn the_reader_says_when_the_message_is_a_draft() {
    let s = seeded();
    assert!(s.open(s.in_drafts).is_draft);
    assert!(!s.open(s.in_inbox).is_draft);
}

const HTML: &[u8] = b"From: Test <me@example.org>\r\nTo: ada@example.org\r\n\
Subject: html text\r\nMessage-ID: <d1@example.org>\r\n\
MIME-Version: 1.0\r\nContent-Type: text/html\r\n\r\n<p>html <b>body</b></p>\r\n";

const WITH_FILE: &[u8] = b"From: Test <me@example.org>\r\nTo: ada@example.org\r\n\
Subject: with file\r\nMessage-ID: <d1@example.org>\r\n\
MIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=B\r\n\r\n\
--B\r\nContent-Type: text/plain\r\n\r\nsee file\r\n\
--B\r\nContent-Type: application/pdf; name=\"a.pdf\"\r\n\
Content-Disposition: attachment; filename=\"a.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\r\nSGVsbG8=\r\n--B--\r\n";

const NO_ID: &[u8] = b"From: Test <me@example.org>\r\nTo: ada@example.org\r\n\
Subject: no id\r\n\r\nbody without id\r\n";

#[test]
fn edit_of_a_server_only_draft_imports_it_linked_to_the_server_copy() {
    let s = seeded();
    s.edit(ComposeId(1), s.in_drafts);
    let (draft, note) = s.ready_with_note(ComposeId(1));
    assert_eq!(draft.body.trim_end(), "server body");
    assert_eq!(draft.subject, "server text");
    assert_eq!(draft.to, vec!["ada@example.org".to_owned()]);
    assert_eq!(note, "", "a linked import needs no note");
    let files = s.files();
    assert_eq!(files.len(), 1);
    let (id, file) = &files[0];
    assert_eq!(draft.draft_id.as_deref(), Some(id.as_str()));
    assert_eq!(file.account, s.key);
    // Current: unchanged, it is not uploaded; the first edit replaces MID.
    assert_eq!(file.server.message_id.as_deref(), Some(MID));
    assert_eq!(file.server.uploaded_ms, file.updated_at_ms);
    assert_eq!(file.server.imported_from.as_deref(), Some(MID));
}

#[test]
fn a_second_edit_after_closing_reopens_the_import_instead_of_importing_again() {
    let s = seeded();
    s.edit(ComposeId(1), s.in_drafts);
    let first = s.ready(ComposeId(1));
    s.close(ComposeId(1));
    s.edit(ComposeId(2), s.in_drafts);
    let second = s.ready(ComposeId(2));
    assert_eq!(second.draft_id, first.draft_id);
    assert_eq!(s.files().len(), 1);
}

#[test]
fn edit_of_an_html_draft_makes_a_detached_copy_and_says_so() {
    let s = seeded_with(HTML, Some(MID));
    s.edit(ComposeId(1), s.in_drafts);
    let (draft, note) = s.ready_with_note(ComposeId(1));
    assert_eq!(note, "Editing a copy. The original stays in Drafts.");
    assert!(draft.body.contains("html"), "{}", draft.body);
    assert!(!draft.body.contains('<'), "{}", draft.body);
    let files = s.files();
    assert_eq!(files.len(), 1);
    let server = &files[0].1.server;
    // Never replaces the original: no id to replace, so the first save
    // uploads a new draft.
    assert_eq!(server.message_id, None);
    assert_eq!(server.imported_from.as_deref(), Some(MID));
    assert!(envio_sync::plan::drafts::needs_upload(&files[0].1, 0));
}

#[test]
fn a_reuploaded_copy_is_reopened_from_the_stale_cached_original() {
    for raw in [HTML, RAW] {
        let s = seeded_with(raw, Some(MID));
        s.edit(ComposeId(1), s.in_drafts);
        let first = s.ready(ComposeId(1));
        let copy = first.draft_id.clone().unwrap();
        s.close(ComposeId(1));
        // The copy was edited and uploaded as <c2@..>; the cache still has
        // the original row only.
        s.drafts.mark_uploading(&copy, "<c2@example.org>").unwrap();
        s.drafts
            .mark_uploaded(&copy, "<c2@example.org>", 99)
            .unwrap();
        s.edit(ComposeId(2), s.in_drafts);
        let again = s.ready(ComposeId(2));
        assert_eq!(again.draft_id.as_deref(), Some(copy.as_str()));
        assert_eq!(s.files().len(), 1, "no duplicate import");
    }
}

#[test]
fn two_edits_of_a_draft_without_a_message_id_make_two_copies_and_lose_nothing() {
    let s = seeded_with(NO_ID, None);
    s.edit(ComposeId(1), s.in_drafts);
    let first = s.ready(ComposeId(1));
    s.close(ComposeId(1));
    s.edit(ComposeId(2), s.in_drafts);
    let second = s.ready(ComposeId(2));
    assert_ne!(first.draft_id, second.draft_id);
    let files = s.files();
    assert_eq!(files.len(), 2);
    for (_, file) in &files {
        let copy = envio_store::ServerCopy {
            detached: true,
            ..envio_store::ServerCopy::default()
        };
        assert_eq!(file.server, copy);
        assert_eq!(file.body.trim_end(), "body without id");
    }
}

#[test]
fn an_import_keeps_the_originals_attachments() {
    let s = seeded_with(WITH_FILE, Some(MID));
    s.edit(ComposeId(1), s.in_drafts);
    let draft = s.ready(ComposeId(1));
    assert_eq!(draft.attachments.len(), 1);
    assert_eq!(std::fs::read(&draft.attachments[0]).unwrap(), b"Hello");
    let file = &s.files()[0].1;
    assert_eq!(file.attachments, draft.attachments);
    // Still linked and current after the second save of the paths.
    assert_eq!(file.server.message_id.as_deref(), Some(MID));
    assert_eq!(file.server.uploaded_ms, file.updated_at_ms);
}

#[test]
fn edit_leaves_a_damaged_file_that_names_the_message_alone() {
    let s = seeded();
    let id = s.local(MID, "unsent words");
    let path = s.h.paths().drafts().join(format!("{id}.json"));
    let text = std::fs::read_to_string(&path).unwrap();
    // The body has the wrong type: the file no longer parses as a draft,
    // but it still names the server copy.
    let hurt = text.replacen("\"body\": \"unsent words\"", "\"body\": 5", 1);
    assert_ne!(hurt, text);
    std::fs::write(&path, &hurt).unwrap();
    s.edit(ComposeId(1), s.in_drafts);
    assert!(!s.refused(ComposeId(1)).is_empty());
    assert!(s.files().is_empty(), "no import beside the damaged file");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), hurt);
}

const COPY_NOTE: &str = "Editing a copy. The original stays in Drafts.";

#[test]
fn a_detached_copy_says_so_every_time_it_is_opened() {
    let s = seeded_with(HTML, Some(MID));
    s.edit(ComposeId(1), s.in_drafts);
    let (first, note) = s.ready_with_note(ComposeId(1));
    assert_eq!(note, COPY_NOTE);
    s.close(ComposeId(1));
    s.edit(ComposeId(2), s.in_drafts);
    let (second, note) = s.ready_with_note(ComposeId(2));
    assert_eq!(
        second.draft_id, first.draft_id,
        "reopened, not imported again"
    );
    assert_eq!(note, COPY_NOTE, "the second open says it too");
    s.close(ComposeId(2));
    // Even after the copy was uploaded as a new draft.
    let copy = first.draft_id.unwrap();
    s.drafts
        .mark_uploaded(&copy, "<c2@example.org>", 99)
        .unwrap();
    s.edit(ComposeId(3), s.in_drafts);
    assert_eq!(s.ready_with_note(ComposeId(3)).1, COPY_NOTE);
    assert_eq!(s.files().len(), 1);
}

#[test]
fn a_linked_import_stays_without_a_note_when_reopened() {
    let s = seeded();
    s.edit(ComposeId(1), s.in_drafts);
    s.ready(ComposeId(1));
    s.close(ComposeId(1));
    s.edit(ComposeId(2), s.in_drafts);
    assert_eq!(s.ready_with_note(ComposeId(2)).1, "");
}

#[test]
fn a_bcc_of_the_server_draft_comes_into_the_compose_and_the_original_stays() {
    let raw = b"From: Test <me@example.org>\r\nTo: ada@example.org\r\n\
Bcc: Hid <hidden@example.org>, h2@example.org\r\n\
Subject: secret\r\nMessage-ID: <d1@example.org>\r\n\r\nbody\r\n";
    let s = seeded_with(raw, Some(MID));
    s.edit(ComposeId(1), s.in_drafts);
    let (draft, note) = s.ready_with_note(ComposeId(1));
    assert_eq!(
        draft.bcc,
        vec!["Hid <hidden@example.org>", "h2@example.org"]
    );
    assert_eq!(note, COPY_NOTE);
    let file = &s.files()[0].1;
    assert_eq!(file.bcc, draft.bcc, "kept in the file too");
    assert_eq!(file.server.message_id, None, "never replaces the original");
}

#[test]
fn a_draft_with_headers_envio_does_not_write_is_a_copy_not_a_replacement() {
    let raw = b"From: Test <me@example.org>\r\nTo: ada@example.org\r\n\
Reply-To: other@example.org\r\nX-Priority: 1\r\n\
Subject: urgent\r\nMessage-ID: <d1@example.org>\r\n\r\nbody\r\n";
    let s = seeded_with(raw, Some(MID));
    s.edit(ComposeId(1), s.in_drafts);
    assert_eq!(s.ready_with_note(ComposeId(1)).1, COPY_NOTE);
    assert_eq!(s.files()[0].1.server.message_id, None);
}

#[test]
fn a_local_draft_uploaded_twice_is_found_by_its_first_message_id() {
    let s = seeded();
    // Composed in Envio: uploaded as MID, then twice more.
    let id = s.local(MID, "unsent words");
    s.drafts.mark_uploaded(&id, "<c2@example.org>", 6).unwrap();
    s.drafts.mark_uploaded(&id, "<c3@example.org>", 7).unwrap();
    s.edit(ComposeId(1), s.in_drafts);
    assert_eq!(s.ready(ComposeId(1)).draft_id.as_deref(), Some(id.as_str()));
    assert_eq!(s.files().len(), 1);
}

/// A server draft `extra` is added to, with Envio's own From.
fn with_header(extra: &str) -> Vec<u8> {
    format!(
        "From: Test <me@example.org>\r\nTo: ada@example.org\r\n{extra}\
Subject: s\r\nMessage-ID: <d1@example.org>\r\n\r\nbody\r\n"
    )
    .into_bytes()
}

#[test]
fn a_draft_the_rebuild_would_change_is_a_copy_and_the_original_stays() {
    for (why, extra) in [
        ("two To", "To: late@example.org\r\n"),
        ("two Cc", "Cc: a@example.org\r\nCc: late@example.org\r\n"),
        ("two From", "From: Test <other@example.org>\r\n"),
        (
            "two In-Reply-To ids",
            "In-Reply-To: <p@example.org> <q@example.org>\r\n",
        ),
    ] {
        let s = seeded_with(&with_header(extra), Some(MID));
        s.edit(ComposeId(1), s.in_drafts);
        assert_eq!(s.ready_with_note(ComposeId(1)).1, COPY_NOTE, "{why}");
        assert_eq!(s.files()[0].1.server.message_id, None, "{why}");
    }
}

#[test]
fn a_from_name_other_than_the_accounts_is_a_copy() {
    let raw = b"From: Someone Else <me@example.org>\r\nTo: ada@example.org\r\n\
Subject: s\r\nMessage-ID: <d1@example.org>\r\n\r\nbody\r\n";
    let s = seeded_with(raw, Some(MID));
    s.edit(ComposeId(1), s.in_drafts);
    assert_eq!(s.ready_with_note(ComposeId(1)).1, COPY_NOTE);
    assert_eq!(s.files()[0].1.server.message_id, None);
}

#[test]
fn a_copy_and_a_linked_import_stay_what_they_are_after_many_uploads() {
    let detached = seeded_with(HTML, Some(MID));
    detached.edit(ComposeId(1), detached.in_drafts);
    let id = detached.ready(ComposeId(1)).draft_id.unwrap();
    detached.close(ComposeId(1));
    let linked = seeded();
    linked.edit(ComposeId(1), linked.in_drafts);
    let own = linked.ready(ComposeId(1)).draft_id.unwrap();
    linked.close(ComposeId(1));
    for n in 0..12 {
        let id_n = format!("<u{n}@example.org>");
        detached.drafts.mark_uploaded(&id, &id_n, 50).unwrap();
        linked.drafts.mark_uploaded(&own, &id_n, 50).unwrap();
    }
    detached.edit(ComposeId(2), detached.in_drafts);
    assert_eq!(detached.ready_with_note(ComposeId(2)).1, COPY_NOTE);
    linked.edit(ComposeId(2), linked.in_drafts);
    assert_eq!(linked.ready_with_note(ComposeId(2)).1, "");
}

#[test]
fn an_incomplete_import_is_dropped_and_imported_again() {
    let s = seeded();
    // What a crash between creating the copy and its files leaves.
    let half = Draft {
        account: s.key.clone(),
        body: "half".into(),
        updated_at_ms: 5,
        server: envio_store::ServerCopy {
            imported_from: Some(MID.into()),
            detached: true,
            incomplete: true,
            ..envio_store::ServerCopy::default()
        },
        ..Draft::default()
    };
    let stale = s.drafts.create_imported(&half).unwrap();
    s.edit(ComposeId(1), s.in_drafts);
    let (draft, note) = s.ready_with_note(ComposeId(1));
    assert_ne!(draft.draft_id.as_deref(), Some(stale.as_str()));
    assert_eq!(note, "", "a fresh, linked import");
    assert!(draft.body.contains("server body"), "{}", draft.body);
    let files = s.files();
    assert_eq!(files.len(), 1, "the half-made one is gone");
    assert!(!files[0].1.server.incomplete);
    assert!(
        s.drafts.deletes().unwrap().is_empty(),
        "no note for the original"
    );
}
