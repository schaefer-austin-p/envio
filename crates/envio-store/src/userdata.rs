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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub theme: ThemeName,
    pub dark: DarkMode,
    pub images: ImagesDefault,
    /// Senders whose remote images always load.
    pub images_from: BTreeSet<String>,
    pub default_account: Option<String>,
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

fn load<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T> {
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

fn save<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<()> {
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

    #[test]
    fn enums_use_lowercase_names() {
        let json = serde_json::to_string(&Settings::default()).unwrap();
        assert!(json.contains(r#""theme":"carbon""#), "{json}");
        assert!(json.contains(r#""dark":"system""#), "{json}");
        assert!(json.contains(r#""images":"ask""#), "{json}");
    }
}
