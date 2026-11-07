//! Opening the cache at startup, and rebuilding it when the file is damaged.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::Path;

use crate::db::Db;
use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opened {
    Created,
    Existing,
    /// The old file was damaged (or from a newer envio) and is now
    /// `<name>.damaged`; sync refills the new cache from the server.
    Rebuilt {
        reason: String,
    },
}

/// Opens the cache at `path`, rebuilding it if it's damaged. Unsent work is
/// in other files and is never touched here.
pub fn open_or_rebuild(path: &Path) -> Result<(Db, Opened)> {
    let existed = path.exists();
    let error = match Db::open(path).and_then(|db| db.check().map(|()| db)) {
        Ok(db) => {
            let opened = if existed {
                Opened::Existing
            } else {
                Opened::Created
            };
            return Ok((db, opened));
        }
        Err(error) if error.is_damage() => error,
        Err(error) => return Err(error),
    };
    move_aside(path)?;
    let db = Db::open(path)?;
    Ok((
        db,
        Opened::Rebuilt {
            reason: error.to_string(),
        },
    ))
}

fn with_suffix(path: &Path, suffix: &str) -> OsString {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    name
}

/// Keeps the damaged file for a bug report and drops its WAL and shared
/// memory files, which belong to it and would confuse the new database.
fn move_aside(path: &Path) -> Result<()> {
    fs::rename(path, with_suffix(path, ".damaged"))?;
    for suffix in ["-wal", "-shm"] {
        match fs::remove_file(with_suffix(path, suffix)) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
    }
    Ok(())
}
