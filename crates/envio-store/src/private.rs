//! Mail on disk is for the user alone: new folders are 0700, new files 0600,
//! and an older install is narrowed to match.

use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

/// Creates `dir` and any missing parents with mode 0700. Folders that
/// already exist keep their mode (see `tighten_tree`).
pub fn create_private_dir(dir: &Path) -> io::Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(dir)
}

/// Opens `path` for appending, creating it with mode 0600.
pub(crate) fn open_private_append(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
}

/// Creates `path` empty with mode 0600 unless it exists (its content is
/// kept; a mode that is too open is narrowed). SQLite gives the `-wal`,
/// `-shm` and `-journal` files the mode of the database file, so making the
/// database private first makes them private too.
pub fn create_private_file(path: &Path) -> io::Result<()> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?;
    narrow(path)
}

/// Drops group and other access to `path` (a file to 0600, a folder to 0700)
/// when it has any; a missing path or a symlink is left alone.
fn narrow(path: &Path) -> io::Result<()> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if meta.file_type().is_symlink() {
        return Ok(());
    }
    let mode = meta.permissions().mode();
    if mode & 0o077 != 0 {
        fs::set_permissions(path, fs::Permissions::from_mode(mode & !0o077))?;
    }
    Ok(())
}

/// Narrows `root` and everything below it, so an install made before files
/// were private is tightened at its next start: access for group and other
/// is removed, owner bits stay. Symlinks are not followed. A missing `root`
/// is fine.
pub fn tighten_tree(root: &Path) -> io::Result<()> {
    narrow(root)?;
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            tighten_tree(&path)?;
        } else {
            narrow(&path)?;
        }
    }
    Ok(())
}
