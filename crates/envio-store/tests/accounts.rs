use std::fs;

use envio_store::{AccountSettings, Accounts, Server, StoreError};

fn settings(email: &str) -> AccountSettings {
    AccountSettings {
        name: "Ada".into(),
        email: email.into(),
        username: email.into(),
        imap: Server {
            host: "imap.example.org".into(),
            port: 993,
        },
        smtp: Server {
            host: "smtp.example.org".into(),
            port: 465,
        },
    }
}

#[test]
fn a_missing_file_is_no_accounts() {
    let dir = tempfile::tempdir().unwrap();
    let accounts = Accounts::open(&dir.path().join("accounts.json")).unwrap();
    assert!(accounts.list().is_empty());
}

#[test]
fn accounts_survive_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    let mut accounts = Accounts::open(&path).unwrap();
    let first = accounts.add(settings("ada@example.org"), 1_000).unwrap();
    let second = accounts.add(settings("bob@example.org"), 1_000).unwrap();
    assert_ne!(first, second);
    let reopened = Accounts::open(&path).unwrap();
    assert_eq!(reopened.list(), accounts.list());
    assert_eq!(
        reopened.get(&first).unwrap().settings.email,
        "ada@example.org"
    );
}

#[test]
fn update_and_remove_are_saved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    let mut accounts = Accounts::open(&path).unwrap();
    let ada = accounts.add(settings("ada@example.org"), 1).unwrap();
    let bob = accounts.add(settings("bob@example.org"), 2).unwrap();
    let mut changed = settings("ada@example.org");
    changed.name = "Ada L.".into();
    accounts.update(&ada, changed).unwrap();
    accounts.remove(&bob).unwrap();
    let reopened = Accounts::open(&path).unwrap();
    assert_eq!(reopened.list().len(), 1);
    assert_eq!(reopened.get(&ada).unwrap().settings.name, "Ada L.");
    assert!(reopened.get(&bob).is_none());
}

#[test]
fn unknown_keys_are_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let mut accounts = Accounts::open(&dir.path().join("accounts.json")).unwrap();
    assert!(matches!(
        accounts.update("nope", settings("a@b")),
        Err(StoreError::NotFound)
    ));
    assert!(matches!(accounts.remove("nope"), Err(StoreError::NotFound)));
}

#[test]
fn a_damaged_file_is_an_error_and_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    fs::write(&path, b"{ not json").unwrap();
    let error = Accounts::open(&path).err().unwrap();
    assert!(matches!(error, StoreError::BadRecord { .. }), "{error:?}");
    assert_eq!(fs::read(&path).unwrap(), b"{ not json");
}

#[test]
fn a_file_from_a_newer_envio_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    fs::write(&path, br#"{"version": 2, "accounts": []}"#).unwrap();
    let error = Accounts::open(&path).err().unwrap();
    assert!(matches!(error, StoreError::BadRecord { .. }), "{error:?}");
}

#[test]
fn a_failed_save_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    let mut accounts = Accounts::open(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(accounts.add(settings("ada@example.org"), 1).is_err());
    assert!(accounts.list().is_empty());
}

#[test]
fn a_key_that_is_not_a_key_is_a_bad_record() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    let mut accounts = Accounts::open(&path).unwrap();
    let key = accounts.add(settings("ada@example.org"), 1_000).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text.replace(&format!("\"{key}\""), "\"../x\"")).unwrap();
    assert!(matches!(
        Accounts::open(&path),
        Err(StoreError::BadRecord { .. })
    ));
}

#[test]
fn two_accounts_with_one_key_are_a_bad_record() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    let mut accounts = Accounts::open(&path).unwrap();
    let first = accounts.add(settings("ada@example.org"), 1_000).unwrap();
    let second = accounts.add(settings("bob@example.org"), 2_000).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        text.replace(&format!("\"{second}\""), &format!("\"{first}\"")),
    )
    .unwrap();
    assert!(matches!(
        Accounts::open(&path),
        Err(StoreError::BadRecord { .. })
    ));
}
