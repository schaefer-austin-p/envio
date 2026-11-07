//! Replacing a file so a crash leaves the old content or the new, never half.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

/// Writes `bytes` to a temp file beside `path`, syncs it, renames it over
/// `path` and syncs the folder. Creates the folder if needed.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "no file name"));
    };
    fs::create_dir_all(dir)?;
    let temp = dir.join(format!(".{}.tmp", name.to_string_lossy()));
    let mut file = File::create(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temp, path)?;
    File::open(dir)?.sync_all()
}

/// A temp file `write_atomic` left behind when it was interrupted.
pub(crate) fn is_temp(name: &str) -> bool {
    name.starts_with('.') && name.ends_with(".tmp")
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn temp_names_are_recognised() {
        assert!(is_temp(".a.json.tmp"));
        assert!(!is_temp("a.json"));
        assert!(!is_temp("a.tmp"));
    }
}
