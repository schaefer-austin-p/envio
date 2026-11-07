use envio_store::{
    BodyState, Db, Envelope, Flags, FolderId, FolderRole, NewMessage, Page, StoreError,
};

fn message(uid: u32, subject: &str, date: i64) -> NewMessage {
    NewMessage {
        uid,
        envelope: Envelope {
            message_id: Some(format!("<{uid}@example.org>")),
            subject: subject.into(),
            from: "Ada <ada@example.org>".into(),
            to: "bob@example.org".into(),
            date,
            in_reply_to: None,
            references: Vec::new(),
        },
        flags: Flags::empty(),
        preview: format!("preview of {subject}"),
    }
}

fn inbox(db: &Db, account: &str) -> FolderId {
    db.upsert_folder(account, "INBOX", FolderRole::Inbox)
        .unwrap()
}

const ALL: Page = Page {
    offset: 0,
    limit: 100,
};

#[test]
fn message_row_ids_are_never_reused() {
    let db = Db::open_in_memory().unwrap();
    let folder = inbox(&db, "a1");
    let first = db.upsert_message(folder, &message(1, "m", 1)).unwrap();
    db.reset_folder(folder).unwrap();
    let second = db.upsert_message(folder, &message(1, "m", 1)).unwrap();
    assert_ne!(first, second);
}

#[test]
fn a_message_round_trips() {
    let db = Db::open_in_memory().unwrap();
    let folder = inbox(&db, "a1");
    let mut new = message(5, "Hello", 1_700_000_000);
    new.envelope.in_reply_to = Some("<1@example.org>".into());
    new.envelope.references = vec!["<0@example.org>".into(), "<1@example.org>".into()];
    new.flags = Flags::SEEN | Flags::FLAGGED;
    let id = db.upsert_message(folder, &new).unwrap();
    let stored = db.message(id).unwrap();
    assert_eq!(stored.folder, folder);
    assert_eq!(stored.uid, 5);
    assert_eq!(stored.envelope, new.envelope);
    assert_eq!(stored.flags, new.flags);
    assert_eq!(stored.preview, new.preview);
    assert_eq!(stored.body, BodyState::Missing);
}

#[test]
fn upsert_updates_by_uid_and_keeps_the_body_state() {
    let db = Db::open_in_memory().unwrap();
    let folder = inbox(&db, "a1");
    let id = db.upsert_message(folder, &message(5, "Old", 1)).unwrap();
    db.set_body_state(id, BodyState::Downloaded).unwrap();
    let again = db.upsert_message(folder, &message(5, "New", 2)).unwrap();
    assert_eq!(id, again);
    let stored = db.message(id).unwrap();
    assert_eq!(stored.envelope.subject, "New");
    assert_eq!(stored.body, BodyState::Downloaded);
    assert_eq!(db.messages(folder, ALL).unwrap().len(), 1);
}

#[test]
fn set_flags_replaces_the_flags() {
    let db = Db::open_in_memory().unwrap();
    let folder = inbox(&db, "a1");
    let id = db.upsert_message(folder, &message(5, "Hi", 1)).unwrap();
    db.set_flags(folder, 5, Flags::ANSWERED).unwrap();
    assert_eq!(db.message(id).unwrap().flags, Flags::ANSWERED);
    assert!(matches!(
        db.set_flags(folder, 6, Flags::SEEN),
        Err(StoreError::NotFound)
    ));
}

#[test]
fn messages_are_newest_first_and_paged() {
    let db = Db::open_in_memory().unwrap();
    let folder = inbox(&db, "a1");
    for (uid, date) in [(1, 300), (2, 100), (3, 200)] {
        db.upsert_message(folder, &message(uid, "m", date)).unwrap();
    }
    let uids = |page| -> Vec<u32> {
        db.messages(folder, page)
            .unwrap()
            .iter()
            .map(|m| m.uid)
            .collect()
    };
    assert_eq!(uids(ALL), [1, 3, 2]);
    assert_eq!(
        uids(Page {
            offset: 1,
            limit: 1
        }),
        [3]
    );
    assert!(
        uids(Page {
            offset: 3,
            limit: 10
        })
        .is_empty()
    );
}

#[test]
fn find_by_message_id_searches_one_account() {
    let db = Db::open_in_memory().unwrap();
    let inbox_a1 = inbox(&db, "a1");
    let sent_a1 = db.upsert_folder("a1", "Sent", FolderRole::Sent).unwrap();
    let inbox_a2 = inbox(&db, "a2");
    db.upsert_message(inbox_a1, &message(1, "x", 1)).unwrap();
    db.upsert_message(sent_a1, &message(1, "x", 1)).unwrap();
    db.upsert_message(inbox_a2, &message(1, "x", 1)).unwrap();
    let found = db.find_by_message_id("a1", "<1@example.org>").unwrap();
    let folders: Vec<FolderId> = found.iter().map(|m| m.folder).collect();
    assert_eq!(folders, [inbox_a1, sent_a1]);
    assert!(
        db.find_by_message_id("a1", "<9@example.org>")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn delete_messages_removes_only_those_uids() {
    let db = Db::open_in_memory().unwrap();
    let folder = inbox(&db, "a1");
    for uid in 1..=4 {
        db.upsert_message(folder, &message(uid, "m", i64::from(uid)))
            .unwrap();
    }
    assert_eq!(db.delete_messages(folder, &[2, 4, 9]).unwrap(), 2);
    assert_eq!(db.uids(folder).unwrap(), [1, 3]);
}

#[test]
fn uids_are_sorted() {
    let db = Db::open_in_memory().unwrap();
    let folder = inbox(&db, "a1");
    for uid in [9, 3, u32::MAX] {
        db.upsert_message(folder, &message(uid, "m", 1)).unwrap();
    }
    assert_eq!(db.uids(folder).unwrap(), [3, 9, u32::MAX]);
}

#[test]
fn reset_and_remove_drop_a_folders_messages() {
    let db = Db::open_in_memory().unwrap();
    let folder = inbox(&db, "a1");
    let other = db.upsert_folder("a1", "Sent", FolderRole::Sent).unwrap();
    db.upsert_message(folder, &message(1, "m", 1)).unwrap();
    let kept = db.upsert_message(other, &message(1, "m", 1)).unwrap();
    db.reset_folder(folder).unwrap();
    assert!(db.uids(folder).unwrap().is_empty());
    db.remove_account_cache("a1").unwrap();
    assert!(matches!(db.message(kept), Err(StoreError::NotFound)));
}

#[test]
fn unknown_messages_are_not_found() {
    let db = Db::open_in_memory().unwrap();
    let folder = inbox(&db, "a1");
    let id = db.upsert_message(folder, &message(1, "m", 1)).unwrap();
    db.delete_messages(folder, &[1]).unwrap();
    assert!(matches!(db.message(id), Err(StoreError::NotFound)));
    assert!(matches!(
        db.set_body_state(id, BodyState::Downloaded),
        Err(StoreError::NotFound)
    ));
}
