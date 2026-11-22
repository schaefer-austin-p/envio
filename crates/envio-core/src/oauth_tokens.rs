//! OAuth refresh tokens, in their own 0600 file next to (not inside) the cache.
//!
//! Not in `passwords.json`: an older envio would try to log in with a refresh
//! token found there, so a downgrade must fail closed. The access token is
//! never written anywhere.
//!
//! Version 1 holds one token per account (the IMAP one). Version 2 keys each
//! account's tokens by the backend that uses them (`imap`, `graph`); it is
//! written only while some account holds a Graph token, so an older build
//! refuses it (fails closed) and IMAP-only users can downgrade.

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, Permissions};
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use envio_net::oauth::ProviderKind;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub const FILE_NAME: &str = "oauth.json";

const VERSION: u32 = 2;
/// Written while no account holds a Graph token.
const BASE_VERSION: u32 = 1;

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

#[derive(Deserialize)]
struct Pair {
    imap: Option<SecretString>,
    graph: Option<SecretString>,
}

#[derive(Deserialize)]
struct FileV2 {
    version: u32,
    tokens: BTreeMap<String, Pair>,
}

#[derive(Serialize)]
struct PairOut<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    imap: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    graph: Option<&'a str>,
}

#[derive(Serialize)]
struct OutV2<'a> {
    version: u32,
    tokens: BTreeMap<&'a str, PairOut<'a>>,
}

#[derive(Serialize)]
struct OutV1<'a> {
    version: u32,
    tokens: BTreeMap<&'a str, &'a str>,
}

/// One account's refresh tokens: the IMAP sign-in and the Graph sign-in are
/// separate grants with different scopes.
#[derive(Clone, Default)]
struct Held {
    imap: Option<SecretString>,
    graph: Option<SecretString>,
}

impl Held {
    fn slot(&mut self, kind: ProviderKind) -> &mut Option<SecretString> {
        match kind {
            ProviderKind::Microsoft => &mut self.imap,
            ProviderKind::MicrosoftGraph => &mut self.graph,
        }
    }

    fn is_empty(&self) -> bool {
        self.imap.is_none() && self.graph.is_none()
    }
}

type Map = BTreeMap<String, Held>;

pub struct OAuthTokens {
    path: PathBuf,
    map: Map,
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

    pub fn get(&self, account: &str, kind: ProviderKind) -> Option<&SecretString> {
        let held = self.map.get(account)?;
        match kind {
            ProviderKind::Microsoft => held.imap.as_ref(),
            ProviderKind::MicrosoftGraph => held.graph.as_ref(),
        }
    }

    pub fn set(
        &mut self,
        account: &str,
        kind: ProviderKind,
        refresh: SecretString,
    ) -> Result<(), TokensError> {
        let mut next = self.map.clone();
        *next.entry(account.to_owned()).or_default().slot(kind) = Some(refresh);
        write(&self.path, &next)?;
        self.map = next;
        Ok(())
    }

    pub fn remove(&mut self, account: &str, kind: ProviderKind) -> Result<(), TokensError> {
        if self.get(account, kind).is_none() {
            return Ok(());
        }
        let mut next = self.map.clone();
        if let Some(held) = next.get_mut(account) {
            *held.slot(kind) = None;
            if held.is_empty() {
                next.remove(account);
            }
        }
        write(&self.path, &next)?;
        self.map = next;
        Ok(())
    }

    /// Drops the account's tokens of every kind (account removal).
    pub fn remove_account(&mut self, account: &str) -> Result<(), TokensError> {
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

fn write(path: &Path, map: &Map) -> Result<(), TokensError> {
    let v2 = map.values().any(|held| held.graph.is_some());
    let mut bytes = Zeroizing::new(Vec::new());
    if v2 {
        let tokens = map
            .iter()
            .map(|(account, held)| {
                let pair = PairOut {
                    imap: held.imap.as_ref().map(ExposeSecret::expose_secret),
                    graph: held.graph.as_ref().map(ExposeSecret::expose_secret),
                };
                (account.as_str(), pair)
            })
            .collect();
        let out = OutV2 {
            version: VERSION,
            tokens,
        };
        serde_json::to_writer(&mut *bytes, &out).map_err(|_| TokensError::Damaged)?;
    } else {
        let tokens = map
            .iter()
            .filter_map(|(account, held)| {
                let token = held.imap.as_ref()?;
                Some((account.as_str(), token.expose_secret()))
            })
            .collect();
        let out = OutV1 {
            version: BASE_VERSION,
            tokens,
        };
        serde_json::to_writer(&mut *bytes, &out).map_err(|_| TokensError::Damaged)?;
    }
    Ok(crate::fsutil::write_atomic(path, &bytes, 0o600)?)
}

fn parse_v1(bytes: &[u8]) -> Option<Map> {
    let file = serde_json::from_slice::<FileV1>(bytes).ok()?;
    (file.version == BASE_VERSION).then(|| {
        // Version 1 files load with every token as `imap`.
        file.tokens
            .into_iter()
            .map(|(account, token)| {
                let held = Held {
                    imap: Some(token),
                    graph: None,
                };
                (account, held)
            })
            .collect()
    })
}

fn parse_v2(bytes: &[u8]) -> Option<Map> {
    let file = serde_json::from_slice::<FileV2>(bytes).ok()?;
    (file.version == VERSION).then(|| {
        file.tokens
            .into_iter()
            .map(|(account, pair)| {
                let held = Held {
                    imap: pair.imap,
                    graph: pair.graph,
                };
                (account, held)
            })
            .filter(|(_, held)| !held.is_empty())
            .collect()
    })
}

fn read(path: &Path) -> Result<Map, TokensError> {
    let meta = match fs::metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        r => r?,
    };
    if meta.permissions().mode() & 0o077 != 0 {
        fs::set_permissions(path, Permissions::from_mode(0o600))?;
        log::warn!("OAuth token file was readable by others; narrowed to 0600");
    }
    let bytes = Zeroizing::new(fs::read(path)?);
    let version = serde_json::from_slice(&bytes)
        .ok()
        .map(|Version { version }| version);
    if let Some(version) = version
        && version > VERSION
    {
        return Err(TokensError::Newer(version));
    }
    let parsed = match version {
        Some(BASE_VERSION) => parse_v1(&bytes),
        Some(VERSION) => parse_v2(&bytes),
        _ => None,
    };
    if let Some(map) = parsed {
        return Ok(map);
    }
    let mut aside = path.as_os_str().to_owned();
    // Unique, so a second damaged file never replaces the first.
    aside.push(format!(".damaged-{}", crate::clock::now_ms()));
    fs::rename(path, aside)?;
    log::error!("OAuth token file is damaged; moved aside");
    Ok(BTreeMap::new())
}
