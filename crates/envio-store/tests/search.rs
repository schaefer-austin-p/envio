use envio_store::{Db, Envelope, Flags, FolderId, FolderRole, NewMessage};

fn message(uid: u32, subject: &str, from: &str, date: i64) -> NewMessage {
    NewMessage {
        uid,
        size: 1_000,
        envelope: Envelope {
            message_id: Some(format!("<{uid}@example.org>")),
            subject: subject.into(),
            from: from.into(),
            to: "me@example.org".into(),
            date,
            in_reply_to: None,
            references: Vec::new(),
        },
        flags: Flags::empty(),
        preview: String::new(),
    }
}

fn setup() -> (Db, FolderId) {
    let db = Db::open_in_memory().unwrap();
    let inbox = db.upsert_folder("a1", "INBOX", FolderRole::Inbox).unwrap();
    db.upsert_message(
        inbox,
        &message(1, "Quarterly report", "Ada <ada@example.org>", 100),
    )
    .unwrap();
    db.upsert_message(
        inbox,
        &message(2, "Lunch on Friday", "Bob <bob@example.org>", 200),
    )
    .unwrap();
    (db, inbox)
}

fn uids(db: &Db, query: &str) -> Vec<u32> {
    db.search("a1", query, 50)
        .unwrap()
        .iter()
        .map(|m| m.uid)
        .collect()
}

#[test]
fn finds_by_subject_sender_and_prefix() {
    let (db, _) = setup();
    assert_eq!(uids(&db, "report"), [1]);
    assert_eq!(uids(&db, "bob"), [2]);
    assert_eq!(uids(&db, "quart"), [1]);
    assert_eq!(uids(&db, "REPORT"), [1]);
}

#[test]
fn every_word_must_match() {
    let (db, _) = setup();
    assert_eq!(uids(&db, "lunch friday"), [2]);
    assert!(uids(&db, "lunch report").is_empty());
}

#[test]
fn results_are_newest_first_and_limited() {
    let (db, inbox) = setup();
    db.upsert_message(
        inbox,
        &message(3, "Report again", "Ada <ada@example.org>", 300),
    )
    .unwrap();
    assert_eq!(uids(&db, "report"), [3, 1]);
    let one: Vec<u32> = db
        .search("a1", "report", 1)
        .unwrap()
        .iter()
        .map(|m| m.uid)
        .collect();
    assert_eq!(one, [3]);
}

#[test]
fn other_accounts_are_not_searched() {
    let (db, _) = setup();
    let other = db.upsert_folder("a2", "INBOX", FolderRole::Inbox).unwrap();
    db.upsert_message(other, &message(1, "Quarterly report", "x", 1))
        .unwrap();
    assert_eq!(db.search("a2", "report", 50).unwrap().len(), 1);
    assert_eq!(uids(&db, "report"), [1]);
}

#[test]
fn the_index_follows_updates_and_deletes() {
    let (db, inbox) = setup();
    db.upsert_message(
        inbox,
        &message(1, "Annual summary", "Ada <ada@example.org>", 100),
    )
    .unwrap();
    assert!(uids(&db, "quarterly").is_empty());
    assert_eq!(uids(&db, "annual"), [1]);
    db.delete_messages(inbox, &[1]).unwrap();
    assert!(uids(&db, "annual").is_empty());
    db.remove_folder(inbox).unwrap();
    assert!(uids(&db, "lunch").is_empty());
    db.check().unwrap();
}

#[test]
fn query_syntax_is_plain_text() {
    let (db, _) = setup();
    for query in ["\"", "-report", "report OR", "NEAR(", "a:b", "*", "(", "^"] {
        db.search("a1", query, 50).unwrap();
    }
    assert!(uids(&db, "   ").is_empty());
}
