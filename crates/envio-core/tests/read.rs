//! Reading from the cache: folders, pages, search, opening, attachments.

mod support;

use envio_core::api::view::{MessageRow, MessageView};
use envio_core::{
    Command, Event, FolderId, FolderRole, ImagesChoice, ListRef, MessageRowId, PAGE, SearchScope,
};
use envio_store::{BodyState, Db};
use support::Harness;
use support::fake::{Kind, serve};

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
            list: ListRef::Folder(self.folder),
            after: None,
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

    fn open(&self, id: MessageRowId, images: ImagesChoice) -> MessageView {
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
    assert_eq!(folders[0].unread, 3);

    let rows = s.rows();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].folder, s.folder);
    assert_eq!(rows[0].account, s.key);
    // The seeded rows' headers named no attachment.
    assert!(!rows[0].has_attachments);
    assert!(rows[0].subject.starts_with("message "));
    assert_eq!(rows[0].from, "Ada");
    assert_eq!(rows[0].from_address, "ada@example.org");
}

#[test]
fn open_a_downloaded_message_renders_sanitized_html() {
    let s = seeded(&[(1, HTML)]);
    let view = s.open(s.id_of("message 1"), ImagesChoice::Default);
    assert!(view.html.contains("Hello"));
    assert!(!view.html.contains("script"));
    assert!(!view.html.contains("alert"));
    assert!(!view.raw);
}

#[test]
fn a_broken_message_opens_as_raw_text() {
    let s = seeded(&[(1, BROKEN)]);
    let view = s.open(s.id_of("message 1"), ImagesChoice::Default);
    assert!(view.raw);
    // Shown as text, not as markup.
    assert!(view.html.contains("&lt;b&gt;not a body"));
}

#[test]
fn remote_images_stay_blocked_until_asked() {
    let s = seeded(&[(1, REMOTE_IMAGE)]);
    let id = s.id_of("message 1");
    let blocked = s.open(id, ImagesChoice::Default);
    assert_eq!(blocked.blocked_images, 1);
    assert!(!blocked.html.contains("images.example.net"));
    let shown = s.open(id, ImagesChoice::Load);
    assert_eq!(shown.blocked_images, 0);
    assert!(shown.html.contains("images.example.net/a.png"));
}

const INLINE_IMAGE: &[u8] =
    b"From: Ada <ada@example.org>\r\nSubject: inline\r\nMIME-Version: 1.0\r\n\
Content-Type: multipart/related; boundary=XX\r\n\r\n\
--XX\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
<p>Logo</p><img src=\"cid:logo@example.org\" alt=\"Logo\">\r\n\
--XX\r\nContent-Type: image/png\r\nContent-ID: <logo@example.org>\r\n\
Content-Transfer-Encoding: base64\r\n\r\naGVsbG8=\r\n--XX--\r\n";

#[test]
fn inline_images_show_even_while_remote_images_are_blocked() {
    let s = seeded(&[(1, INLINE_IMAGE)]);
    let view = s.open(s.id_of("message 1"), ImagesChoice::Default);
    assert_eq!(view.blocked_images, 0);
    let start = view.html.find("src=\"file://").expect(&view.html);
    let url = &view.html[start + 5..];
    let path = &url[7..url.find('"').unwrap()];
    assert!(
        path.starts_with(s.h.cache_dir().join("inline").to_str().unwrap()),
        "{path}"
    );
    assert_eq!(std::fs::read(path).unwrap(), b"hello");
}

#[test]
fn search_finds_by_subject() {
    let s = seeded(&[(7, HTML), (8, HTML)]);
    s.h.send(Command::Search {
        scope: SearchScope::Account(s.key.clone()),
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
    let view = s.open(id, ImagesChoice::Default);
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
fn save_attachment_creates_a_missing_folder() {
    let s = seeded(&[(1, WITH_ATTACHMENT)]);
    let id = s.id_of("message 1");
    let out = tempfile::tempdir().unwrap();
    let dir = out.path().join("cache").join("attachments");
    s.h.send(Command::SaveAttachment {
        id,
        index: 0,
        dir: dir.clone(),
    });
    let path = s.h.wait(|e| match e {
        Event::AttachmentSaved { path, .. } => Some(path.clone()),
        _ => None,
    });
    assert_eq!(path, dir.join("evil.txt"));
}

#[test]
fn a_failed_save_says_which_attachment_failed() {
    let s = seeded(&[(1, WITH_ATTACHMENT)]);
    let id = s.id_of("message 1");
    let out = tempfile::tempdir().unwrap();
    // A file where the folder should be: the save cannot start.
    let blocked = out.path().join("file");
    std::fs::write(&blocked, b"x").unwrap();
    s.h.send(Command::SaveAttachment {
        id,
        index: 0,
        dir: blocked.join("sub"),
    });
    let failed = s.h.wait(|e| match e {
        Event::AttachmentFailed { id, index } => Some((*id, *index)),
        _ => None,
    });
    assert_eq!(failed, (id, 0));
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
        images: ImagesChoice::Default,
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
        images: ImagesChoice::Default,
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

const WITH_CC: &[u8] = b"From: Ada <ada@example.org>\r\nTo: Me <me@example.org>\r\n\
Cc: Bob <bob@example.org>, carol@example.org\r\nSubject: cc\r\n\
Date: Thu, 14 Jan 2027 10:00:00 +0000\r\n\r\nHello\r\n";

fn first_page(
    h: &Harness,
    list: ListRef,
    after: Option<envio_core::Cursor>,
) -> (Vec<MessageRow>, bool) {
    h.send(Command::ListMessages { list, after });
    h.wait(|e| match e {
        Event::Messages {
            list: l,
            after: a,
            rows,
            end,
        } if *l == list && *a == after => Some((rows.clone(), *end)),
        _ => None,
    })
}

#[test]
fn the_unified_inbox_lists_every_account_inbox() {
    let mut h = Harness::unstarted();
    let one = h.seed_account("one@example.org", Some("pw"));
    let two = h.seed_account("two@example.org", Some("pw"));
    h.seed_folder_with(&one, "INBOX", FolderRole::Inbox, &[(1, HTML), (2, HTML)]);
    h.seed_folder_with(&two, "INBOX", FolderRole::Inbox, &[(1, HTML)]);
    h.seed_folder_with(&two, "Archive", FolderRole::Archive, &[(1, HTML)]);
    h.start_core();
    let (rows, end) = first_page(&h, ListRef::Inboxes, None);
    assert!(end);
    assert_eq!(rows.len(), 3);
    let count = |key: &str| rows.iter().filter(|r| r.account == key).count();
    assert_eq!((count(&one), count(&two)), (2, 1));
}

#[test]
fn the_next_page_starts_after_the_cursor() {
    let n = PAGE as usize + 10;
    let messages: Vec<(u32, &[u8])> = (1..=n as u32).map(|uid| (uid, HTML)).collect();
    let s = seeded(&messages);
    let list = ListRef::Folder(s.folder);
    let (first, end) = first_page(&s.h, list, None);
    assert_eq!(first.len(), PAGE as usize);
    assert!(!end);
    let cursor = first.last().unwrap().cursor;
    let (second, end) = first_page(&s.h, list, Some(cursor));
    assert_eq!(second.len(), 10);
    assert!(end);
    let mut ids: Vec<_> = first.iter().chain(&second).map(|r| r.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), n, "no row twice, none missing");
}

#[test]
fn the_last_page_says_end() {
    let full: Vec<(u32, &[u8])> = (1..=PAGE).map(|uid| (uid, HTML)).collect();
    let s = seeded(&full);
    let list = ListRef::Folder(s.folder);
    // Exactly a page: the app asks once more and gets nothing, with `end`.
    let (rows, end) = first_page(&s.h, list, None);
    assert_eq!(rows.len(), PAGE as usize);
    assert!(!end);
    let (rows, end) = first_page(&s.h, list, Some(rows.last().unwrap().cursor));
    assert!(rows.is_empty());
    assert!(end);
}

#[test]
fn folder_rows_carry_unread_counts() {
    let mut h = Harness::unstarted();
    let key = h.seed_account(ME, Some("pw"));
    let inbox = h.seed_folder_with(
        &key,
        "INBOX",
        FolderRole::Inbox,
        &[(1, HTML), (2, HTML), (3, HTML)],
    );
    h.seed_folder_with(&key, "Archive", FolderRole::Archive, &[(1, HTML)]);
    let db = Db::open(&h.paths().db()).unwrap();
    db.set_flags(inbox, 1, envio_core::Flags::SEEN).unwrap();
    drop(db);
    h.start_core();
    let unread = h.wait(|e| match e {
        Event::Started { accounts, .. } => Some(accounts[0].unread),
        _ => None,
    });
    assert_eq!(unread, 2);
    h.send(Command::ListFolders {
        account: key.clone(),
    });
    let folders = h.wait(|e| match e {
        Event::Folders { folders, .. } => Some(folders.clone()),
        _ => None,
    });
    let of = |name: &str| folders.iter().find(|f| f.name == name).unwrap().unread;
    assert_eq!((of("INBOX"), of("Archive")), (2, 1));
}

#[test]
fn a_view_has_cc_from_the_message() {
    let s = seeded(&[(1, WITH_CC)]);
    let view = s.open(s.id_of("message 1"), ImagesChoice::Default);
    let header = view.header;
    assert_eq!(header.from, "Ada");
    assert_eq!(header.from_address, "ada@example.org");
    assert_eq!(header.to, ["Me <me@example.org>"]);
    assert_eq!(header.cc, ["Bob <bob@example.org>", "carol@example.org"]);
    assert_eq!(header.subject, "cc");
    assert!(header.date.is_some());
    assert_eq!(header.account, s.key);
    assert!(!header.sender_known);
}

#[test]
fn images_load_by_default_for_an_allowed_sender() {
    let mut h = Harness::unstarted();
    let key = h.seed_account(ME, Some("pw"));
    h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[(1, REMOTE_IMAGE)]);
    std::fs::create_dir_all(h.data_dir()).unwrap();
    let settings = envio_core::Settings {
        images_from: ["ada@example.org".to_owned()].into(),
        ..envio_core::Settings::default()
    };
    envio_store::save_settings(&h.paths().settings(), &settings).unwrap();
    h.start_core();
    let s = Seeded {
        h,
        key,
        folder: FolderId(1),
    };
    let view = s.open(s.id_of("message 1"), ImagesChoice::Default);
    assert_eq!(view.blocked_images, 0);
    assert!(view.html.contains("images.example.net/a.png"));
    assert!(view.header.sender_known);
}

#[test]
fn opening_a_message_marks_it_read_without_an_undo_offer() {
    let s = seeded(&[(1, HTML), (2, HTML)]);
    let id = s.id_of("message 1");
    assert!(
        !s.rows()
            .iter()
            .find(|r| r.id == id)
            .unwrap()
            .flags
            .contains(envio_core::Flags::SEEN)
    );
    s.open(id, ImagesChoice::Default);
    // The list is told; an Undo toast for something nobody asked for is not.
    let changed = s.h.wait(|e| match e {
        Event::MessagesChanged { folders } => Some(folders.contains(&s.folder)),
        Event::UndoOffer { .. } => Some(false),
        _ => None,
    });
    assert!(changed, "MessagesChanged for the folder, no UndoOffer");
    let rows = s.rows();
    let seen = |subject: &str| {
        rows.iter()
            .find(|r| r.subject == subject)
            .unwrap()
            .flags
            .contains(envio_core::Flags::SEEN)
    };
    assert!(seen("message 1"));
    assert!(!seen("message 2"));
    // Opening it again changes nothing.
    s.open(id, ImagesChoice::Load);
    s.h.send(Command::ListMessages {
        list: ListRef::Folder(s.folder),
        after: None,
    });
    let again = s.h.wait(|e| match e {
        Event::MessagesChanged { .. } | Event::UndoOffer { .. } => Some(true),
        Event::Messages { .. } => Some(false),
        _ => None,
    });
    assert!(!again, "a second open sent no change");
}

#[test]
fn a_row_shows_the_attachment_mark_once_the_body_says_so() {
    let server = serve(Kind::Message);
    let mut h = Harness::unstarted();
    h.foreground = true;
    let key = h.seed_account_at(ME, Some("pw"), server.port);
    h.pin_imap(&key, &server.hex);
    // Cached from the header alone: no mark, and no body yet.
    h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[(1, HTML)]);
    let db = Db::open(&h.paths().db()).unwrap();
    let folder = db.folders(&key).unwrap()[0].id;
    let id = db.list(ListRef::Folder(folder), None, 5).unwrap()[0].id;
    db.set_body_state(id, BodyState::Missing).unwrap();
    drop(db);
    h.start_core();
    let rows = |h: &Harness| {
        h.send(Command::ListMessages {
            list: ListRef::Folder(folder),
            after: None,
        });
        h.wait(|e| match e {
            Event::Messages { rows, .. } => Some(rows.clone()),
            _ => None,
        })
    };
    assert!(!rows(&h)[0].has_attachments);
    h.send(Command::OpenMessage {
        id,
        images: ImagesChoice::Default,
        width_px: 300,
    });
    h.wait(|e| match e {
        Event::MessageOpened { id: i, .. } if *i == id => Some(()),
        _ => None,
    });
    // The body is stored before MessageOpened, so the list now has the mark.
    let rows = rows(&h);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].has_attachments);
}

#[test]
fn opening_a_cached_body_sets_the_attachment_mark_a_migration_left_at_zero() {
    let mut h = Harness::unstarted();
    let key = h.seed_account(ME, Some("pw"));
    let folder = h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[(1, WITH_ATTACHMENT)]);
    let db = Db::open(&h.paths().db()).unwrap();
    let id = db.list(ListRef::Folder(folder), None, 5).unwrap()[0].id;
    // The body is stored; the row is as migration 0004 left it.
    assert_eq!(db.message(id).unwrap().body, BodyState::Downloaded);
    db.set_has_attachments(id, false).unwrap();
    drop(db);
    h.start_core();
    let rows = |h: &Harness| {
        h.send(Command::ListMessages {
            list: ListRef::Folder(folder),
            after: None,
        });
        h.wait(|e| match e {
            Event::Messages { rows, .. } => Some(rows.clone()),
            _ => None,
        })
    };
    assert!(!rows(&h)[0].has_attachments);
    h.send(Command::OpenMessage {
        id,
        images: ImagesChoice::Default,
        width_px: 300,
    });
    h.wait(|e| match e {
        Event::MessagesChanged { folders } if folders.contains(&folder) => Some(()),
        _ => None,
    });
    assert!(rows(&h)[0].has_attachments);
}

#[test]
fn a_cached_body_that_does_not_parse_keeps_the_attachment_mark() {
    let mut h = Harness::unstarted();
    let key = h.seed_account(ME, Some("pw"));
    let folder = h.seed_folder_with(
        &key,
        "INBOX",
        FolderRole::Inbox,
        &[(1, b"no headers here\r\n")],
    );
    let db = Db::open(&h.paths().db()).unwrap();
    let id = db.list(ListRef::Folder(folder), None, 5).unwrap()[0].id;
    assert_eq!(db.message(id).unwrap().body, BodyState::Downloaded);
    db.set_has_attachments(id, true).unwrap();
    drop(db);
    h.start_core();
    h.send(Command::OpenMessage {
        id,
        images: ImagesChoice::Default,
        width_px: 300,
    });
    h.wait(|e| match e {
        Event::MessageOpened { .. } => Some(()),
        _ => None,
    });
    h.send(Command::ListMessages {
        list: ListRef::Folder(folder),
        after: None,
    });
    let rows = h.wait(|e| match e {
        Event::Messages { rows, .. } => Some(rows.clone()),
        _ => None,
    });
    assert!(rows[0].has_attachments);
}

#[test]
fn folders_are_listed_role_first() {
    let mut h = Harness::unstarted();
    let key = h.seed_account(ME, Some("pw"));
    for (path, role) in [
        ("Trash", FolderRole::Trash),
        ("Drafts", FolderRole::Drafts),
        ("Banking", FolderRole::Other),
        ("INBOX", FolderRole::Inbox),
        ("Sent", FolderRole::Sent),
    ] {
        h.seed_folder_with(&key, path, role, &[]);
    }
    h.start_core();
    h.send(Command::ListFolders {
        account: key.clone(),
    });
    let folders = h.wait(|e| match e {
        Event::Folders { account, folders } if *account == key => Some(folders.clone()),
        _ => None,
    });
    let paths: Vec<&str> = folders.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["INBOX", "Drafts", "Sent", "Trash", "Banking"]);
}

/// A cached message whose server size is `size` bytes (the cached body is
/// small: the size is what the cap looks at).
fn seeded_sized(size: u32) -> (Seeded, MessageRowId) {
    let mut h = Harness::unstarted();
    let key = h.seed_account(ME, Some("pw"));
    let folder = h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[(1, HTML)]);
    let conn = rusqlite::Connection::open(h.paths().db()).unwrap();
    conn.execute("UPDATE messages SET size = ?1", [size])
        .unwrap();
    drop(conn);
    h.start_core();
    let s = Seeded { h, key, folder };
    let id = s.id_of("message 1");
    (s, id)
}

#[test]
fn a_message_over_the_cap_is_not_opened_until_asked() {
    let (s, id) = seeded_sized(30 * 1000 * 1000);
    s.h.send(Command::OpenMessage {
        id,
        images: ImagesChoice::Default,
        width_px: 300,
    });
    let text = s.h.wait(|e| match e {
        Event::MessageTooLarge { id: i, text } if *i == id => Some(text.clone()),
        _ => None,
    });
    assert!(text.contains("30 MB"), "{text}");
    s.h.send(Command::OpenMessageAnyway {
        id,
        images: ImagesChoice::Default,
        width_px: 300,
    });
    let view = s.h.wait(|e| match e {
        Event::MessageOpened { id: i, view } if *i == id => Some(view.clone()),
        _ => None,
    });
    assert!(view.html.contains("Hello"));
}

#[test]
fn open_anyway_fetches_a_body_that_is_not_on_disk() {
    let server = serve(Kind::Message);
    let mut h = Harness::unstarted();
    h.foreground = true;
    let key = h.seed_account_at(ME, Some("pw"), server.port);
    h.pin_imap(&key, &server.hex);
    h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[(1, HTML)]);
    let db = Db::open(&h.paths().db()).unwrap();
    let folder = db.folders(&key).unwrap()[0].id;
    let id = db.list(ListRef::Folder(folder), None, 5).unwrap()[0].id;
    db.set_body_state(id, BodyState::Missing).unwrap();
    drop(db);
    let conn = rusqlite::Connection::open(h.paths().db()).unwrap();
    conn.execute("UPDATE messages SET size = ?1", [30 * 1000 * 1000_u32])
        .unwrap();
    drop(conn);
    h.start_core();
    h.send(Command::OpenMessage {
        id,
        images: ImagesChoice::Default,
        width_px: 300,
    });
    h.wait(|e| match e {
        Event::MessageTooLarge { id: i, .. } if *i == id => Some(()),
        _ => None,
    });
    h.send(Command::OpenMessageAnyway {
        id,
        images: ImagesChoice::Default,
        width_px: 300,
    });
    // The fetch completes and the message opens; no second cap notice.
    let opened = h.wait(|e| match e {
        Event::MessageTooLarge { id: i, .. } if *i == id => Some(false),
        Event::MessageOpened { id: i, .. } if *i == id => Some(true),
        _ => None,
    });
    assert!(opened, "MessageTooLarge came again");
    let db = Db::open(&h.paths().db()).unwrap();
    assert_eq!(db.message(id).unwrap().body, BodyState::Downloaded);
}

#[test]
fn a_message_at_the_cap_opens_normally() {
    let (s, id) = seeded_sized(20 * 1024 * 1024);
    let view = s.open(id, ImagesChoice::Default);
    assert!(view.html.contains("Hello"));
}
