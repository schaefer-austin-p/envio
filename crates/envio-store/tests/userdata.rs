use std::fs;

use envio_store::{
    Contact, DarkMode, ImagesDefault, Settings, ThemeName, load_contacts, load_settings,
    save_contacts, save_settings,
};

#[test]
fn missing_files_give_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let settings = load_settings(&dir.path().join("settings.json")).unwrap();
    assert_eq!(settings, Settings::default());
    assert_eq!(settings.theme, ThemeName::Carbon);
    assert_eq!(settings.dark, DarkMode::System);
    assert_eq!(settings.images, ImagesDefault::Ask);
    assert!(
        load_contacts(&dir.path().join("contacts.json"))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn settings_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let settings = Settings {
        version: 1,
        theme: ThemeName::Airmail,
        dark: DarkMode::Dark,
        images: ImagesDefault::Always,
        images_from: ["news@example.org".to_owned()].into(),
        default_account: Some("1700".into()),
    };
    save_settings(&path, &settings).unwrap();
    assert_eq!(load_settings(&path).unwrap(), settings);

    // The version written is 1 whatever the caller held.
    save_settings(&path, &Settings::default()).unwrap();
    assert_eq!(load_settings(&path).unwrap().version, 1);

    let contacts_path = dir.path().join("contacts.json");
    let contacts = vec![Contact {
        name: "Bob".into(),
        address: "bob@example.org".into(),
    }];
    save_contacts(&contacts_path, &contacts).unwrap();
    assert_eq!(load_contacts(&contacts_path).unwrap(), contacts);
}

#[test]
fn unknown_fields_are_ignored_and_missing_fields_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(
        &path,
        r#"{"version": 9, "theme": "airmail", "future": [1, 2]}"#,
    )
    .unwrap();
    let settings = load_settings(&path).unwrap();
    assert_eq!(settings.theme, ThemeName::Airmail);
    assert_eq!(settings.dark, DarkMode::System);
    assert_eq!(settings.default_account, None);
    assert!(settings.images_from.is_empty());
}

#[test]
fn an_unreadable_settings_file_is_moved_aside() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, b"{ not json").unwrap();
    assert_eq!(load_settings(&path).unwrap(), Settings::default());
    assert!(!path.exists());
    assert_eq!(
        fs::read(dir.path().join("settings.json.damaged")).unwrap(),
        b"{ not json"
    );

    let contacts = dir.path().join("contacts.json");
    fs::write(&contacts, b"[1, 2").unwrap();
    assert!(load_contacts(&contacts).unwrap().is_empty());
    assert!(dir.path().join("contacts.json.damaged").exists());
}

#[test]
fn a_second_damaged_file_keeps_the_first() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, b"first").unwrap();
    assert_eq!(load_settings(&path).unwrap(), Settings::default());
    fs::write(&path, b"second").unwrap();
    assert_eq!(load_settings(&path).unwrap(), Settings::default());
    assert_eq!(
        fs::read(dir.path().join("settings.json.damaged")).unwrap(),
        b"first"
    );
    assert_eq!(
        fs::read(dir.path().join("settings.json.damaged.1")).unwrap(),
        b"second"
    );
}

#[test]
fn a_file_that_cannot_be_read_is_moved_aside_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    // A directory where the file should be: reading fails, not parsing.
    let path = dir.path().join("settings.json");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("keep"), b"x").unwrap();
    assert_eq!(load_settings(&path).unwrap(), Settings::default());
    assert!(
        dir.path()
            .join("settings.json.damaged")
            .join("keep")
            .exists()
    );
    // A later save writes a new file and leaves the damaged copy alone.
    save_settings(&path, &Settings::default()).unwrap();
    assert!(path.is_file());
    assert!(
        dir.path()
            .join("settings.json.damaged")
            .join("keep")
            .exists()
    );
}
