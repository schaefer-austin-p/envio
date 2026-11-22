//! The OAuth token file: its own 0600 file, never in the cache.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use envio_core::oauth_tokens::{FILE_NAME, OAuthTokens};
use envio_net::oauth::ProviderKind;
use secrecy::{ExposeSecret, SecretString};

const IMAP: ProviderKind = ProviderKind::Microsoft;
const GRAPH: ProviderKind = ProviderKind::MicrosoftGraph;

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
    tokens.set("a1", IMAP, rt("refresh-token-1")).unwrap();
    let again = OAuthTokens::open(dir.path()).unwrap();
    assert_eq!(
        again.get("a1", IMAP).unwrap().expose_secret(),
        "refresh-token-1"
    );
    assert!(again.get("nope", IMAP).is_none());
}

#[test]
fn the_file_is_created_0600() {
    let dir = tempfile::tempdir().unwrap();
    let mut tokens = OAuthTokens::open(dir.path()).unwrap();
    tokens.set("a1", IMAP, rt("x")).unwrap();
    assert_eq!(mode(&dir.path().join(FILE_NAME)), 0o600);
}

#[test]
fn a_wider_mode_is_narrowed_on_open() {
    let dir = tempfile::tempdir().unwrap();
    let mut tokens = OAuthTokens::open(dir.path()).unwrap();
    tokens.set("a1", IMAP, rt("x")).unwrap();
    let path = dir.path().join(FILE_NAME);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let again = OAuthTokens::open(dir.path()).unwrap();
    assert_eq!(mode(&path), 0o600);
    assert_eq!(again.get("a1", IMAP).unwrap().expose_secret(), "x");
}

#[test]
fn a_failed_write_leaves_the_old_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE_NAME);
    let mut tokens = OAuthTokens::open(dir.path()).unwrap();
    tokens.set("a1", IMAP, rt("old")).unwrap();
    // The rename onto a directory fails for root too, unlike a read-only
    // folder. The real file waits aside meanwhile.
    let keep = dir.path().join("kept");
    fs::rename(&path, &keep).unwrap();
    fs::create_dir(&path).unwrap();
    let result = tokens.set("a1", IMAP, rt("new"));
    assert!(result.is_err());
    // The in-memory value is the old one too, not just the file.
    assert_eq!(tokens.get("a1", IMAP).unwrap().expose_secret(), "old");
    // No temp file with the new token is left behind.
    let names: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(names.iter().all(|n| !n.ends_with(".tmp")), "{names:?}");
    fs::remove_dir(&path).unwrap();
    fs::rename(&keep, &path).unwrap();
    let again = OAuthTokens::open(dir.path()).unwrap();
    assert_eq!(again.get("a1", IMAP).unwrap().expose_secret(), "old");
}

#[test]
fn a_damaged_file_is_moved_aside_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE_NAME);
    fs::write(&path, b"{ not json").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let tokens = OAuthTokens::open(dir.path()).unwrap();
    assert!(tokens.get("a1", IMAP).is_none());
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
    let text = br#"{"version": 3, "tokens": {"a1": {"secret": "x"}}}"#;
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
    tokens.set("a1", IMAP, rt("refresh-token-1")).unwrap();
    let text = format!("{tokens:?}");
    assert!(text.contains("a1"));
    assert!(!text.contains("refresh-token-1"));
}

#[test]
fn remove_deletes_only_that_account() {
    let dir = tempfile::tempdir().unwrap();
    let mut tokens = OAuthTokens::open(dir.path()).unwrap();
    tokens.set("a1", IMAP, rt("one")).unwrap();
    tokens.set("a2", IMAP, rt("two")).unwrap();
    tokens.remove("a1", IMAP).unwrap();
    let again = OAuthTokens::open(dir.path()).unwrap();
    assert!(again.get("a1", IMAP).is_none());
    assert_eq!(again.get("a2", IMAP).unwrap().expose_secret(), "two");
}

#[test]
fn remove_is_idempotent_and_writes_nothing_for_an_unknown_account() {
    let dir = tempfile::tempdir().unwrap();
    let mut tokens = OAuthTokens::open(dir.path()).unwrap();
    tokens.remove("a1", IMAP).unwrap();
    assert!(!dir.path().join(FILE_NAME).exists());
    tokens.set("a1", IMAP, rt("one")).unwrap();
    tokens.remove("a1", IMAP).unwrap();
    tokens.remove("a1", IMAP).unwrap();
    assert!(
        OAuthTokens::open(dir.path())
            .unwrap()
            .get("a1", IMAP)
            .is_none()
    );
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

fn file(dir: &Path) -> String {
    fs::read_to_string(dir.join(FILE_NAME)).unwrap()
}

fn write_v1(dir: &Path) -> String {
    // A v1 file exactly as an older build writes it.
    let text = r#"{"version":1,"tokens":{"a1":"old-refresh","a2":"other"}}"#;
    let path = dir.join(FILE_NAME);
    fs::write(&path, text).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    text.to_owned()
}

#[test]
fn a_v1_file_loads_every_token_as_imap_and_stays_v1_byte_equal() {
    let dir = tempfile::tempdir().unwrap();
    let text = write_v1(dir.path());
    let mut tokens = OAuthTokens::open(dir.path()).unwrap();
    assert_eq!(
        tokens.get("a1", IMAP).unwrap().expose_secret(),
        "old-refresh"
    );
    assert!(tokens.get("a1", GRAPH).is_none());
    // Rewritten with only imap tokens: still v1, same bytes.
    tokens.set("a1", IMAP, rt("old-refresh")).unwrap();
    assert_eq!(file(dir.path()), text);
}

#[test]
fn the_first_graph_token_upgrades_the_file_to_v2() {
    let dir = tempfile::tempdir().unwrap();
    write_v1(dir.path());
    let mut tokens = OAuthTokens::open(dir.path()).unwrap();
    tokens.set("a1", GRAPH, rt("g-refresh")).unwrap();
    let value: serde_json::Value = serde_json::from_str(&file(dir.path())).unwrap();
    assert_eq!(value["version"], 2);
    assert_eq!(value["tokens"]["a1"]["imap"], "old-refresh");
    assert_eq!(value["tokens"]["a1"]["graph"], "g-refresh");
    assert_eq!(value["tokens"]["a2"]["imap"], "other");
    let again = OAuthTokens::open(dir.path()).unwrap();
    assert_eq!(again.get("a1", GRAPH).unwrap().expose_secret(), "g-refresh");
    assert_eq!(
        again.get("a1", IMAP).unwrap().expose_secret(),
        "old-refresh"
    );
}

#[test]
fn a_file_holding_only_imap_tokens_is_written_as_v1_again() {
    let dir = tempfile::tempdir().unwrap();
    let mut tokens = OAuthTokens::open(dir.path()).unwrap();
    tokens.set("a1", IMAP, rt("one")).unwrap();
    tokens.set("a1", GRAPH, rt("two")).unwrap();
    assert!(file(dir.path()).contains("\"version\":2"));
    // Dropping the Graph token lets an older build read the file again.
    tokens.remove("a1", GRAPH).unwrap();
    assert_eq!(file(dir.path()), r#"{"version":1,"tokens":{"a1":"one"}}"#);
}

#[test]
fn a_token_of_one_kind_is_never_read_as_the_other() {
    let dir = tempfile::tempdir().unwrap();
    let mut tokens = OAuthTokens::open(dir.path()).unwrap();
    tokens.set("a1", GRAPH, rt("g")).unwrap();
    assert!(tokens.get("a1", IMAP).is_none());
    tokens.set("a2", IMAP, rt("i")).unwrap();
    assert!(tokens.get("a2", GRAPH).is_none());
    let again = OAuthTokens::open(dir.path()).unwrap();
    assert!(again.get("a1", IMAP).is_none() && again.get("a2", GRAPH).is_none());
}

#[test]
fn a_v3_file_is_newer_and_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE_NAME);
    let text = br#"{"version":3,"tokens":{"a1":{"imap":"x"}}}"#;
    fs::write(&path, text).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(matches!(
        OAuthTokens::open(dir.path()),
        Err(envio_core::oauth_tokens::TokensError::Newer(3))
    ));
    assert_eq!(fs::read(&path).unwrap(), text);
}

#[test]
fn remove_account_drops_both_kinds() {
    let dir = tempfile::tempdir().unwrap();
    let mut tokens = OAuthTokens::open(dir.path()).unwrap();
    tokens.set("a1", IMAP, rt("one")).unwrap();
    tokens.set("a1", GRAPH, rt("two")).unwrap();
    tokens.set("a2", GRAPH, rt("three")).unwrap();
    tokens.remove_account("a1").unwrap();
    let again = OAuthTokens::open(dir.path()).unwrap();
    assert!(again.get("a1", IMAP).is_none() && again.get("a1", GRAPH).is_none());
    assert_eq!(again.get("a2", GRAPH).unwrap().expose_secret(), "three");
}

#[test]
fn debug_names_no_graph_token() {
    let dir = tempfile::tempdir().unwrap();
    let mut tokens = OAuthTokens::open(dir.path()).unwrap();
    tokens.set("a1", GRAPH, rt("graph-secret")).unwrap();
    assert!(!format!("{tokens:?}").contains("graph-secret"));
}
