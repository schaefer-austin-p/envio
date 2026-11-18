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
/// were private is tightened: access for group and other is removed, owner
/// bits stay. Symlinks are not followed. A missing `root` is fine.
///
/// An entry that cannot be read or changed (owned by someone else, gone
/// meanwhile) does not stop the walk: everything else is still narrowed, and
/// the first error is returned at the end.
pub fn tighten_tree(root: &Path) -> io::Result<()> {
    let mut first = None;
    walk(root, &mut first);
    first.map_or(Ok(()), Err)
}

fn walk(path: &Path, first: &mut Option<io::Error>) {
    let mut note = |result: io::Result<()>| {
        if let Err(error) = result {
            first.get_or_insert(error);
        }
    };
    note(narrow(path));
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            return;
        }
        Err(error) => return note(Err(error)),
    };
    let mut subfolders = Vec::new();
    for entry in entries {
        match entry.and_then(|entry| Ok((entry.path(), entry.file_type()?))) {
            Ok((path, kind)) if kind.is_dir() => subfolders.push(path),
            Ok((path, _)) => note(narrow(&path)),
            Err(error) => note(Err(error)),
        }
    }
    for folder in subfolders {
        walk(&folder, first);
    }
}

/// Marks a root whose existing files `tighten_tree` has narrowed.
const MARKER: &str = ".private-v1";

/// `tighten_tree` as a one-time migration: once a whole walk of `root` has
/// succeeded a private marker file is written, and later calls return at
/// once (files made since are created private). A walk with errors leaves no
/// marker, so the next start tries again.
pub fn tighten_once(root: &Path) -> io::Result<()> {
    let marker = root.join(MARKER);
    if marker.exists() {
        return Ok(());
    }
    tighten_tree(root)?;
    create_private_file(&marker)
}
