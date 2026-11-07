use std::fs;

use envio_store::{Opened, SCHEMA_VERSION, open_or_rebuild};

#[test]
fn a_new_path_is_created_then_reopened() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("envio.db");
    let (db, opened) = open_or_rebuild(&path).unwrap();
    assert_eq!(opened, Opened::Created);
    drop(db);
    let (_, opened) = open_or_rebuild(&path).unwrap();
    assert_eq!(opened, Opened::Existing);
}

#[test]
fn a_garbage_file_is_moved_aside_and_rebuilt() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("envio.db");
    let garbage = vec![0x5A_u8; 8192];
    fs::write(&path, &garbage).unwrap();
    fs::write(dir.path().join("envio.db-wal"), b"stale").unwrap();
    let (db, opened) = open_or_rebuild(&path).unwrap();
    assert!(matches!(opened, Opened::Rebuilt { .. }), "{opened:?}");
    assert_eq!(db.schema_version().unwrap(), SCHEMA_VERSION);
    db.check().unwrap();
    assert_eq!(
        fs::read(dir.path().join("envio.db.damaged")).unwrap(),
        garbage
    );
    let wal = fs::read(dir.path().join("envio.db-wal")).unwrap_or_default();
    assert_ne!(wal, b"stale");
}

#[test]
fn a_newer_schema_is_rebuilt() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("envio.db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.pragma_update(None, "user_version", 999).unwrap();
    drop(conn);
    let (db, opened) = open_or_rebuild(&path).unwrap();
    let Opened::Rebuilt { reason } = opened else {
        panic!("{opened:?}")
    };
    assert!(reason.contains("newer"), "{reason}");
    assert_eq!(db.schema_version().unwrap(), SCHEMA_VERSION);
}

#[test]
fn a_rebuilt_cache_can_be_rebuilt_again() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("envio.db");
    for _ in 0..2 {
        fs::write(&path, b"not a database, not at all, not even close........").unwrap();
        let (_, opened) = open_or_rebuild(&path).unwrap();
        assert!(matches!(opened, Opened::Rebuilt { .. }));
    }
}

#[test]
fn a_path_that_cannot_be_opened_is_an_error_not_a_rebuild() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing").join("envio.db");
    assert!(open_or_rebuild(&path).is_err());
    assert!(!dir.path().join("missing").exists());
}
