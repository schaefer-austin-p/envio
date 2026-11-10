use envio_store::{Db, Envelope, Flags, FolderRole, NewMessage};

fn message(uid: u32, flags: Flags) -> NewMessage {
    NewMessage {
        uid,
        size: 4_200 + uid,
        envelope: Envelope {
            message_id: Some(format!("<{uid}@example.org>")),
            subject: format!("Message {uid}"),
            from: "Ada <ada@example.org>".into(),
            to: "bob@example.org".into(),
            date: 1_800_000_000,
            in_reply_to: None,
            references: Vec::new(),
        },
        flags,
        preview: String::new(),
    }
}

#[test]
fn a_message_is_found_by_its_uid_with_its_size() {
    let db = Db::open_in_memory().unwrap();
    let inbox = db
        .upsert_folder("1700", "INBOX", FolderRole::Inbox)
        .unwrap();
    db.upsert_message(inbox, &message(7, Flags::SEEN)).unwrap();
    let found = db.message_at(inbox, 7).unwrap().unwrap();
    assert_eq!(found.uid, 7);
    assert_eq!(found.size, 4_207);
    assert_eq!(found.flags, Flags::SEEN);
    assert!(db.message_at(inbox, 8).unwrap().is_none());
}

#[test]
fn flags_come_in_one_read_ascending_by_uid() {
    let db = Db::open_in_memory().unwrap();
    let inbox = db
        .upsert_folder("1700", "INBOX", FolderRole::Inbox)
        .unwrap();
    let other = db
        .upsert_folder("1700", "Archive", FolderRole::Archive)
        .unwrap();
    db.upsert_message(inbox, &message(9, Flags::FLAGGED))
        .unwrap();
    db.upsert_message(inbox, &message(3, Flags::SEEN)).unwrap();
    db.upsert_message(other, &message(5, Flags::empty()))
        .unwrap();
    assert_eq!(
        db.flags_by_uid(inbox).unwrap(),
        [(3, Flags::SEEN), (9, Flags::FLAGGED)]
    );
}

#[test]
fn a_folder_is_found_by_its_path_in_its_account_only() {
    let db = Db::open_in_memory().unwrap();
    let id = db
        .upsert_folder("1700", "Archive", FolderRole::Archive)
        .unwrap();
    db.upsert_folder("1800", "Archive", FolderRole::Archive)
        .unwrap();
    let folder = db.folder_by_path("1700", "Archive").unwrap().unwrap();
    assert_eq!((folder.id, folder.role), (id, FolderRole::Archive));
    assert!(db.folder_by_path("1700", "Missing").unwrap().is_none());
}
