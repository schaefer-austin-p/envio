//! `GraphMap`: its own connection to the cache file, for code that has no
//! `Db` (envio-sync has no rusqlite). It never migrates and keeps nothing in
//! memory, so a reset made through `Db` shows on the next call.

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

use super::{Allocation, GraphFolder, MapRow, MetaChange, to_u32, to_u64};
use crate::error::{Result, StoreError};
use crate::folders::FolderId;

/// The schema version that has the graph tables.
const GRAPH_SCHEMA: i64 = 6;

const ROW: &str = "uid, graph_id, received_at, flags, categories, modseq";

pub struct GraphMap {
    conn: Connection,
}

impl GraphMap {
    /// Opens the existing cache file. `Db::open` always migrates first, so a
    /// schema older than 0006 only happens on misuse.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update_and_check(None, "journal_mode", "wal", |row| row.get::<_, String>(0))?;
        conn.pragma_update(None, "foreign_keys", true)?;
        let found: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if found < GRAPH_SCHEMA {
            return Err(StoreError::SchemaTooOld {
                found,
                needed: GRAPH_SCHEMA,
            });
        }
        Ok(Self { conn })
    }

    /// Runs `body` in one transaction. Reads use a deferred one, so they
    /// don't take the write lock and wait behind a `Db` step.
    fn tx<T>(&self, write: bool, body: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        self.conn
            .execute_batch(if write { "BEGIN IMMEDIATE" } else { "BEGIN" })?;
        let result = body(&self.conn).and_then(|value| {
            self.conn.execute_batch("COMMIT")?;
            Ok(value)
        });
        if result.is_err() && !self.conn.is_autocommit() {
            let _ = self.conn.execute_batch("ROLLBACK");
        }
        result
    }

    pub fn folder(&self, folder: FolderId) -> Result<Option<GraphFolder>> {
        self.tx(false, |conn| super::folder(conn, folder))
    }

    /// The folder row that maps this Graph folder id.
    pub fn folder_row_of(&self, graph_id: &str) -> Result<Option<FolderId>> {
        self.tx(false, |conn| {
            Ok(conn
                .query_row(
                    "SELECT folder_row_id FROM graph_folders WHERE graph_id = ?1",
                    [graph_id],
                    |row| row.get(0),
                )
                .optional()?
                .map(FolderId))
        })
    }

    pub fn reset(&self, folder: FolderId) -> Result<u32> {
        self.tx(true, |conn| super::reset(conn, folder))
    }

    pub fn allocate(&self, folder: FolderId, ids: &[(String, i64)]) -> Result<Allocation> {
        self.tx(true, |conn| super::allocate(conn, folder, ids))
    }

    pub fn allocate_low(
        &self,
        folder: FolderId,
        id: &str,
        received_ms: i64,
    ) -> Result<Option<u32>> {
        self.tx(true, |conn| {
            super::allocate_low(conn, folder, id, received_ms)
        })
    }

    pub fn apply(
        &self,
        folder: FolderId,
        changes: &[MetaChange],
        removed: &[String],
    ) -> Result<()> {
        self.tx(true, |conn| super::apply(conn, folder, changes, removed))
    }

    /// Rows received after `received_after_ms`, oldest first.
    pub fn rows_since(&self, folder: FolderId, received_after_ms: i64) -> Result<Vec<MapRow>> {
        self.query(
            &format!(
                "SELECT {ROW} FROM graph_messages
                 WHERE folder_row_id = ?1 AND received_at > ?2 ORDER BY received_at, uid"
            ),
            params![folder.0, received_after_ms],
        )
    }

    /// Rows whose flags or categories changed after `modseq`.
    pub fn rows_changed(&self, folder: FolderId, modseq: u64) -> Result<Vec<MapRow>> {
        let modseq = i64::try_from(modseq).unwrap_or(i64::MAX);
        self.query(
            &format!(
                "SELECT {ROW} FROM graph_messages
                 WHERE folder_row_id = ?1 AND modseq > ?2 ORDER BY uid"
            ),
            params![folder.0, modseq],
        )
    }

    /// How many messages are mapped, both ranges.
    pub fn count(&self, folder: FolderId) -> Result<u32> {
        self.tx(false, |conn| {
            let n: i64 = conn.query_row(
                "SELECT count(*) FROM graph_messages WHERE folder_row_id = ?1",
                [folder.0],
                |row| row.get(0),
            )?;
            to_u32(n, "count")
        })
    }

    pub fn row(&self, folder: FolderId, uid: u32) -> Result<Option<MapRow>> {
        Ok(self
            .query(
                &format!("SELECT {ROW} FROM graph_messages WHERE folder_row_id = ?1 AND uid = ?2"),
                params![folder.0, uid],
            )?
            .pop())
    }

    pub fn uid_of(&self, folder: FolderId, graph_id: &str) -> Result<Option<u32>> {
        self.tx(false, |conn| {
            super::uid_of(conn, folder, graph_id)?
                .map(|uid| to_u32(uid, "uid"))
                .transpose()
        })
    }

    /// The graph ids of the uids that are mapped; the others are left out.
    pub fn ids_of(&self, folder: FolderId, uids: &[u32]) -> Result<Vec<(u32, String)>> {
        self.tx(false, |conn| {
            let mut stmt = conn.prepare(
                "SELECT graph_id FROM graph_messages WHERE folder_row_id = ?1 AND uid = ?2",
            )?;
            let mut found = Vec::new();
            for &uid in uids {
                let id: Option<String> = stmt
                    .query_row(params![folder.0, uid], |row| row.get(0))
                    .optional()?;
                found.extend(id.map(|id| (uid, id)));
            }
            Ok(found)
        })
    }

    fn query(&self, sql: &str, args: impl rusqlite::Params) -> Result<Vec<MapRow>> {
        self.tx(false, |conn| {
            let mut stmt = conn.prepare(sql)?;
            let raw: Vec<(i64, String, i64, i64, String, i64)> = stmt
                .query_map(args, |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                })?
                .collect::<rusqlite::Result<_>>()?;
            raw.into_iter().map(into_row).collect()
        })
    }
}

fn into_row(raw: (i64, String, i64, i64, String, i64)) -> Result<MapRow> {
    let (uid, graph_id, received_ms, flags, categories, modseq) = raw;
    Ok(MapRow {
        uid: to_u32(uid, "uid")?,
        graph_id,
        received_ms,
        flags: u8::try_from(flags)
            .map_err(|_| StoreError::Damaged(format!("graph map: bad flags {flags}")))?,
        categories: serde_json::from_str(&categories)
            .map_err(|e| StoreError::Damaged(format!("graph map categories: {e}")))?,
        modseq: to_u64(modseq, "modseq")?,
    })
}
