use std::collections::HashSet;
use std::fs;

use envio_store::{
    Bodies, BodyState, Db, Envelope, Flags, FolderId, FolderRole, NewMessage, StoreError,
    StorePaths,
};

fn message(uid: u32, date: i64) -> NewMessage {
    NewMessage {
        uid,
        size: 1_000,
        envelope: Envelope {
            date,
            ..Envelope::default()
        },
        flags: Flags::empty(),
        preview: String::new(),
    }
}

#[test]
fn a_body_round_trips_and_can_be_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let bodies = Bodies::new(dir.path().join("bodies"));
    bodies
        .write(FolderId(3), 7, b"Subject: a\r\n\r\nhi")
        .unwrap();
    assert_eq!(
        bodies.read(FolderId(3), 7).unwrap(),
        b"Subject: a\r\n\r\nhi"
    );
    bodies.write(FolderId(3), 7, b"second").unwrap();
    assert_eq!(bodies.read(FolderId(3), 7).unwrap(), b"second");
}

#[test]
fn a_missing_body_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let bodies = Bodies::new(dir.path().join("bodies"));
    assert!(matches!(
        bodies.read(FolderId(1), 1),
        Err(StoreError::NotFound)
    ));
}

#[test]
fn removing_a_folder_drops_its_bodies_only() {
    let dir = tempfile::tempdir().unwrap();
    let bodies = Bodies::new(dir.path().join("bodies"));
    bodies.write(FolderId(1), 1, b"a").unwrap();
    bodies.write(FolderId(2), 1, b"b").unwrap();
    bodies.remove_folder(FolderId(1)).unwrap();
    bodies.remove_folder(FolderId(9)).unwrap();
    assert!(matches!(
        bodies.read(FolderId(1), 1),
        Err(StoreError::NotFound)
    ));
    assert_eq!(bodies.read(FolderId(2), 1).unwrap(), b"b");
}

#[test]
fn retain_removes_unkept_bodies_and_temp_files_only() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("bodies");
    let bodies = Bodies::new(&root);
    assert_eq!(bodies.retain(&HashSet::new()).unwrap(), 0);
    bodies.write(FolderId(1), 1, b"keep").unwrap();
    bodies.write(FolderId(1), 2, b"drop").unwrap();
    fs::write(root.join("1").join(".3.eml.tmp"), b"torn").unwrap();
    fs::write(root.join("1").join("notes.txt"), b"not ours").unwrap();
    let keep = HashSet::from([(FolderId(1), 1)]);
    assert_eq!(bodies.retain(&keep).unwrap(), 2);
    assert_eq!(bodies.read(FolderId(1), 1).unwrap(), b"keep");
    assert!(matches!(
        bodies.read(FolderId(1), 2),
        Err(StoreError::NotFound)
    ));
    assert!(!root.join("1").join(".3.eml.tmp").exists());
    assert!(root.join("1").join("notes.txt").exists());
}

#[test]
fn retention_keeps_recent_downloaded_bodies() {
    let db = Db::open_in_memory().unwrap();
    let folder = db.upsert_folder("a1", "INBOX", FolderRole::Inbox).unwrap();
    let old = db.upsert_message(folder, &message(1, 100)).unwrap();
    let recent = db.upsert_message(folder, &message(2, 500)).unwrap();
    db.upsert_message(folder, &message(3, 600)).unwrap();
    db.set_body_state(old, BodyState::Downloaded).unwrap();
    db.set_body_state(recent, BodyState::Downloaded).unwrap();

    assert_eq!(db.mark_bodies_pruned(300).unwrap(), 1);
    assert_eq!(db.message(old).unwrap().body, BodyState::Missing);
    assert_eq!(db.message(recent).unwrap().body, BodyState::Downloaded);
    assert_eq!(
        db.bodies_to_keep(300).unwrap(),
        HashSet::from([(folder, 2)])
    );
}

#[test]
fn store_paths_split_cache_from_unsent_work() {
    let paths = StorePaths::new("/c", "/d");
    for cache in [paths.db(), paths.bodies()] {
        assert!(cache.starts_with("/c"), "{cache:?}");
    }
    for data in [
        paths.accounts(),
        paths.journal(),
        paths.outbox(),
        paths.drafts(),
    ] {
        assert!(data.starts_with("/d"), "{data:?}");
    }
}
