//! Message bodies as raw `.eml` files in the cache folder,
//! `<dir>/<folder id>/<uid>.eml`, and the queries that decide which to keep.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::atomic::{is_temp, write_atomic};
use crate::db::Db;
use crate::error::{Result, StoreError};
use crate::folders::FolderId;

pub struct Bodies {
    dir: PathBuf,
}

impl Bodies {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn folder_dir(&self, folder: FolderId) -> PathBuf {
        self.dir.join(folder.0.to_string())
    }

    fn path(&self, folder: FolderId, uid: u32) -> PathBuf {
        self.folder_dir(folder).join(format!("{uid}.eml"))
    }

    pub fn write(&self, folder: FolderId, uid: u32, eml: &[u8]) -> Result<()> {
        Ok(write_atomic(&self.path(folder, uid), eml)?)
    }

    pub fn read(&self, folder: FolderId, uid: u32) -> Result<Vec<u8>> {
        fs::read(self.path(folder, uid)).map_err(StoreError::missing_or_io)
    }

    /// Drops a folder's bodies (after a reset or removal). Missing is fine.
    pub fn remove_folder(&self, folder: FolderId) -> Result<()> {
        match fs::remove_dir_all(self.folder_dir(folder)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => Ok(result?),
        }
    }

    /// Deletes every body not in `keep`, and interrupted writes. Files that
    /// aren't ours are left alone. Returns how many files were deleted.
    /// Run after `Db::mark_bodies_pruned`, with `Db::bodies_to_keep`.
    pub fn retain(&self, keep: &HashSet<(FolderId, u32)>) -> Result<usize> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error.into()),
        };
        let mut removed = 0;
        for entry in entries {
            let entry = entry?;
            let folder = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse().ok());
            if let Some(folder) = folder.map(FolderId)
                && entry.file_type()?.is_dir()
            {
                removed += retain_in(&entry.path(), folder, keep)?;
            }
        }
        Ok(removed)
    }
}

fn retain_in(dir: &Path, folder: FolderId, keep: &HashSet<(FolderId, u32)>) -> Result<usize> {
    let mut removed = 0;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| is_stale(name, folder, keep))
        {
            fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    Ok(removed)
}

fn is_stale(name: &str, folder: FolderId, keep: &HashSet<(FolderId, u32)>) -> bool {
    if is_temp(name) {
        return true;
    }
    name.strip_suffix(".eml")
        .and_then(|uid| uid.parse::<u32>().ok())
        .is_some_and(|uid| !keep.contains(&(folder, uid)))
}

impl Db {
    /// Bodies to keep on disk: downloaded, dated at or after `cutoff` (unix seconds).
    pub fn bodies_to_keep(&self, cutoff: i64) -> Result<HashSet<(FolderId, u32)>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT folder_id, uid FROM messages WHERE body_state = 1 AND date >= ?1")?;
        let keep = stmt
            .query_map([cutoff], |row| Ok((FolderId(row.get(0)?), row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(keep)
    }

    /// Marks bodies dated before `cutoff` as missing; returns how many.
    pub fn mark_bodies_pruned(&self, cutoff: i64) -> Result<usize> {
        Ok(self.conn().execute(
            "UPDATE messages SET body_state = 0 WHERE body_state = 1 AND date < ?1",
            [cutoff],
        )?)
    }
}
