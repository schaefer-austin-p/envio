use envio_store::{Db, Envelope, Flags, FolderId, FolderRole, NewMessage, StoreError};

fn message(uid: u32) -> NewMessage {
    NewMessage {
        uid,
        size: 1_000,
        envelope: Envelope {
            message_id: Some(format!("<{uid}@example.org>")),
            subject: format!("m{uid}"),
            from: "Ada <ada@example.org>".into(),
            to: "bob@example.org".into(),
            date: i64::from(uid),
            in_reply_to: None,
            references: Vec::new(),
            has_attachments: false,
        },
        flags: Flags::empty(),
        preview: String::new(),
    }
}

fn folder(db: &Db, path: &str) -> FolderId {
    db.upsert_folder("a1", path, FolderRole::Other).unwrap()
}

fn words(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn keywords_round_trip_sorted_and_deduplicated() {
    let db = Db::open_in_memory().unwrap();
    let f = folder(&db, "INBOX");
    for uid in 1..=3 {
        db.upsert_message(f, &message(uid)).unwrap();
    }
    db.set_keywords(f, 1, &[]).unwrap();
    db.set_keywords(f, 2, &words(&["$Forwarded"])).unwrap();
    db.set_keywords(f, 3, &words(&["$b", "$Label1", "$b", "$a"]))
        .unwrap();
    assert_eq!(db.keywords_of(f, 1).unwrap(), Some(vec![]));
    assert_eq!(db.keywords_of(f, 2).unwrap(), Some(words(&["$Forwarded"])));
    assert_eq!(
        db.keywords_of(f, 3).unwrap(),
        Some(words(&["$Label1", "$a", "$b"]))
    );
    // Setting again replaces.
    db.set_keywords(f, 2, &[]).unwrap();
    assert_eq!(db.keywords_of(f, 2).unwrap(), Some(vec![]));
}

#[test]
fn unknown_and_missing_rows_have_no_keywords_answer() {
    let db = Db::open_in_memory().unwrap();
    let f = folder(&db, "INBOX");
    db.upsert_message(f, &message(1)).unwrap();
    assert_eq!(db.keywords_of(f, 1).unwrap(), None);
    assert_eq!(db.keywords_of(f, 99).unwrap(), None);
    let error = db.set_keywords(f, 99, &[]).unwrap_err();
    assert!(matches!(error, StoreError::NotFound), "{error:?}");
}

#[test]
fn upsert_keeps_known_keywords_and_a_new_row_is_unknown() {
    let db = Db::open_in_memory().unwrap();
    let f = folder(&db, "INBOX");
    db.upsert_message(f, &message(1)).unwrap();
    db.set_keywords(f, 1, &words(&["$Forwarded"])).unwrap();
    let mut again = message(1);
    again.preview = "new preview".into();
    db.upsert_message(f, &again).unwrap();
    assert_eq!(db.keywords_of(f, 1).unwrap(), Some(words(&["$Forwarded"])));
    db.upsert_message(f, &message(2)).unwrap();
    assert_eq!(db.keywords_of(f, 2).unwrap(), None);
}

#[test]
fn has_unknown_keywords_looks_only_at_the_listed_uids_of_the_folder() {
    let db = Db::open_in_memory().unwrap();
    let f = folder(&db, "INBOX");
    let g = folder(&db, "Archive");
    for uid in 1..=3 {
        db.upsert_message(f, &message(uid)).unwrap();
    }
    db.upsert_message(g, &message(7)).unwrap();
    assert!(db.has_unknown_keywords(f, &[1, 2]).unwrap());
    assert!(!db.has_unknown_keywords(f, &[]).unwrap());
    db.set_keywords(f, 1, &[]).unwrap();
    db.set_keywords(f, 2, &words(&["$a"])).unwrap();
    // UID 3 is still NULL but not listed.
    assert!(!db.has_unknown_keywords(f, &[1, 2]).unwrap());
    assert!(db.has_unknown_keywords(f, &[1, 2, 3]).unwrap());
    // Another folder's NULL row (uid 7) and an uncached UID never count.
    assert!(!db.has_unknown_keywords(f, &[1, 2, 7, 99]).unwrap());
    assert!(db.has_unknown_keywords(g, &[7]).unwrap());
}

#[test]
fn has_unknown_keywords_checks_every_chunk_of_a_long_uid_list() {
    let db = Db::open_in_memory().unwrap();
    let f = folder(&db, "INBOX");
    let uids: Vec<u32> = (1..=1_200).collect();
    for &uid in &uids {
        db.upsert_message(f, &message(uid)).unwrap();
        if uid != 1_100 {
            db.set_keywords(f, uid, &[]).unwrap();
        }
    }
    assert!(db.has_unknown_keywords(f, &uids).unwrap());
    db.set_keywords(f, 1_100, &[]).unwrap();
    assert!(!db.has_unknown_keywords(f, &uids).unwrap());
}

#[test]
fn keywords_by_uid_is_ascending_with_nones() {
    let db = Db::open_in_memory().unwrap();
    let f = folder(&db, "INBOX");
    let g = folder(&db, "Archive");
    for uid in [30, 10, 20] {
        db.upsert_message(f, &message(uid)).unwrap();
    }
    db.upsert_message(g, &message(5)).unwrap();
    db.set_keywords(f, 20, &words(&["$b", "$a"])).unwrap();
    db.set_keywords(f, 30, &[]).unwrap();
    assert_eq!(
        db.keywords_by_uid(f).unwrap(),
        vec![
            (10, None),
            (20, Some(words(&["$a", "$b"]))),
            (30, Some(vec![]))
        ]
    );
}
