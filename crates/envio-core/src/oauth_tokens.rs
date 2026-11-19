//! OAuth refresh tokens, in their own 0600 file next to (not inside) the cache.
//!
//! Not in `passwords.json`: an older envio would try to log in with a refresh
//! token found there, so a downgrade must fail closed. The access token is
//! never written anywhere.

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, Permissions};
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub const FILE_NAME: &str = "oauth.json";

const VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum TokensError {
    #[error("OAuth token file: {0}")]
    Io(#[from] io::Error),
    #[error("OAuth token file is damaged")]
    Damaged,
    /// Written by a newer envio: left alone, so a downgrade loses nothing.
    #[error("OAuth token file is from a newer envio (version {0})")]
    Newer(u32),
}

#[derive(Deserialize)]
struct Version {
    version: u32,
}

#[derive(Deserialize)]
struct FileV1 {
    version: u32,
    tokens: BTreeMap<String, SecretString>,
}

#[derive(Serialize)]
struct FileOut<'a> {
    version: u32,
    tokens: BTreeMap<&'a str, &'a str>,
}

pub struct OAuthTokens {
    path: PathBuf,
    map: BTreeMap<String, SecretString>,
}

impl fmt::Debug for OAuthTokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OAuthTokens")
            .field("accounts", &self.map.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl OAuthTokens {
    pub fn open(data_dir: &Path) -> Result<Self, TokensError> {
        let path = data_dir.join(FILE_NAME);
        let map = read(&path)?;
        Ok(Self { path, map })
    }

    pub fn get(&self, account: &str) -> Option<&SecretString> {
        self.map.get(account)
    }

    pub fn set(&mut self, account: &str, refresh: SecretString) -> Result<(), TokensError> {
        let mut next = self.map.clone();
        next.insert(account.to_owned(), refresh);
        write(&self.path, &next)?;
        self.map = next;
        Ok(())
    }

    pub fn remove(&mut self, account: &str) -> Result<(), TokensError> {
        if !self.map.contains_key(account) {
            return Ok(());
        }
        let mut next = self.map.clone();
        next.remove(account);
        write(&self.path, &next)?;
        self.map = next;
        Ok(())
    }
}

fn write(path: &Path, map: &BTreeMap<String, SecretString>) -> Result<(), TokensError> {
    let plain: BTreeMap<&str, &str> = map
        .iter()
        .map(|(k, v)| (k.as_str(), v.expose_secret()))
        .collect();
    let out = FileOut {
        version: VERSION,
        tokens: plain,
    };
    let mut bytes = Zeroizing::new(Vec::new());
    serde_json::to_writer(&mut *bytes, &out).map_err(|_| TokensError::Damaged)?;
    Ok(crate::fsutil::write_atomic(path, &bytes, 0o600)?)
}

fn read(path: &Path) -> Result<BTreeMap<String, SecretString>, TokensError> {
    let meta = match fs::metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        r => r?,
    };
    if meta.permissions().mode() & 0o077 != 0 {
        fs::set_permissions(path, Permissions::from_mode(0o600))?;
        log::warn!("OAuth token file was readable by others; narrowed to 0600");
    }
    let bytes = Zeroizing::new(fs::read(path)?);
    if let Ok(Version { version }) = serde_json::from_slice(&bytes)
        && version > VERSION
    {
        return Err(TokensError::Newer(version));
    }
    match serde_json::from_slice::<FileV1>(&bytes) {
        Ok(f) if f.version == VERSION => Ok(f.tokens),
        _ => {
            let mut aside = path.as_os_str().to_owned();
            // Unique, so a second damaged file never replaces the first.
            aside.push(format!(".damaged-{}", crate::clock::now_ms()));
            fs::rename(path, aside)?;
            log::error!("OAuth token file is damaged; moved aside");
            Ok(BTreeMap::new())
        }
    }
}
