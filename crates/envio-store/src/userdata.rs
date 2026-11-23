//! What the user chose and what they typed that isn't mail: `settings.json`
//! and `contacts.json` in the data folder, so a rebuilt cache keeps them.
//! Both read leniently (unknown fields ignored, missing ones defaulted).

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::atomic::write_atomic;
use crate::error::Result;

/// The version `save_settings` writes.
const SETTINGS_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeName {
    #[default]
    Carbon,
    Airmail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DarkMode {
    #[default]
    System,
    Light,
    Dark,
}

/// Whether remote images load without asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ImagesDefault {
    #[default]
    Ask,
    Always,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub theme: ThemeName,
    pub dark: DarkMode,
    pub images: ImagesDefault,
    /// Senders whose remote images always load.
    pub images_from: BTreeSet<String>,
    pub default_account: Option<String>,
    /// How many days back sync caches: 30, 90, 365, or 0 for all. Any other
    /// number read from the file snaps to 30, so an odd value never moves
    /// the whole file to `.damaged`.
    #[serde(default = "thirty", deserialize_with = "snap_window")]
    pub sync_window_days: u32,
    /// Account keys whose new-mail notifications are off (empty: every
    /// account notifies).
    pub notify_off: BTreeSet<String>,
    /// Whether a card for one new message shows its sender and subject.
    pub notify_show_text: bool,
    /// Run fullscreen (phone: the indicator panel hides until a swipe down).
    pub fullscreen: bool,
    /// The list pane's width in grid units in the two-pane layout; read
    /// values snap into `LIST_PANE_MIN_GU..=LIST_PANE_MAX_GU`.
    #[serde(deserialize_with = "clamp_list_pane")]
    pub list_pane_gu: u32,
}

/// The list pane's default, smallest and largest width in grid units.
pub const LIST_PANE_DEFAULT_GU: u32 = 50;
pub const LIST_PANE_MIN_GU: u32 = 36;
pub const LIST_PANE_MAX_GU: u32 = 70;

/// `gu` limited to what the splitter allows.
pub fn clamp_list_pane_gu(gu: i64) -> u32 {
    gu.clamp(LIST_PANE_MIN_GU.into(), LIST_PANE_MAX_GU.into()) as u32
}

fn clamp_list_pane<'de, D: serde::Deserializer<'de>>(de: D) -> std::result::Result<u32, D::Error> {
    // Like the sync window: an odd number is snapped, a wrong type is not.
    Ok(clamp_list_pane_gu(i64::deserialize(de)?))
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 0,
            theme: ThemeName::default(),
            dark: DarkMode::default(),
            images: ImagesDefault::default(),
            images_from: BTreeSet::new(),
            default_account: None,
            sync_window_days: thirty(),
            notify_off: BTreeSet::new(),
            notify_show_text: false,
            fullscreen: false,
            list_pane_gu: LIST_PANE_DEFAULT_GU,
        }
    }
}

impl Settings {
    /// The sync window in days; `None` means all mail.
    pub fn sync_window(&self) -> Option<u32> {
        match self.sync_window_days {
            0 => None,
            days => Some(days),
        }
    }
}

fn thirty() -> u32 {
    30
}

fn snap_window<'de, D: serde::Deserializer<'de>>(de: D) -> std::result::Result<u32, D::Error> {
    // A wrong type (not a number) is still an error; an odd number is not.
    let days = i64::deserialize(de)?;
    Ok(match days {
        0 => 0,
        90 => 90,
        365 => 365,
        _ => 30,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    pub name: String,
    pub address: String,
}

/// Missing file: defaults. A file that cannot be read or does not parse:
/// moved to `<name>.damaged` (or `.damaged.1`, … so an earlier one is kept),
/// then defaults. Moving it aside means a later save never writes over it.
pub fn load_settings(path: &Path) -> Result<Settings> {
    load(path)
}

/// The settings as saved, for a start-up that cannot wait for the worker.
/// Read-only: it never writes, renames or creates anything (the worker's
/// `load_settings` sets a bad file aside). Missing or unparsable: defaults.
pub fn peek_settings(path: &Path) -> Settings {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_settings(path: &Path, settings: &Settings) -> Result<()> {
    let settings = Settings {
        version: SETTINGS_VERSION,
        ..settings.clone()
    };
    save(path, &settings)
}

pub fn load_contacts(path: &Path) -> Result<Vec<Contact>> {
    load(path)
}

pub fn save_contacts(path: &Path, contacts: &[Contact]) -> Result<()> {
    save(path, &contacts)
}

pub(crate) fn load<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(T::default()),
        Err(_) => return set_aside(path),
    };
    match serde_json::from_slice(&bytes) {
        Ok(value) => Ok(value),
        Err(_) => set_aside(path),
    }
}

pub(crate) fn save<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).expect("user data always serializes");
    Ok(write_atomic(path, &bytes)?)
}

/// Keeps the unreadable file for a bug report and gives back the defaults.
/// An earlier damaged copy is never replaced.
fn set_aside<T: Default>(path: &Path) -> Result<T> {
    let mut n = 0u32;
    loop {
        let mut name: OsString = path.as_os_str().to_owned();
        name.push(".damaged");
        if n > 0 {
            name.push(format!(".{n}"));
        }
        if !Path::new(&name).exists() {
            fs::rename(path, name)?;
            return Ok(T::default());
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir_with(name: &str, bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    #[test]
    fn peek_missing_file_is_the_defaults_and_creates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        assert_eq!(peek_settings(&path), Settings::default());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn peek_corrupt_file_is_the_defaults_and_leaves_the_file() {
        let (dir, path) = dir_with("settings.json", b"{ not json");
        assert_eq!(peek_settings(&path), Settings::default());
        assert_eq!(fs::read(&path).unwrap(), b"{ not json");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn peek_reads_what_save_wrote_and_matches_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let saved = Settings {
            theme: ThemeName::Airmail,
            dark: DarkMode::Dark,
            fullscreen: true,
            ..Settings::default()
        };
        save_settings(&path, &saved).unwrap();
        let peeked = peek_settings(&path);
        assert_eq!(peeked.theme, ThemeName::Airmail);
        assert_eq!(peeked.dark, DarkMode::Dark);
        assert!(peeked.fullscreen);
        assert_eq!(peeked, load_settings(&path).unwrap());
    }

    #[test]
    fn peek_snaps_odd_values_like_load() {
        let (_dir, path) = dir_with(
            "settings.json",
            br#"{"sync_window_days":7,"list_pane_gu":9999,"theme":"airmail"}"#,
        );
        assert_eq!(peek_settings(&path), load_settings(&path).unwrap());
    }

    #[test]
    fn enums_use_lowercase_names() {
        let json = serde_json::to_string(&Settings::default()).unwrap();
        assert!(json.contains(r#""theme":"carbon""#), "{json}");
        assert!(json.contains(r#""dark":"system""#), "{json}");
        assert!(json.contains(r#""images":"ask""#), "{json}");
    }

    #[test]
    fn sync_window_defaults_to_thirty() {
        let settings = Settings::default();
        assert_eq!(settings.sync_window_days, 30);
        assert_eq!(settings.sync_window(), Some(30));
        let all = Settings {
            sync_window_days: 0,
            ..Settings::default()
        };
        assert_eq!(all.sync_window(), None);
    }

    #[test]
    fn odd_sync_window_snaps_to_thirty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        for (text, want) in [("45", 30), ("-1", 30), ("90", 90), ("365", 365), ("0", 0)] {
            fs::write(&path, format!(r#"{{"sync_window_days": {text}}}"#)).unwrap();
            assert_eq!(load_settings(&path).unwrap().sync_window_days, want);
        }
        assert!(!dir.path().join("settings.json.damaged").exists());
    }

    #[test]
    fn settings_without_the_field_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"version": 1, "theme": "airmail"}"#).unwrap();
        let settings = load_settings(&path).unwrap();
        assert_eq!(settings.theme, ThemeName::Airmail);
        assert_eq!(settings.sync_window_days, 30);
    }

    #[test]
    fn old_settings_files_read_with_notifications_on() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"version": 1, "theme": "airmail"}"#).unwrap();
        let settings = load_settings(&path).unwrap();
        assert!(settings.notify_off.is_empty());
        assert!(!settings.notify_show_text);
    }

    #[test]
    fn list_pane_width_defaults_clamps_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"version": 1}"#).unwrap();
        assert_eq!(load_settings(&path).unwrap().list_pane_gu, 50);
        fs::write(&path, r#"{"version": 1, "list_pane_gu": 5}"#).unwrap();
        assert_eq!(load_settings(&path).unwrap().list_pane_gu, 36);
        fs::write(&path, r#"{"version": 1, "list_pane_gu": 900}"#).unwrap();
        assert_eq!(load_settings(&path).unwrap().list_pane_gu, 70);
        let settings = Settings {
            list_pane_gu: 61,
            ..Settings::default()
        };
        save_settings(&path, &settings).unwrap();
        assert_eq!(load_settings(&path).unwrap().list_pane_gu, 61);
    }

    #[test]
    fn fullscreen_is_off_by_default_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        assert!(!Settings::default().fullscreen);
        // A file written before the field existed reads as off.
        std::fs::write(&path, r#"{"version":1}"#).unwrap();
        assert!(!load_settings(&path).unwrap().fullscreen);
        let on = Settings {
            fullscreen: true,
            ..Settings::default()
        };
        save_settings(&path, &on).unwrap();
        assert!(load_settings(&path).unwrap().fullscreen);
    }

    #[test]
    fn notify_fields_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let settings = Settings {
            notify_off: BTreeSet::from(["a1".to_owned(), "b2".to_owned()]),
            notify_show_text: true,
            ..Settings::default()
        };
        save_settings(&path, &settings).unwrap();
        let back = load_settings(&path).unwrap();
        assert_eq!(back.notify_off, settings.notify_off);
        assert!(back.notify_show_text);
    }
}
