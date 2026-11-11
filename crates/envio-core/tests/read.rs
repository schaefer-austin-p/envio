//! Reading from the cache: folders, pages, search, opening, attachments.

mod support;

use envio_core::api::view::{MessageRow, MessageView};
use envio_core::{Command, Event, FolderId, FolderRole, MessageRowId, Page};
use envio_store::{BodyState, Db};
use support::Harness;

const ME: &str = "me@example.org";

const HTML: &[u8] = b"From: Ada <ada@example.org>\r\nSubject: hi\r\nMIME-Version: 1.0\r\n\
Content-Type: text/html; charset=utf-8\r\n\r\n\
<p>Hello</p><script>alert(1)</script>";

const REMOTE_IMAGE: &[u8] = b"From: Ada <ada@example.org>\r\nSubject: pic\r\nMIME-Version: 1.0\r\n\
Content-Type: text/html; charset=utf-8\r\n\r\n\
<p>Look</p><img src=\"http://images.example.net/a.png\" alt=\"a\">";

// Declared multipart without a boundary: the parser shows it as raw text.
const BROKEN: &[u8] = b"From: Ada <ada@example.org>\r\nSubject: broken\r\n\
Content-Type: multipart/mixed\r\n\r\n<b>not a body</b>";

const WITH_ATTACHMENT: &[u8] =
    b"From: Ada <ada@example.org>\r\nSubject: file\r\nMIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=XX\r\n\r\n\
--XX\r\nContent-Type: text/plain\r\n\r\nsee attached\r\n\
--XX\r\nContent-Type: text/plain; name=\"x.txt\"\r\n\
Content-Disposition: attachment; filename=\"../../evil.txt\"\r\n\
Content-Transfer-Encoding: base64\r\n\r\naGVsbG8=\r\n--XX--\r\n";

struct Seeded {
    h: Harness,
    key: String,
    folder: FolderId,
}

fn seeded(messages: &[(u32, &[u8])]) -> Seeded {
    let mut h = Harness::unstarted();
    let key = h.seed_account(ME, Some("pw"));
    let folder = h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, messages);
    h.start_core();
    Seeded { h, key, folder }
}

impl Seeded {
    fn rows(&self) -> Vec<MessageRow> {
        self.h.send(Command::ListMessages {
            folder: self.folder,
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

    fn id_of(&self, subject: &str) -> MessageRowId {
        self.rows()
            .iter()
            .find(|r| r.subject == subject)
            .unwrap()
            .id
    }

    fn open(&self, id: MessageRowId, images: bool) -> MessageView {
        self.h.send(Command::OpenMessage {
            id,
            images,
            width_px: 300,
        });
        self.h.wait(|e| match e {
            Event::MessageOpened { id: i, view } if *i == id => Some(view.clone()),
            _ => None,
        })
    }
}

#[test]
fn list_folders_and_a_page_of_messages() {
    let s = seeded(&[(1, HTML), (2, HTML), (3, HTML)]);
    s.h.send(Command::ListFolders {
        account: s.key.clone(),
    });
    let folders = s.h.wait(|e| match e {
        Event::Folders { account, folders } if *account == s.key => Some(folders.clone()),
        _ => None,
    });
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].id, s.folder);
    assert_eq!(folders[0].name, "INBOX");
    assert_eq!(folders[0].role, FolderRole::Inbox);
    assert_eq!(folders[0].unread, 0);

    s.h.send(Command::ListMessages {
        folder: s.folder,
        page: Page {
            offset: 1,
            limit: 1,
        },
    });
    let (page, rows) = s.h.wait(|e| match e {
        Event::Messages { page, rows, .. } => Some((*page, rows.clone())),
        _ => None,
    });
    assert_eq!(page.offset, 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].folder, s.folder);
    assert!(rows[0].subject.starts_with("message "));
    assert_eq!(rows[0].from, "Ada <ada@example.org>");
}

#[test]
fn open_a_downloaded_message_renders_sanitized_html() {
    let s = seeded(&[(1, HTML)]);
    let view = s.open(s.id_of("message 1"), false);
    assert!(view.html.contains("Hello"));
    assert!(!view.html.contains("script"));
    assert!(!view.html.contains("alert"));
    assert!(!view.raw);
}

#[test]
fn a_broken_message_opens_as_raw_text() {
    let s = seeded(&[(1, BROKEN)]);
    let view = s.open(s.id_of("message 1"), false);
    assert!(view.raw);
    // Shown as text, not as markup.
    assert!(view.html.contains("&lt;b&gt;not a body"));
}

#[test]
fn remote_images_stay_blocked_until_asked() {
    let s = seeded(&[(1, REMOTE_IMAGE)]);
    let id = s.id_of("message 1");
    let blocked = s.open(id, false);
    assert_eq!(blocked.blocked_images, 1);
    assert!(!blocked.html.contains("images.example.net"));
    let shown = s.open(id, true);
    assert_eq!(shown.blocked_images, 0);
    assert!(shown.html.contains("images.example.net/a.png"));
}

#[test]
fn search_finds_by_subject() {
    let s = seeded(&[(7, HTML), (8, HTML)]);
    s.h.send(Command::Search {
        account: s.key.clone(),
        query: "message 7".into(),
    });
    let (query, rows) = s.h.wait(|e| match e {
        Event::SearchResults { query, rows, .. } => Some((query.clone(), rows.clone())),
        _ => None,
    });
    assert_eq!(query, "message 7");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].subject, "message 7");
}

#[test]
fn save_attachment_sanitizes_the_file_name() {
    let s = seeded(&[(1, WITH_ATTACHMENT)]);
    let id = s.id_of("message 1");
    let view = s.open(id, false);
    assert_eq!(view.attachments.len(), 1);
    assert_eq!(view.attachments[0].name, "evil.txt");
    assert_eq!(view.attachments[0].size, 5);

    let out = tempfile::tempdir().unwrap();
    let mut saved = Vec::new();
    for _ in 0..2 {
        s.h.send(Command::SaveAttachment {
            id,
            index: 0,
            dir: out.path().to_owned(),
        });
        saved.push(s.h.wait(|e| match e {
            Event::AttachmentSaved { path, .. } => Some(path.clone()),
            _ => None,
        }));
    }
    assert_eq!(saved[0], out.path().join("evil.txt"));
    assert_eq!(saved[1], out.path().join("evil (2).txt"));
    assert_eq!(std::fs::read(&saved[0]).unwrap(), b"hello");
    assert_eq!(std::fs::read(&saved[1]).unwrap(), b"hello");
}

#[test]
fn opening_without_a_body_offline_is_unavailable() {
    let mut h = Harness::unstarted();
    h.foreground = true;
    let key = h.seed_account(ME, Some("pw"));
    h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[(1, HTML)]);
    let db = Db::open(&h.paths().db()).unwrap();
    let folder = db.folders(&key).unwrap()[0].id;
    let id = db
        .list(envio_store::ListRef::Folder(folder), None, 5)
        .unwrap()[0]
        .id;
    db.set_body_state(id, BodyState::Missing).unwrap();
    drop(db);
    h.start_core();
    h.send(Command::OpenMessage {
        id,
        images: false,
        width_px: 300,
    });
    let text = h.wait(|e| match e {
        Event::MessageUnavailable { id: i, text } if *i == id => Some(text.clone()),
        _ => None,
    });
    assert!(!text.is_empty());
}

#[test]
fn removing_an_account_answers_a_pending_open() {
    let mut h = Harness::unstarted();
    let key = h.seed_account(ME, Some("pw"));
    h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[(1, HTML)]);
    let db = Db::open(&h.paths().db()).unwrap();
    let folder = db.folders(&key).unwrap()[0].id;
    let id = db
        .list(envio_store::ListRef::Folder(folder), None, 5)
        .unwrap()[0]
        .id;
    db.set_body_state(id, BodyState::Missing).unwrap();
    drop(db);
    h.start_core();
    // Queued together: the removal is handled before the step that would
    // have failed the open.
    h.send(Command::OpenMessage {
        id,
        images: false,
        width_px: 300,
    });
    h.send(Command::RemoveAccount {
        account: key,
        confirmed: true,
    });
    h.wait(|e| match e {
        Event::MessageUnavailable { id: i, .. } if *i == id => Some(()),
        _ => None,
    });
}
