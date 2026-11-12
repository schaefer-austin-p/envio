use envio_store::{BodyState, Db, Envelope, Flags, FolderId, FolderRole, MessageRowId, NewMessage};

fn add(db: &Db, folder: FolderId, uid: u32, (date, size): (i64, u32)) -> MessageRowId {
    db.upsert_message(
        folder,
        &NewMessage {
            uid,
            size,
            envelope: Envelope {
                subject: format!("m{uid}"),
                date,
                ..Envelope::default()
            },
            flags: Flags::empty(),
            preview: String::new(),
        },
    )
    .unwrap()
}

fn inbox(db: &Db) -> FolderId {
    db.upsert_folder("a", "INBOX", FolderRole::Inbox).unwrap()
}

#[test]
fn only_messages_without_a_body_are_candidates() {
    let db = Db::open_in_memory().unwrap();
    let folder = inbox(&db);
    let missing = add(&db, folder, 1, (100, 10));
    let have = add(&db, folder, 2, (100, 10));
    db.set_body_state(have, BodyState::Downloaded).unwrap();
    assert_eq!(
        db.prefetch_candidates(folder, 0, 1_000, 10).unwrap(),
        [missing]
    );
}

#[test]
fn the_date_and_size_bounds_are_inclusive() {
    let db = Db::open_in_memory().unwrap();
    let folder = inbox(&db);
    add(&db, folder, 1, (99, 10));
    let on_date = add(&db, folder, 2, (100, 10));
    let on_size = add(&db, folder, 3, (100, 500));
    add(&db, folder, 4, (100, 501));
    assert_eq!(
        db.prefetch_candidates(folder, 100, 500, 10).unwrap(),
        [on_size, on_date]
    );
}

#[test]
fn candidates_come_newest_first_up_to_the_limit() {
    let db = Db::open_in_memory().unwrap();
    let folder = inbox(&db);
    let old = add(&db, folder, 1, (100, 10));
    let newest = add(&db, folder, 2, (300, 10));
    let same_a = add(&db, folder, 3, (200, 10));
    let same_b = add(&db, folder, 4, (200, 10));
    assert_eq!(
        db.prefetch_candidates(folder, 0, 1_000, 10).unwrap(),
        [newest, same_b, same_a, old]
    );
    assert_eq!(
        db.prefetch_candidates(folder, 0, 1_000, 2).unwrap(),
        [newest, same_b]
    );
}

#[test]
fn another_folder_is_not_searched() {
    let db = Db::open_in_memory().unwrap();
    let folder = inbox(&db);
    let other = db
        .upsert_folder("a", "Archive", FolderRole::Archive)
        .unwrap();
    add(&db, other, 1, (100, 10));
    assert!(
        db.prefetch_candidates(folder, 0, 1_000, 10)
            .unwrap()
            .is_empty()
    );
}
