//! The OAuth token file: its own 0600 file, never in the cache.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use envio_core::oauth_tokens::{FILE_NAME, OAuthTokens};
use secrecy::{ExposeSecret, SecretString};

fn rt(s: &str) -> SecretString {
    SecretString::from(s.to_owned())
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn a_refresh_token_round_trips_through_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut tokens = OAuthTokens::open(dir.path()).unwrap();
    tokens.set("a1", rt("refresh-token-1")).unwrap();
    let again = OAuthTokens::open(dir.path()).unwrap();
    assert_eq!(again.get("a1").unwrap().expose_secret(), "refresh-token-1");
    assert!(again.get("nope").is_none());
}

#[test]
fn the_file_is_created_0600() {
    let dir = tempfile::tempdir().unwrap();
    let mut tokens = OAuthTokens::open(dir.path()).unwrap();
    tokens.set("a1", rt("x")).unwrap();
    assert_eq!(mode(&dir.path().join(FILE_NAME)), 0o600);
}

#[test]
fn a_wider_mode_is_narrowed_on_open() {
    let dir = tempfile::tempdir().unwrap();
    let mut tokens = OAuthTokens::open(dir.path()).unwrap();
    tokens.set("a1", rt("x")).unwrap();
    let path = dir.path().join(FILE_NAME);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let again = OAuthTokens::open(dir.path()).unwrap();
    assert_eq!(mode(&path), 0o600);
    assert_eq!(again.get("a1").unwrap().expose_secret(), "x");
}

#[test]
fn a_failed_write_leaves_the_old_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut tokens = OAuthTokens::open(dir.path()).unwrap();
    tokens.set("a1", rt("old")).unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();
    let probe = dir.path().join("probe");
    let as_root = fs::write(&probe, b"x").is_ok();
    if as_root {
        fs::remove_file(&probe).unwrap();
    }
    let result = tokens.set("a1", rt("new"));
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    if as_root {
        eprintln!("skipped: running as root, read-only has no effect");
        return;
    }
    assert!(result.is_err());
    // The in-memory value is the old one too, not just the file.
    assert_eq!(tokens.get("a1").unwrap().expose_secret(), "old");
    let again = OAuthTokens::open(dir.path()).unwrap();
    assert_eq!(again.get("a1").unwrap().expose_secret(), "old");
}

#[test]
fn a_damaged_file_is_moved_aside_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE_NAME);
    fs::write(&path, b"{ not json").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let tokens = OAuthTokens::open(dir.path()).unwrap();
    assert!(tokens.get("a1").is_none());
    assert!(!path.exists());
    assert_eq!(asides(dir.path()), [b"{ not json".to_vec()]);
}

fn asides(dir: &Path) -> Vec<Vec<u8>> {
    let mut found: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(&format!("{FILE_NAME}.damaged"))
        })
        .map(|e| fs::read(e.path()).unwrap())
        .collect();
    found.sort();
    found
}

#[test]
fn a_second_damaged_file_does_not_replace_the_first() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE_NAME);
    for text in [&b"first damage"[..], b"second damage"] {
        fs::write(&path, text).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        OAuthTokens::open(dir.path()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        asides(dir.path()),
        [b"first damage".to_vec(), b"second damage".to_vec()]
    );
}

#[test]
fn a_file_from_a_newer_envio_is_refused_and_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE_NAME);
    let text = br#"{"version": 2, "tokens": {"a1": {"secret": "x"}}}"#;
    fs::write(&path, text).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(OAuthTokens::open(dir.path()).is_err());
    assert_eq!(fs::read(&path).unwrap(), text);
    assert!(asides(dir.path()).is_empty());
}

#[test]
fn debug_never_shows_a_token() {
    let dir = tempfile::tempdir().unwrap();
    let mut tokens = OAuthTokens::open(dir.path()).unwrap();
    tokens.set("a1", rt("refresh-token-1")).unwrap();
    let text = format!("{tokens:?}");
    assert!(text.contains("a1"));
    assert!(!text.contains("refresh-token-1"));
}

#[test]
fn remove_deletes_only_that_account() {
    let dir = tempfile::tempdir().unwrap();
    let mut tokens = OAuthTokens::open(dir.path()).unwrap();
    tokens.set("a1", rt("one")).unwrap();
    tokens.set("a2", rt("two")).unwrap();
    tokens.remove("a1").unwrap();
    let again = OAuthTokens::open(dir.path()).unwrap();
    assert!(again.get("a1").is_none());
    assert_eq!(again.get("a2").unwrap().expose_secret(), "two");
}

#[test]
fn remove_is_idempotent_and_writes_nothing_for_an_unknown_account() {
    let dir = tempfile::tempdir().unwrap();
    let mut tokens = OAuthTokens::open(dir.path()).unwrap();
    tokens.remove("a1").unwrap();
    assert!(!dir.path().join(FILE_NAME).exists());
    tokens.set("a1", rt("one")).unwrap();
    tokens.remove("a1").unwrap();
    tokens.remove("a1").unwrap();
    assert!(OAuthTokens::open(dir.path()).unwrap().get("a1").is_none());
}

#[test]
fn a_newer_file_error_names_the_version_and_no_token() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE_NAME);
    fs::write(&path, br#"{"version": 9, "tokens": {"a1": "secret-rt"}}"#).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let error = OAuthTokens::open(dir.path()).unwrap_err();
    let text = format!("{error} {error:?}");
    assert!(text.contains('9'), "{text}");
    assert!(!text.contains("secret-rt"), "{text}");
}

#[test]
fn the_file_is_not_the_passwords_file() {
    assert_eq!(FILE_NAME, "oauth.json");
    assert_ne!(FILE_NAME, envio_core::secrets::FILE_NAME);
}
