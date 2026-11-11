use std::fs;

use envio_store::{Opened, SCHEMA_VERSION, StorePaths, open_or_rebuild};

#[test]
fn a_new_path_is_created_then_reopened() {
    let dir = tempfile::tempdir().unwrap();
    let paths = StorePaths::new(dir.path(), dir.path().join("data"));
    let (db, opened) = open_or_rebuild(&paths).unwrap();
    assert_eq!(opened, Opened::Created);
    drop(db);
    let (_, opened) = open_or_rebuild(&paths).unwrap();
    assert_eq!(opened, Opened::Existing);
}

#[test]
fn a_garbage_file_is_moved_aside_and_rebuilt() {
    let dir = tempfile::tempdir().unwrap();
    let paths = StorePaths::new(dir.path(), dir.path().join("data"));
    let garbage = vec![0x5A_u8; 8192];
    fs::write(paths.db(), &garbage).unwrap();
    fs::write(dir.path().join("envio.db-wal"), b"stale").unwrap();
    let (db, opened) = open_or_rebuild(&paths).unwrap();
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
    let paths = StorePaths::new(dir.path(), dir.path().join("data"));
    let conn = rusqlite::Connection::open(paths.db()).unwrap();
    conn.pragma_update(None, "user_version", 999).unwrap();
    drop(conn);
    let (db, opened) = open_or_rebuild(&paths).unwrap();
    let Opened::Rebuilt { reason } = opened else {
        panic!("{opened:?}")
    };
    assert!(reason.contains("newer"), "{reason}");
    assert_eq!(db.schema_version().unwrap(), SCHEMA_VERSION);
}

#[test]
fn a_rebuilt_cache_can_be_rebuilt_again() {
    let dir = tempfile::tempdir().unwrap();
    let paths = StorePaths::new(dir.path(), dir.path().join("data"));
    for _ in 0..2 {
        fs::write(
            paths.db(),
            b"not a database, not at all, not even close........",
        )
        .unwrap();
        let (_, opened) = open_or_rebuild(&paths).unwrap();
        assert!(matches!(opened, Opened::Rebuilt { .. }));
    }
}

#[test]
fn a_first_launch_creates_the_cache_folder() {
    let dir = tempfile::tempdir().unwrap();
    let paths = StorePaths::new(dir.path().join("missing"), dir.path().join("data"));
    let (_, opened) = open_or_rebuild(&paths).unwrap();
    assert_eq!(opened, Opened::Created);
    assert!(paths.db().exists());
}

#[test]
fn a_rebuild_clears_the_bodies() {
    let dir = tempfile::tempdir().unwrap();
    let paths = StorePaths::new(dir.path(), dir.path().join("data"));
    fs::create_dir_all(paths.bodies().join("3")).unwrap();
    fs::write(paths.bodies().join("3").join("120.eml"), b"old").unwrap();
    fs::write(paths.db(), vec![0x5A_u8; 8192]).unwrap();
    let (_, opened) = open_or_rebuild(&paths).unwrap();
    assert!(matches!(opened, Opened::Rebuilt { .. }));
    assert!(!paths.bodies().exists());
}

#[test]
fn a_healthy_open_keeps_the_bodies() {
    let dir = tempfile::tempdir().unwrap();
    let paths = StorePaths::new(dir.path(), dir.path().join("data"));
    open_or_rebuild(&paths).unwrap();
    fs::create_dir_all(paths.bodies().join("3")).unwrap();
    fs::write(paths.bodies().join("3").join("120.eml"), b"keep").unwrap();
    open_or_rebuild(&paths).unwrap();
    assert!(paths.bodies().join("3").join("120.eml").exists());
}

#[test]
fn a_zero_byte_cache_counts_as_created() {
    let dir = tempfile::tempdir().unwrap();
    let paths = StorePaths::new(dir.path(), dir.path().join("data"));
    fs::write(paths.db(), b"").unwrap();
    let (db, opened) = open_or_rebuild(&paths).unwrap();
    assert_eq!(opened, Opened::Created);
    db.check().unwrap();
    assert!(!dir.path().join("envio.db.damaged").exists());
}
