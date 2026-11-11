//! Opening the cache at startup, and rebuilding it when the file is damaged.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::Path;

use crate::db::Db;
use crate::error::Result;
use crate::paths::StorePaths;

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

/// Opens the cache database, rebuilding it if it's damaged; a rebuild also
/// clears the bodies folder, since folder ids restart. Unsent work is in the
/// data folder and is never touched here. When core finds damage mid-session
/// (`StoreError::is_damage`), it drops the `Db` and calls this again.
pub fn open_or_rebuild(paths: &StorePaths) -> Result<(Db, Opened)> {
    let path = &paths.db();
    // A zero-byte file (a crash right after creation) is a new cache.
    let existed = fs::metadata(path).is_ok_and(|m| m.len() > 0);
    let error = match open_checked(path) {
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
    // The handle that found the damage is already closed (`open_checked`
    // drops it), so if anything below fails nothing holds, or can write to,
    // the moved-aside file (#50): the error goes to the caller, who opens
    // nothing until it calls again.
    move_aside(path)?;
    match fs::remove_dir_all(paths.bodies()) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    let db = Db::open(path)?;
    Ok((
        db,
        Opened::Rebuilt {
            reason: error.to_string(),
        },
    ))
}

/// Opens and checks; a database that fails the check is closed before the
/// error comes back.
fn open_checked(path: &Path) -> Result<Db> {
    let db = Db::open(path)?;
    match db.check() {
        Ok(()) => Ok(db),
        Err(error) => {
            drop(db);
            Err(error)
        }
    }
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
