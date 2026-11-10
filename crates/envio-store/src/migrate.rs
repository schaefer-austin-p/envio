//! Numbered schema migrations. A migration is never edited once merged: a
//! schema change is a new `migrations/NNNN_name.sql` file and a new entry at
//! the end of `MIGRATIONS`. When you add one, also add a test that fills the
//! previous schema with rows and checks they survive the migration.

use rusqlite::Connection;

use crate::error::{Result, StoreError};

pub(crate) const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_init.sql"),
    include_str!("../migrations/0002_message_size.sql"),
];

/// The schema version of a fully migrated database.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

pub(crate) fn migrate(conn: &mut Connection) -> Result<()> {
    apply(conn, MIGRATIONS)
}

/// Runs the migrations `conn` hasn't had yet, each in its own transaction,
/// recording progress in `PRAGMA user_version`.
fn apply(conn: &mut Connection, migrations: &[&str]) -> Result<()> {
    let found: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let supported = migrations.len() as i64;
    if found > supported {
        return Err(StoreError::SchemaTooNew { found, supported });
    }
    let done = usize::try_from(found)
        .map_err(|_| StoreError::Damaged(format!("negative schema version {found}")))?;
    for (index, sql) in migrations.iter().enumerate().skip(done) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", index as i64 + 1)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    fn schema(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare(
                "SELECT type || ' ' || name || ' ' || ifnull(sql, '') FROM sqlite_master
                 WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
            )
            .unwrap();
        stmt.query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    fn version(conn: &Connection) -> i64 {
        conn.pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap()
    }

    #[test]
    fn every_older_schema_migrates_to_the_current_one() {
        let mut fresh = Connection::open_in_memory().unwrap();
        migrate(&mut fresh).unwrap();
        for start in 0..MIGRATIONS.len() {
            let mut conn = Connection::open_in_memory().unwrap();
            apply(&mut conn, &MIGRATIONS[..start]).unwrap();
            assert_eq!(version(&conn), start as i64);
            migrate(&mut conn).unwrap();
            assert_eq!(version(&conn), SCHEMA_VERSION, "from version {start}");
            assert_eq!(schema(&conn), schema(&fresh), "from version {start}");
        }
    }

    #[test]
    fn rows_cached_before_0002_survive_it_with_size_zero() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn, &MIGRATIONS[..1]).unwrap();
        conn.execute_batch(
            "INSERT INTO folders (account, path, role, uidvalidity, uidnext)
                 VALUES ('1700', 'INBOX', 'inbox', 7, 43);
             INSERT INTO messages (folder_id, uid, message_id, subject, date, flags, preview)
                 VALUES (1, 42, '<a@example.org>', 'Quarterly report', 1800000000, 5,
                         'Numbers attached');",
        )
        .unwrap();
        migrate(&mut conn).unwrap();
        let row: (i64, u32, String, String, i64, i64) = conn
            .query_row(
                "SELECT folder_id, uid, message_id, subject, flags, size FROM messages",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            row,
            (
                1,
                42,
                "<a@example.org>".into(),
                "Quarterly report".into(),
                5,
                0
            )
        );
        let uidnext: u32 = conn
            .query_row("SELECT uidnext FROM folders", [], |row| row.get(0))
            .unwrap();
        assert_eq!(uidnext, 43);
        // The search index still finds the old row.
        let hits: i64 = conn
            .query_row(
                "SELECT count(*) FROM messages_fts WHERE messages_fts MATCH 'quarterly'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1);
    }

    #[test]
    fn migrating_twice_changes_nothing() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        let before = schema(&conn);
        migrate(&mut conn).unwrap();
        assert_eq!(schema(&conn), before);
        assert_eq!(version(&conn), SCHEMA_VERSION);
    }

    #[test]
    fn a_newer_schema_is_refused() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        let error = migrate(&mut conn).unwrap_err();
        assert!(
            matches!(error, StoreError::SchemaTooNew { found: 99, supported } if supported == SCHEMA_VERSION),
            "{error:?}"
        );
    }

    #[test]
    fn a_failed_migration_keeps_the_previous_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        let error = apply(
            &mut conn,
            &["CREATE TABLE a (x);", "CREATE TABLE b (x); NOT SQL"],
        )
        .unwrap_err();
        assert!(matches!(error, StoreError::Sqlite(_)), "{error:?}");
        assert_eq!(version(&conn), 1);
        assert!(!schema(&conn).iter().any(|s| s.starts_with("table b ")));
    }

    #[test]
    fn migration_files_are_numbered_in_order() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".sql"))
            .collect();
        names.sort();
        assert_eq!(names.len(), MIGRATIONS.len(), "{names:?}");
        for (index, name) in names.iter().enumerate() {
            assert!(name.starts_with(&format!("{:04}_", index + 1)), "{name}");
        }
    }
}
