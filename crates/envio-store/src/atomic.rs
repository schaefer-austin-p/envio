//! Replacing a file so a crash leaves the old content or the new, never half.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::private::create_private_dir;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Writes `bytes` to a temp file beside `path`, syncs it, renames it over
/// `path` and syncs the folder. Creates the folder if needed.
///
/// An `Err` from the final folder sync means the new content may already be
/// in place; only the durability of the rename is unconfirmed.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "no file name"));
    };
    create_private_dir(dir)?;
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = dir.join(format!(
        ".{}.{}-{n}.tmp",
        name.to_string_lossy(),
        std::process::id()
    ));
    let written = write_and_rename(&temp, path, bytes);
    if written.is_err() {
        let _ = fs::remove_file(&temp);
    }
    written?;
    File::open(dir)?.sync_all()
}

fn write_and_rename(temp: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temp, path)
}

/// A temp file `write_atomic` left behind when it was interrupted.
pub(crate) fn is_temp(name: &str) -> bool {
    name.starts_with('.') && name.ends_with(".tmp")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::private::open_private_append;

    #[test]
    fn writes_and_replaces_without_leaving_a_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("a.json");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        let names: Vec<_> = fs::read_dir(dir.path().join("sub")).unwrap().collect();
        assert_eq!(names.len(), 1);
    }

    #[test]
    fn a_failed_rename_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("a.json");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("inside"), b"x").unwrap();
        assert!(write_atomic(&target, b"data").is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn files_and_new_folders_are_private() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a").join("b").join("a.json");
        write_atomic(&path, b"x").unwrap();
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().unwrap()), 0o700);
        assert_eq!(mode(path.parent().unwrap().parent().unwrap()), 0o700);
        let log = dir.path().join("a").join("log");
        drop(open_private_append(&log).unwrap());
        assert_eq!(mode(&log), 0o600);
    }

    #[test]
    fn temp_names_are_recognised() {
        assert!(is_temp(".a.json.tmp"));
        assert!(is_temp(".a.json.42-7.tmp"));
        assert!(!is_temp("a.json"));
        assert!(!is_temp("a.tmp"));
    }
}
