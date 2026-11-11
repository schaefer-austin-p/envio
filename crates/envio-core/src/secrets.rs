//! Passwords, in their own 0600 file next to (not inside) the cache.

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, Permissions};
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub const FILE_NAME: &str = "passwords.json";

const VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum SecretsError {
    #[error("passwords file: {0}")]
    Io(#[from] io::Error),
    #[error("passwords file is damaged")]
    Damaged,
    /// Written by a newer envio: left alone, so a downgrade loses nothing.
    #[error("passwords file is from a newer envio (version {0})")]
    Newer(u32),
}

#[derive(Deserialize)]
struct Version {
    version: u32,
}

#[derive(Deserialize)]
struct FileV1 {
    version: u32,
    passwords: BTreeMap<String, SecretString>,
}

#[derive(Serialize)]
struct FileOut<'a> {
    version: u32,
    passwords: BTreeMap<&'a str, &'a str>,
}

pub struct Secrets {
    path: PathBuf,
    map: BTreeMap<String, SecretString>,
}

impl fmt::Debug for Secrets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Secrets")
            .field("accounts", &self.map.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Secrets {
    pub fn open(data_dir: &Path) -> Result<Self, SecretsError> {
        let path = data_dir.join(FILE_NAME);
        let map = read(&path)?;
        Ok(Self { path, map })
    }

    pub fn get(&self, account: &str) -> Option<&SecretString> {
        self.map.get(account)
    }

    pub fn set(&mut self, account: &str, password: SecretString) -> Result<(), SecretsError> {
        let mut next = self.map.clone();
        next.insert(account.to_owned(), password);
        write(&self.path, &next)?;
        self.map = next;
        Ok(())
    }

    pub fn remove(&mut self, account: &str) -> Result<(), SecretsError> {
        if !self.map.contains_key(account) {
            return Ok(());
        }
        let mut next = self.map.clone();
        next.remove(account);
        write(&self.path, &next)?;
        self.map = next;
        Ok(())
    }

    pub fn accounts(&self) -> Vec<String> {
        self.map.keys().cloned().collect()
    }
}

fn write(path: &Path, map: &BTreeMap<String, SecretString>) -> Result<(), SecretsError> {
    let plain: BTreeMap<&str, &str> = map
        .iter()
        .map(|(k, v)| (k.as_str(), v.expose_secret()))
        .collect();
    let out = FileOut {
        version: VERSION,
        passwords: plain,
    };
    let mut bytes = Zeroizing::new(Vec::new());
    serde_json::to_writer(&mut *bytes, &out).map_err(|_| SecretsError::Damaged)?;
    Ok(crate::fsutil::write_atomic(path, &bytes, 0o600)?)
}

fn read(path: &Path) -> Result<BTreeMap<String, SecretString>, SecretsError> {
    let meta = match fs::metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        r => r?,
    };
    if meta.permissions().mode() & 0o077 != 0 {
        fs::set_permissions(path, Permissions::from_mode(0o600))?;
        log::warn!("passwords file was readable by others; narrowed to 0600");
    }
    let bytes = Zeroizing::new(fs::read(path)?);
    if let Ok(Version { version }) = serde_json::from_slice(&bytes)
        && version > VERSION
    {
        return Err(SecretsError::Newer(version));
    }
    match serde_json::from_slice::<FileV1>(&bytes) {
        Ok(f) if f.version == VERSION => Ok(f.passwords),
        _ => {
            let mut aside = path.as_os_str().to_owned();
            aside.push(".damaged");
            fs::rename(path, aside)?;
            log::error!("passwords file is damaged; moved aside");
            Ok(BTreeMap::new())
        }
    }
}
