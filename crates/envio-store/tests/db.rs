use envio_store::{Db, SCHEMA_VERSION};

#[test]
fn a_new_database_is_at_the_current_schema() {
    let db = Db::open_in_memory().unwrap();
    assert_eq!(db.schema_version().unwrap(), SCHEMA_VERSION);
    db.check().unwrap();
}

#[test]
fn a_file_database_reopens_at_the_same_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("envio.db");
    drop(Db::open(&path).unwrap());
    let db = Db::open(&path).unwrap();
    assert_eq!(db.schema_version().unwrap(), SCHEMA_VERSION);
    db.check().unwrap();
}
