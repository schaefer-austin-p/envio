//! The passwords file: its own 0600 file, never in the cache.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use envio_core::secrets::{FILE_NAME, Secrets};
use secrecy::{ExposeSecret, SecretString};

fn pw(s: &str) -> SecretString {
    SecretString::from(s.to_owned())
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn a_password_round_trips_through_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut secrets = Secrets::open(dir.path()).unwrap();
    secrets.set("a1", pw("hunter2")).unwrap();
    let again = Secrets::open(dir.path()).unwrap();
    assert_eq!(again.get("a1").unwrap().expose_secret(), "hunter2");
    assert!(again.get("nope").is_none());
}

#[test]
fn the_file_is_created_0600() {
    let dir = tempfile::tempdir().unwrap();
    let mut secrets = Secrets::open(dir.path()).unwrap();
    secrets.set("a1", pw("x")).unwrap();
    assert_eq!(mode(&dir.path().join(FILE_NAME)), 0o600);
}

#[test]
fn a_wider_mode_is_narrowed_on_open() {
    let dir = tempfile::tempdir().unwrap();
    let mut secrets = Secrets::open(dir.path()).unwrap();
    secrets.set("a1", pw("x")).unwrap();
    let path = dir.path().join(FILE_NAME);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let again = Secrets::open(dir.path()).unwrap();
    assert_eq!(mode(&path), 0o600);
    assert_eq!(again.get("a1").unwrap().expose_secret(), "x");
}

#[test]
fn a_failed_write_leaves_the_old_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut secrets = Secrets::open(dir.path()).unwrap();
    secrets.set("a1", pw("old")).unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();
    let probe = dir.path().join("probe");
    let as_root = fs::write(&probe, b"x").is_ok();
    if as_root {
        fs::remove_file(&probe).unwrap();
    }
    let result = secrets.set("a1", pw("new"));
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    if as_root {
        eprintln!("skipped: running as root, read-only has no effect");
        return;
    }
    assert!(result.is_err());
    let again = Secrets::open(dir.path()).unwrap();
    assert_eq!(again.get("a1").unwrap().expose_secret(), "old");
}

#[test]
fn a_damaged_file_is_moved_aside_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE_NAME);
    fs::write(&path, b"{ not json").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let secrets = Secrets::open(dir.path()).unwrap();
    assert!(secrets.accounts().is_empty());
    assert!(!path.exists());
    let aside = dir.path().join(format!("{FILE_NAME}.damaged"));
    assert_eq!(fs::read(aside).unwrap(), b"{ not json");
}

#[test]
fn a_file_from_a_newer_envio_is_refused_and_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE_NAME);
    let text = br#"{"version": 2, "passwords": {"a1": {"secret": "x"}}}"#;
    fs::write(&path, text).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(Secrets::open(dir.path()).is_err());
    assert_eq!(fs::read(&path).unwrap(), text);
    assert!(!dir.path().join(format!("{FILE_NAME}.damaged")).exists());
}

#[test]
fn debug_never_shows_a_password() {
    let dir = tempfile::tempdir().unwrap();
    let mut secrets = Secrets::open(dir.path()).unwrap();
    secrets.set("a1", pw("hunter2")).unwrap();
    let text = format!("{secrets:?}");
    assert!(text.contains("a1"));
    assert!(!text.contains("hunter2"));
}

#[test]
fn remove_deletes_only_that_account() {
    let dir = tempfile::tempdir().unwrap();
    let mut secrets = Secrets::open(dir.path()).unwrap();
    secrets.set("a1", pw("one")).unwrap();
    secrets.set("a2", pw("two")).unwrap();
    secrets.remove("a1").unwrap();
    let again = Secrets::open(dir.path()).unwrap();
    assert!(again.get("a1").is_none());
    assert_eq!(again.get("a2").unwrap().expose_secret(), "two");
    assert_eq!(again.accounts(), vec!["a2".to_owned()]);
}
