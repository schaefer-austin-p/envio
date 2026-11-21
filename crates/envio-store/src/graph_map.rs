//! The uid map of the Graph backend: Graph has no uids, so envio mints them
//! per folder. The functions here take a connection and open no transaction
//! of their own; `Db` calls them inside a step and `GraphMap` (in `open`)
//! wraps each call in its own transaction.

mod open;

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

use rusqlite::{Connection, OptionalExtension, params};

pub use open::GraphMap;

use crate::db::{Db, found};
use crate::error::{Result, StoreError};
use crate::folders::FolderId;

/// The first uid of the high range, `[2^31, 2^32)`, which counts up.
const HIGH_START: i64 = 1 << 31;
/// The high range ends here: a folder is reset before `uid_next` reaches it,
/// so `uid_next` always fits a `u32`.
const HIGH_END: i64 = u32::MAX as i64;
/// The first uid of the low range, `[1, 2^31)`, which counts down.
const LOW_START: i64 = HIGH_START - 1;

/// One folder's map state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphFolder {
    pub graph_id: String,
    /// Non-zero; reported as `uid_validity`.
    pub generation: u32,
    pub uid_next: u32,
    pub low_next: u32,
    pub modseq: u64,
    pub delta_link: String,
}

/// What `upsert_folder` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Upserted {
    /// A new map row, with its generation.
    New(u32),
    Same,
    /// The folder row now names another Graph folder: reset, new generation.
    Changed(u32),
}

/// The result of allocating high-range uids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Allocation {
    /// Every asked id with its uid, in the order of the ids' received time.
    Done(Vec<(String, u32)>),
    /// `uid_next` would reach `2^32 - 1`; nothing was written.
    Exhausted,
}

/// What the delta feed says about one message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaChange {
    pub graph_id: String,
    pub flags: u8,
    pub categories: Vec<String>,
}

/// One mapped message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapRow {
    pub uid: u32,
    pub graph_id: String,
    pub received_ms: i64,
    pub flags: u8,
    pub categories: Vec<String>,
    pub modseq: u64,
}

pub(crate) fn to_u32(value: i64, what: &str) -> Result<u32> {
    u32::try_from(value).map_err(|_| StoreError::Damaged(format!("graph map: bad {what} {value}")))
}

pub(crate) fn to_u64(value: i64, what: &str) -> Result<u64> {
    u64::try_from(value).map_err(|_| StoreError::Damaged(format!("graph map: bad {what} {value}")))
}

/// A random non-zero `u32` that differs from `old`. The std hasher is seeded
/// from the OS per instance, which is all a generation needs.
fn fresh_generation(old: i64) -> u32 {
    loop {
        let candidate = RandomState::new().build_hasher().finish() as u32;
        if candidate != 0 && i64::from(candidate) != old {
            return candidate;
        }
    }
}

pub(crate) fn folder(conn: &Connection, folder: FolderId) -> Result<Option<GraphFolder>> {
    let row = conn
        .query_row(
            "SELECT graph_id, generation, uid_next, low_next, modseq, delta_link
             FROM graph_folders WHERE folder_row_id = ?1",
            [folder.0],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?;
    row.map(
        |(graph_id, generation, uid_next, low_next, modseq, delta_link)| {
            Ok(GraphFolder {
                graph_id,
                generation: to_u32(generation, "generation")?,
                uid_next: to_u32(uid_next, "uid_next")?,
                low_next: to_u32(low_next, "low_next")?,
                modseq: to_u64(modseq, "modseq")?,
                delta_link,
            })
        },
    )
    .transpose()
}

/// Gives the folder a new generation and an empty map. `NotFound` when the
/// folder has no map row.
pub(crate) fn reset(conn: &Connection, folder: FolderId) -> Result<u32> {
    let old: Option<i64> = conn
        .query_row(
            "SELECT generation FROM graph_folders WHERE folder_row_id = ?1",
            [folder.0],
            |row| row.get(0),
        )
        .optional()?;
    let old = old.ok_or(StoreError::NotFound)?;
    let generation = fresh_generation(old);
    conn.execute(
        "DELETE FROM graph_messages WHERE folder_row_id = ?1",
        [folder.0],
    )?;
    conn.execute(
        "UPDATE graph_folders
         SET generation = ?2, uid_next = ?3, low_next = ?4, modseq = 0, delta_link = ''
         WHERE folder_row_id = ?1",
        params![folder.0, generation, HIGH_START, LOW_START],
    )?;
    Ok(generation)
}

/// Makes `graph_id` the Graph folder of `folder`. A Graph id is one folder
/// row (the index is unique), so a map row of another folder row with the
/// same id is a folder that moved away, and it is dropped.
pub(crate) fn upsert_folder(
    conn: &Connection,
    folder: FolderId,
    graph_id: &str,
) -> Result<Upserted> {
    let existing = self::folder(conn, folder)?;
    if existing.as_ref().is_some_and(|f| f.graph_id == graph_id) {
        return Ok(Upserted::Same);
    }
    conn.execute(
        "DELETE FROM graph_folders WHERE graph_id = ?1 AND folder_row_id != ?2",
        params![graph_id, folder.0],
    )?;
    if existing.is_some() {
        conn.execute(
            "UPDATE graph_folders SET graph_id = ?2 WHERE folder_row_id = ?1",
            params![folder.0, graph_id],
        )?;
        return reset(conn, folder).map(Upserted::Changed);
    }
    let generation = fresh_generation(0);
    conn.execute(
        "INSERT INTO graph_folders
             (folder_row_id, graph_id, generation, uid_next, low_next, modseq, delta_link)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, '')",
        params![folder.0, graph_id, generation, HIGH_START, LOW_START],
    )?;
    Ok(Upserted::New(generation))
}

fn uid_of(conn: &Connection, folder: FolderId, id: &str) -> Result<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT uid FROM graph_messages WHERE folder_row_id = ?1 AND graph_id = ?2",
            params![folder.0, id],
            |row| row.get(0),
        )
        .optional()?)
}

const INSERT_ROW: &str = "INSERT INTO graph_messages
    (folder_row_id, graph_id, uid, received_at, flags, categories, modseq)
    VALUES (?1, ?2, ?3, ?4, 0, '[]', 0)";

/// Maps the ids not mapped yet to high-range uids, oldest first, writing
/// every row before returning. A mapped id keeps its uid.
pub(crate) fn allocate(
    conn: &Connection,
    folder: FolderId,
    ids: &[(String, i64)],
) -> Result<Allocation> {
    let state = self::folder(conn, folder)?.ok_or(StoreError::NotFound)?;
    let mut order: Vec<&(String, i64)> = ids.iter().collect();
    order.sort_by_key(|(_, received)| *received);
    let mut known = Vec::with_capacity(order.len());
    for (id, _) in &order {
        known.push(uid_of(conn, folder, id)?);
    }
    let fresh = known.iter().filter(|uid| uid.is_none()).count();
    let mut next = i64::from(state.uid_next);
    if next.saturating_add(i64::try_from(fresh).unwrap_or(i64::MAX)) >= HIGH_END {
        return Ok(Allocation::Exhausted);
    }
    let mut insert = conn.prepare(INSERT_ROW)?;
    let mut done = Vec::with_capacity(order.len());
    for ((id, received), known) in order.into_iter().zip(known) {
        let uid = match known {
            Some(uid) => uid,
            None => {
                insert.execute(params![folder.0, id, next, received])?;
                next += 1;
                next - 1
            }
        };
        done.push((id.clone(), to_u32(uid, "uid")?));
    }
    conn.execute(
        "UPDATE graph_folders SET uid_next = ?2 WHERE folder_row_id = ?1",
        params![folder.0, next],
    )?;
    Ok(Allocation::Done(done))
}

/// Maps one id to a low-range uid (a message found before a refresh mapped
/// it). `None` when the range has run out; the last uid is left unused so the
/// counter never reaches 0.
pub(crate) fn allocate_low(
    conn: &Connection,
    folder: FolderId,
    id: &str,
    received_ms: i64,
) -> Result<Option<u32>> {
    let state = self::folder(conn, folder)?.ok_or(StoreError::NotFound)?;
    if let Some(uid) = uid_of(conn, folder, id)? {
        return to_u32(uid, "uid").map(Some);
    }
    let uid = i64::from(state.low_next);
    if uid <= 1 {
        return Ok(None);
    }
    conn.execute(INSERT_ROW, params![folder.0, id, uid, received_ms])?;
    conn.execute(
        "UPDATE graph_folders SET low_next = ?2 WHERE folder_row_id = ?1",
        params![folder.0, uid - 1],
    )?;
    to_u32(uid, "uid").map(Some)
}

/// Stores flag and category changes and drops removed ids. One new modseq
/// goes to the rows that actually changed; the folder's counter follows.
pub(crate) fn apply(
    conn: &Connection,
    folder: FolderId,
    changes: &[MetaChange],
    removed: &[String],
) -> Result<()> {
    let state = self::folder(conn, folder)?.ok_or(StoreError::NotFound)?;
    let top: i64 = conn.query_row(
        "SELECT coalesce(max(modseq), 0) FROM graph_messages WHERE folder_row_id = ?1",
        [folder.0],
        |row| row.get(0),
    )?;
    let modseq = top.max(i64::try_from(state.modseq).unwrap_or(i64::MAX)) + 1;
    let mut update = conn.prepare(
        "UPDATE graph_messages SET flags = ?3, categories = ?4, modseq = ?5
         WHERE folder_row_id = ?1 AND graph_id = ?2 AND (flags != ?3 OR categories != ?4)",
    )?;
    let mut any = false;
    for change in changes {
        let categories = serde_json::to_string(&change.categories)
            .map_err(|e| StoreError::Damaged(format!("graph map categories: {e}")))?;
        let n = update.execute(params![
            folder.0,
            change.graph_id,
            change.flags,
            categories,
            modseq
        ])?;
        any |= n > 0;
    }
    let mut delete =
        conn.prepare("DELETE FROM graph_messages WHERE folder_row_id = ?1 AND graph_id = ?2")?;
    for id in removed {
        delete.execute(params![folder.0, id])?;
    }
    if any {
        conn.execute(
            "UPDATE graph_folders SET modseq = ?2 WHERE folder_row_id = ?1",
            params![folder.0, modseq],
        )?;
    }
    Ok(())
}

/// Saves the delta link and modseq of a finished sync.
pub(crate) fn commit(
    conn: &Connection,
    folder: FolderId,
    delta_link: &str,
    modseq: u64,
) -> Result<()> {
    let modseq = i64::try_from(modseq)
        .map_err(|_| StoreError::Damaged(format!("graph map: modseq {modseq} too large")))?;
    found(conn.execute(
        "UPDATE graph_folders SET delta_link = ?2, modseq = ?3 WHERE folder_row_id = ?1",
        params![folder.0, delta_link, modseq],
    )?)
}

impl Db {
    /// New generation and an empty map for the folder (see `GraphMap::reset`).
    pub fn graph_reset(&self, folder: FolderId) -> Result<u32> {
        self.savepoint(|| reset(self.conn(), folder))
    }

    /// Records the Graph folder id of a folder row; see `Upserted`.
    pub fn graph_upsert_folder(&self, folder: FolderId, graph_id: &str) -> Result<Upserted> {
        self.savepoint(|| upsert_folder(self.conn(), folder, graph_id))
    }

    /// Saves the delta link and modseq of a finished sync.
    pub fn graph_commit(&self, folder: FolderId, delta_link: &str, modseq: u64) -> Result<()> {
        commit(self.conn(), folder, delta_link, modseq)
    }
}
