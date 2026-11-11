use envio_store::{
    Cursor, Db, Envelope, Flags, FolderId, FolderRole, ListRef, MessageRowId, NewMessage,
};

fn message(uid: u32, date: i64, flags: Flags) -> NewMessage {
    NewMessage {
        uid,
        size: 1_000,
        envelope: Envelope {
            message_id: Some(format!("<{uid}@example.org>")),
            subject: format!("m{uid}"),
            from: "Ada <ada@example.org>".into(),
            to: "bob@example.org".into(),
            date,
            in_reply_to: None,
            references: Vec::new(),
        },
        flags,
        preview: String::new(),
    }
}

fn setup(dates: &[(u32, i64)]) -> (Db, FolderId) {
    let db = Db::open_in_memory().unwrap();
    let inbox = db.upsert_folder("a1", "INBOX", FolderRole::Inbox).unwrap();
    for &(uid, date) in dates {
        db.upsert_message(inbox, &message(uid, date, Flags::empty()))
            .unwrap();
    }
    (db, inbox)
}

fn uids(db: &Db, list: ListRef, after: Option<Cursor>, limit: u32) -> Vec<u32> {
    db.list(list, after, limit)
        .unwrap()
        .iter()
        .map(|m| m.uid)
        .collect()
}

fn next(db: &Db, list: ListRef, after: Option<Cursor>, limit: u32) -> (Vec<u32>, Option<Cursor>) {
    let rows = db.list(list, after, limit).unwrap();
    let cursor = rows.last().map(Db::cursor_of);
    (rows.iter().map(|m| m.uid).collect(), cursor)
}

#[test]
fn pages_follow_date_then_id_newest_first() {
    let (db, inbox) = setup(&[(1, 300), (2, 100), (3, 200), (4, 400), (5, 50)]);
    let list = ListRef::Folder(inbox);
    let (first, cursor) = next(&db, list, None, 2);
    assert_eq!(first, [4, 1]);
    let (second, cursor) = next(&db, list, cursor, 2);
    assert_eq!(second, [3, 2]);
    let (third, cursor) = next(&db, list, cursor, 2);
    assert_eq!(third, [5]);
    assert!(uids(&db, list, cursor, 2).is_empty());
}

#[test]
fn new_mail_between_pages_does_not_repeat_a_row() {
    let (db, inbox) = setup(&[(1, 100), (2, 200), (3, 300), (4, 400)]);
    let list = ListRef::Folder(inbox);
    let (first, cursor) = next(&db, list, None, 2);
    assert_eq!(first, [4, 3]);
    db.upsert_message(inbox, &message(5, 500, Flags::empty()))
        .unwrap();
    db.upsert_message(inbox, &message(6, 450, Flags::empty()))
        .unwrap();
    assert_eq!(uids(&db, list, cursor, 2), [2, 1]);
}

#[test]
fn a_deleted_row_does_not_skip_the_next() {
    let (db, inbox) = setup(&[(1, 100), (2, 200), (3, 300), (4, 400)]);
    let list = ListRef::Folder(inbox);
    let (first, cursor) = next(&db, list, None, 2);
    assert_eq!(first, [4, 3]);
    db.delete_messages(inbox, &[4, 3]).unwrap();
    assert_eq!(uids(&db, list, cursor, 2), [2, 1]);
}

#[test]
fn equal_dates_page_by_id() {
    let (db, inbox) = setup(&[(1, 100), (2, 100), (3, 100), (4, 100)]);
    let list = ListRef::Folder(inbox);
    let (first, cursor) = next(&db, list, None, 2);
    assert_eq!(first, [4, 3]);
    let (second, cursor) = next(&db, list, cursor, 2);
    assert_eq!(second, [2, 1]);
    assert!(uids(&db, list, cursor, 2).is_empty());
    // A cursor whose row is gone still continues after its place.
    let gone = Cursor {
        date: 100,
        id: MessageRowId(3),
    };
    assert_eq!(uids(&db, list, Some(gone), 10), [2, 1]);
}

#[test]
fn the_unified_inbox_merges_every_account_inbox_only() {
    let db = Db::open_in_memory().unwrap();
    let a1 = db.upsert_folder("a1", "INBOX", FolderRole::Inbox).unwrap();
    let a2 = db.upsert_folder("a2", "INBOX", FolderRole::Inbox).unwrap();
    let sent = db.upsert_folder("a1", "Sent", FolderRole::Sent).unwrap();
    db.upsert_message(a1, &message(1, 100, Flags::empty()))
        .unwrap();
    db.upsert_message(a2, &message(1, 300, Flags::empty()))
        .unwrap();
    db.upsert_message(a1, &message(2, 200, Flags::empty()))
        .unwrap();
    db.upsert_message(sent, &message(9, 900, Flags::empty()))
        .unwrap();
    let rows = db.list(ListRef::Inboxes, None, 10).unwrap();
    let got: Vec<(FolderId, u32)> = rows.iter().map(|m| (m.folder, m.uid)).collect();
    assert_eq!(got, [(a2, 1), (a1, 2), (a1, 1)]);
    let cursor = Db::cursor_of(&rows[0]);
    assert_eq!(uids(&db, ListRef::Inboxes, Some(cursor), 10), [2, 1]);
}

#[test]
fn unread_counts_per_folder_ignore_seen() {
    let db = Db::open_in_memory().unwrap();
    let inbox = db.upsert_folder("a1", "INBOX", FolderRole::Inbox).unwrap();
    let sent = db.upsert_folder("a1", "Sent", FolderRole::Sent).unwrap();
    let other = db.upsert_folder("a2", "INBOX", FolderRole::Inbox).unwrap();
    db.upsert_message(inbox, &message(1, 1, Flags::empty()))
        .unwrap();
    db.upsert_message(inbox, &message(2, 2, Flags::FLAGGED))
        .unwrap();
    db.upsert_message(inbox, &message(3, 3, Flags::SEEN | Flags::FLAGGED))
        .unwrap();
    db.upsert_message(sent, &message(1, 4, Flags::SEEN))
        .unwrap();
    db.upsert_message(other, &message(1, 5, Flags::empty()))
        .unwrap();
    let counts = db.unread_counts("a1").unwrap();
    assert_eq!(counts.get(&inbox), Some(&2));
    assert_eq!(counts.get(&sent), None);
    assert_eq!(counts.get(&other), None);
}
