//! The SQLite cache: opening, one transaction per sync step, integrity checks.

use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

use crate::error::{Result, StoreError};
use crate::migrate;

/// The mail database. It is a cache of the server: anything not on the
/// server yet lives in files outside it (journal, Outbox, drafts).
pub struct Db {
    conn: Connection,
}

impl Db {
    /// Opens (or creates) the cache at `path` and migrates it.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            crate::private::create_private_dir(dir)?;
        }
        // Private before SQLite makes it: the -wal and -shm files copy its
        // mode. An older, open file is narrowed.
        crate::private::create_private_file(path)?;
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update_and_check(None, "journal_mode", "wal", |row| row.get::<_, String>(0))?;
        conn.pragma_update(None, "synchronous", "normal")?;
        Self::setup(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::setup(Connection::open_in_memory()?)
    }

    fn setup(mut conn: Connection) -> Result<Self> {
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", true)?;
        migrate::migrate(&mut conn)?;
        Ok(Self { conn })
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    /// Runs `step` in one transaction: all of it is saved, or none of it.
    /// Sync runs each resumable step through here. Steps don't nest.
    pub fn step<T>(&self, step: impl FnOnce(&Self) -> Result<T>) -> Result<T> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = step(self).and_then(|value| {
            self.conn.execute_batch("COMMIT")?;
            Ok(value)
        });
        if result.is_err() && !self.conn.is_autocommit() {
            let _ = self.conn.execute_batch("ROLLBACK");
        }
        result
    }

    /// Runs `body` atomically whether or not a step is open: SQLite
    /// savepoints nest inside transactions and inside each other.
    pub(crate) fn savepoint<T>(&self, body: impl FnOnce() -> Result<T>) -> Result<T> {
        self.conn.execute_batch("SAVEPOINT envio")?;
        let result = body().and_then(|value| {
            self.conn.execute_batch("RELEASE envio")?;
            Ok(value)
        });
        if result.is_err() {
            let _ = self.conn.execute_batch("ROLLBACK TO envio; RELEASE envio");
        }
        result
    }

    /// Checks the file's structure, then that the search index matches the
    /// `messages` rows (FTS5 `integrity-check` with rank 1, which re-reads
    /// every row). `Damaged` means: rebuild. Other errors say nothing about
    /// the file. Cost grows with the cache: on the phone (arm64, warm) about
    /// 80 ms for 10 000 messages and 340 ms for 40 000.
    pub fn check(&self) -> Result<()> {
        let verdict: String = self
            .conn
            .query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if verdict != "ok" {
            return Err(StoreError::Damaged(verdict));
        }
        let checked = self.conn.execute(
            // Rank 0 checks the index only against itself; rank 1 also
            // compares it with the external content table.
            "INSERT INTO messages_fts (messages_fts, rank) VALUES ('integrity-check', 1)",
            [],
        );
        match checked {
            Ok(_) => Ok(()),
            // Only corruption means rebuild; BUSY, IOERR, READONLY and the
            // like are ordinary errors and say nothing about the file.
            Err(error) => {
                let error = StoreError::Sqlite(error);
                if error.is_damage() {
                    Err(StoreError::Damaged(format!("search index: {error}")))
                } else {
                    Err(error)
                }
            }
        }
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }
}

/// `NotFound` when a statement changed no row.
pub(crate) fn found(changed: usize) -> Result<()> {
    if changed == 0 {
        Err(StoreError::NotFound)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_table(db: &Db, name: &str) -> bool {
        db.conn()
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = ?1",
                [name],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
            == 1
    }

    #[test]
    fn a_busy_database_is_not_damaged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("envio.db");
        let db = Db::open(&path).unwrap();
        db.conn().busy_timeout(Duration::ZERO).unwrap();
        let other = Connection::open(&path).unwrap();
        other.execute_batch("BEGIN IMMEDIATE").unwrap();
        let error = db.check().unwrap_err();
        assert!(!error.is_damage(), "{error}");
        assert!(matches!(error, StoreError::Sqlite(_)), "{error}");
        other.execute_batch("ROLLBACK").unwrap();
        db.check().unwrap();
    }

    #[test]
    fn a_search_index_that_disagrees_with_the_messages_is_damaged() {
        let db = Db::open_in_memory().unwrap();
        db.check().unwrap();
        // An index entry for a message row that does not exist, as a write
        // that bypassed the triggers would leave behind.
        db.conn()
            .execute(
                "INSERT INTO messages_fts (rowid, subject, from_addr, to_addrs, preview)
                 VALUES (42, 'ghost', 'a@example.org', 'b@example.org', 'gone')",
                [],
            )
            .unwrap();
        let error = db.check().unwrap_err();
        assert!(
            matches!(&error, StoreError::Damaged(why) if why.starts_with("search index")),
            "{error}"
        );
    }

    #[test]
    fn a_step_that_fails_saves_nothing() {
        let db = Db::open_in_memory().unwrap();
        let error = db
            .step(|db| {
                db.conn().execute("CREATE TABLE t (x)", [])?;
                Err::<(), _>(StoreError::NotFound)
            })
            .unwrap_err();
        assert!(matches!(error, StoreError::NotFound));
        assert!(!has_table(&db, "t"));
        assert!(db.conn().is_autocommit());
    }

    #[test]
    fn a_step_that_succeeds_is_saved() {
        let db = Db::open_in_memory().unwrap();
        let value = db
            .step(|db| {
                db.conn().execute("CREATE TABLE t (x)", [])?;
                Ok(7)
            })
            .unwrap();
        assert_eq!(value, 7);
        assert!(has_table(&db, "t"));
    }

    #[test]
    fn foreign_keys_are_on() {
        let db = Db::open_in_memory().unwrap();
        let on: i64 = db
            .conn()
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        assert_eq!(on, 1);
    }
}
