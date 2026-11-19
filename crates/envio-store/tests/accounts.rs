use std::fs;

use envio_store::{AccountSettings, Accounts, Security, Server, StoreError};

const PIN: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn settings(email: &str) -> AccountSettings {
    AccountSettings {
        name: "Ada".into(),
        email: email.into(),
        username: email.into(),
        imap: Server {
            host: "imap.example.org".into(),
            port: 993,
            security: Security::Tls,
            pin: None,
        },
        smtp: Server {
            host: "smtp.example.org".into(),
            port: 465,
            security: Security::Tls,
            pin: None,
        },
        auth: None,
    }
}

#[test]
fn tls_mode_and_pins_survive_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    let mut accounts = Accounts::open(&path).unwrap();
    let mut pinned = settings("ada@example.org");
    pinned.imap.pin = Some(PIN.into());
    pinned.smtp.security = Security::StartTls;
    pinned.smtp.port = 587;
    let key = accounts.add(pinned.clone(), 1_000).unwrap();
    let reopened = Accounts::open(&path).unwrap();
    assert_eq!(reopened.get(&key).unwrap().settings, pinned);
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"starttls\""), "{text}");
    assert!(text.contains("\"version\": 2"), "{text}");
}

#[test]
fn a_version_1_file_reads_as_implicit_tls_without_pins() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    let v1 = r#"{"version": 1, "accounts": [{"key": "1000",
      "settings": {"name": "Ada", "email": "ada@example.org",
      "username": "ada", "imap": {"host": "imap.example.org", "port": 993},
      "smtp": {"host": "smtp.example.org", "port": 465}}}]}"#;
    fs::write(&path, v1).unwrap();
    let accounts = Accounts::open(&path).unwrap();
    let imap = &accounts.list()[0].settings.imap;
    assert_eq!(imap.security, Security::Tls);
    assert_eq!(imap.pin, None);
}

#[test]
fn a_malformed_pin_breaks_only_its_account() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    let mut accounts = Accounts::open(&path).unwrap();
    let mut pinned = settings("ada@example.org");
    pinned.imap.pin = Some(PIN.into());
    let bad = accounts.add(pinned, 1_000).unwrap();
    let good = accounts.add(settings("bob@example.org"), 2_000).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text.replace(PIN, "not-a-pin")).unwrap();

    let mut accounts = Accounts::open(&path).unwrap();
    let broken = accounts.get(&bad).unwrap();
    let reason = broken.broken.as_deref().unwrap();
    assert!(reason.contains("pin"), "{reason}");
    assert_eq!(broken.settings.email, "ada@example.org");
    assert_eq!(accounts.get(&good).unwrap().broken, None);

    // Adding another account keeps the broken entry as it was.
    accounts.add(settings("cy@example.org"), 3_000).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("not-a-pin"), "{text}");
    let reopened = Accounts::open(&path).unwrap();
    assert_eq!(reopened.list().len(), 3);
    assert!(reopened.get(&bad).unwrap().broken.is_some());
}

#[test]
fn an_entry_that_does_not_parse_is_broken_with_its_key() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    let text = r#"{"version": 2, "accounts": [{"key": "1000",
      "settings": {"name": "Ada", "email": "ada@example.org", "imap": {"host": "h", "port": "SECRET-PORT-VALUE"}}}]}"#;
    fs::write(&path, text).unwrap();
    let accounts = Accounts::open(&path).unwrap();
    let account = accounts.get("1000").unwrap();
    assert_eq!(
        account.broken.as_deref(),
        Some("unreadable IMAP server settings")
    );
    assert_eq!(account.settings.name, "Ada");
}

#[test]
fn a_newer_version_still_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    fs::write(&path, r#"{"version": 3, "accounts": [{"key": "1000"}]}"#).unwrap();
    assert!(matches!(
        Accounts::open(&path),
        Err(StoreError::BadRecord { .. })
    ));
}

#[test]
fn a_broken_account_can_be_removed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    let mut accounts = Accounts::open(&path).unwrap();
    let mut pinned = settings("ada@example.org");
    pinned.imap.pin = Some(PIN.into());
    let bad = accounts.add(pinned, 1_000).unwrap();
    let good = accounts.add(settings("bob@example.org"), 2_000).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text.replace(PIN, "not-a-pin")).unwrap();

    let mut accounts = Accounts::open(&path).unwrap();
    accounts.remove(&bad).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(!text.contains("not-a-pin"), "{text}");
    let reopened = Accounts::open(&path).unwrap();
    assert_eq!(reopened.list().len(), 1);
    assert!(reopened.get(&good).is_some());
}

#[test]
fn a_malformed_pin_is_never_written() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    let mut accounts = Accounts::open(&path).unwrap();
    let mut pinned = settings("ada@example.org");
    // Uppercase is the display form, not the stored one.
    pinned.smtp.pin = Some(PIN.to_uppercase());
    assert!(matches!(
        accounts.add(pinned, 1_000),
        Err(StoreError::BadRecord { .. })
    ));
    assert!(accounts.list().is_empty());
    assert!(!path.exists());
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
    fs::write(&path, br#"{"version": 3, "accounts": []}"#).unwrap();
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

#[test]
fn an_account_without_a_label_still_loads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    let old = r#"{"version": 2, "accounts": [{"key": "1000",
      "settings": {"name": "Ada", "email": "ada@example.org",
      "username": "ada", "imap": {"host": "imap.example.org", "port": 993},
      "smtp": {"host": "smtp.example.org", "port": 465}}}]}"#;
    fs::write(&path, old).unwrap();
    let accounts = Accounts::open(&path).unwrap();
    assert_eq!(accounts.list()[0].label, None);
    assert_eq!(accounts.list()[0].settings.name, "Ada");
}

#[test]
fn a_label_round_trips_and_clearing_it_drops_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    let mut accounts = Accounts::open(&path).unwrap();
    let key = accounts.add(settings("ada@example.org"), 1_000).unwrap();
    accounts.set_label(&key, Some("  Work ")).unwrap();
    let reopened = Accounts::open(&path).unwrap();
    assert_eq!(reopened.get(&key).unwrap().label.as_deref(), Some("Work"));
    assert_eq!(reopened.get(&key).unwrap().settings.name, "Ada");
    // A settings edit keeps the label.
    accounts.update(&key, settings("ada@example.org")).unwrap();
    assert_eq!(accounts.get(&key).unwrap().label.as_deref(), Some("Work"));
    accounts.set_label(&key, Some("   ")).unwrap();
    let cleared = Accounts::open(&path).unwrap();
    assert_eq!(cleared.get(&key).unwrap().label, None);
    assert!(!fs::read_to_string(&path).unwrap().contains("label"));
}

#[test]
fn a_blank_label_counts_as_unset() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    let text = r#"{"version": 2, "accounts": [
      {"key": "1000", "label": "   ", "settings": {"name": "Ada", "email": "a@x.org",
        "username": "", "imap": {"host": "h", "port": 993}, "smtp": {"host": "h", "port": 465}}},
      {"key": "1001", "label": " Work ", "settings": {"name": "Bo", "email": "b@x.org",
        "username": "", "imap": {"host": "h", "port": 993}, "smtp": {"host": "h", "port": 465}}}]}"#;
    fs::write(&path, text).unwrap();
    let accounts = Accounts::open(&path).unwrap();
    assert_eq!(accounts.get("1000").unwrap().label(), None);
    assert_eq!(accounts.get("1001").unwrap().label(), Some("Work"));
}

#[test]
fn a_broken_entry_refuses_a_label_and_keeps_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    let mut accounts = Accounts::open(&path).unwrap();
    let mut pinned = settings("ada@example.org");
    pinned.imap.pin = Some(PIN.into());
    let bad = accounts.add(pinned, 1_000).unwrap();
    accounts.set_label(&bad, Some("Work")).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text.replace(PIN, "not-a-pin")).unwrap();

    let mut accounts = Accounts::open(&path).unwrap();
    // The stand-in keeps the label the entry had.
    assert_eq!(accounts.get(&bad).unwrap().label(), Some("Work"));
    // A new one is refused, and memory still matches the file.
    assert!(accounts.set_label(&bad, Some("Other")).is_err());
    assert_eq!(accounts.get(&bad).unwrap().label(), Some("Work"));
    let reopened = Accounts::open(&path).unwrap();
    assert_eq!(reopened.get(&bad).unwrap().label(), Some("Work"));
}

#[test]
fn an_old_file_loads_without_auth_and_saves_without_the_field() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    let old = r#"{"version": 2, "accounts": [{"key": "1000",
      "settings": {"name": "Ada", "email": "ada@example.org",
      "username": "ada", "imap": {"host": "imap.example.org", "port": 993},
      "smtp": {"host": "smtp.example.org", "port": 465}}}]}"#;
    fs::write(&path, old).unwrap();
    let mut accounts = Accounts::open(&path).unwrap();
    let settings = accounts.list()[0].settings.clone();
    assert_eq!(settings.auth, None);
    assert!(!settings.is_oauth());
    accounts.update("1000", settings).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(!text.contains("auth"), "{text}");
}

#[test]
fn a_none_auth_round_trips_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    let mut accounts = Accounts::open(&path).unwrap();
    let key = accounts.add(settings("ada@example.org"), 1_000).unwrap();
    let first = fs::read(&path).unwrap();
    let settings = accounts.get(&key).unwrap().settings.clone();
    accounts.update(&key, settings).unwrap();
    assert_eq!(fs::read(&path).unwrap(), first);
    assert!(!String::from_utf8(first).unwrap().contains("auth"));
}

#[test]
fn a_microsoft_account_round_trips_its_auth_kind() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    let mut accounts = Accounts::open(&path).unwrap();
    let mut ms = settings("ada@outlook.example");
    ms.auth = Some("microsoft".into());
    let key = accounts.add(ms.clone(), 1_000).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"auth\": \"microsoft\""), "{text}");
    let reopened = Accounts::open(&path).unwrap();
    let got = &reopened.get(&key).unwrap().settings;
    assert_eq!(got, &ms);
    assert!(got.is_oauth());
}

#[test]
fn a_broken_entry_keeps_no_auth_but_stays_raw() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accounts.json");
    let broken = r#"{"version": 2, "accounts": [{"key": "1000",
      "settings": {"name": "Ada", "email": "ada@outlook.example", "auth": "microsoft"}}]}"#;
    fs::write(&path, broken).unwrap();
    let accounts = Accounts::open(&path).unwrap();
    let account = &accounts.list()[0];
    assert!(account.broken.is_some());
    assert!(!account.settings.is_oauth());
}

#[test]
fn an_account_with_an_unknown_future_auth_kind_is_not_oauth() {
    let mut s = settings("ada@example.org");
    s.auth = Some("someday".into());
    assert!(!s.is_oauth());
    s.auth = Some("microsoft".into());
    assert!(s.is_oauth());
}
