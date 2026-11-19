//! Account settings in `accounts.json` in the data folder, not in the cache,
//! so a rebuilt cache keeps them. Passwords are never here (Plan 6 keeps
//! them in their own 0600 file).

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic::write_atomic;
use crate::error::{Result, StoreError};
use crate::ids::{fresh_key, is_key};

/// 2 added `security` and `pin`. An envio that only knows 1 refuses the
/// file rather than rewriting it without the pins.
const VERSION: u32 = 2;

/// How the connection is encrypted; envio-net's `Security`, kept here as
/// plain data because store and net don't depend on each other.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Security {
    /// TLS from the first byte. Version 1 files had only this.
    #[default]
    Tls,
    StartTls,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Server {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub security: Security,
    /// The accepted self-signed certificate: SHA-256 of its DER as 64
    /// lowercase hex digits (envio-net's `Fingerprint::to_hex`). Here and
    /// not in the cache, so a rebuilt cache can't drop it and quietly
    /// trust whatever certificate comes next.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSettings {
    /// The name shown on sent mail.
    pub name: String,
    pub email: String,
    pub username: String,
    pub imap: Server,
    pub smtp: Server,
    /// How the account signs in. Absent: a password. `"microsoft"`: OAuth
    /// 2.0 (the refresh token lives in the core's `oauth.json`, never here).
    /// A plain string, so this crate needs no OAuth types.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,
}

impl AccountSettings {
    /// Whether the account signs in with an OAuth token, not a password.
    pub fn is_oauth(&self) -> bool {
        self.auth.as_deref() == Some("microsoft")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    /// Never changes; the cache, journal and Outbox name the account by it.
    pub key: String,
    pub settings: AccountSettings,
    /// What the user calls the account (tabs, drawer, Accounts page). Absent
    /// or empty: the screens show the name, else the address. Not the sender
    /// name, which stays in `settings.name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Set when this entry of the file could not be read: what failed, with
    /// no secrets. `settings` then holds only what parsed (name, email) and
    /// empty values elsewhere, and the account must not be used. The file
    /// keeps the entry as it was.
    #[serde(skip)]
    pub broken: Option<String>,
}

impl Account {
    /// The label if the user set a non-blank one. Read the label only through
    /// this: a blank or whitespace-only string counts as unset.
    pub fn label(&self) -> Option<&str> {
        self.label
            .as_deref()
            .map(str::trim)
            .filter(|l| !l.is_empty())
    }
}

#[derive(Deserialize)]
struct FileIn {
    version: u32,
    accounts: Vec<serde_json::Value>,
}

#[derive(Serialize)]
struct FileOut {
    version: u32,
    accounts: Vec<serde_json::Value>,
}

pub struct Accounts {
    path: PathBuf,
    accounts: Vec<Account>,
    /// The original JSON of each broken entry by key, written back unchanged.
    raw: HashMap<String, serde_json::Value>,
}

impl Accounts {
    /// Reads `path`; a missing file means no accounts yet.
    pub fn open(path: &Path) -> Result<Self> {
        let (accounts, raw) = match fs::read(path) {
            Ok(bytes) => parse(path, &bytes)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Default::default(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            path: path.to_owned(),
            accounts,
            raw,
        })
    }

    pub fn list(&self) -> &[Account] {
        &self.accounts
    }

    pub fn get(&self, key: &str) -> Option<&Account> {
        self.accounts.iter().find(|account| account.key == key)
    }

    /// Adds an account and returns its key.
    pub fn add(&mut self, settings: AccountSettings, now_ms: i64) -> Result<String> {
        let key = fresh_key(now_ms, |key| self.get(key).is_some());
        let account = Account {
            key: key.clone(),
            settings,
            label: None,
            broken: None,
        };
        self.change(|accounts| accounts.push(account))?;
        Ok(key)
    }

    /// Replaces the settings; on a broken account this repairs it.
    pub fn update(&mut self, key: &str, settings: AccountSettings) -> Result<()> {
        let index = self.index(key)?;
        self.change(|accounts| {
            accounts[index].settings = settings;
            accounts[index].broken = None;
        })
    }

    /// Sets the label; blank (or `None`) clears it. Surrounding whitespace is
    /// dropped. A broken entry is written back as read, so it takes none.
    pub fn set_label(&mut self, key: &str, label: Option<&str>) -> Result<()> {
        let index = self.index(key)?;
        if let Some(reason) = &self.accounts[index].broken {
            return Err(bad(
                &self.path,
                format!("account {key} is broken: {reason}"),
            ));
        }
        let label = label
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned);
        self.change(|accounts| accounts[index].label = label)
    }

    /// Removes the settings. The caller also drops the account's cache
    /// (`Db::remove_account_cache`) and its password.
    pub fn remove(&mut self, key: &str) -> Result<()> {
        let index = self.index(key)?;
        self.change(|accounts| {
            accounts.remove(index);
        })
    }

    fn index(&self, key: &str) -> Result<usize> {
        self.accounts
            .iter()
            .position(|account| account.key == key)
            .ok_or(StoreError::NotFound)
    }

    /// Saves an edited copy; memory changes only once the file is written.
    /// Broken entries are written back exactly as they were read.
    fn change(&mut self, edit: impl FnOnce(&mut Vec<Account>)) -> Result<()> {
        let mut next = self.accounts.clone();
        edit(&mut next);
        // Never write a file that `open` would refuse.
        check(&self.path, &next)?;
        let entries = next
            .iter()
            .map(|account| match self.raw.get(&account.key) {
                Some(original) if account.broken.is_some() => original.clone(),
                _ => serde_json::to_value(account).expect("account settings always serialize"),
            })
            .collect();
        let file = FileOut {
            version: VERSION,
            accounts: entries,
        };
        let bytes = serde_json::to_vec_pretty(&file).expect("account settings always serialize");
        write_atomic(&self.path, &bytes)?;
        self.raw
            .retain(|key, _| next.iter().any(|a| &a.key == key && a.broken.is_some()));
        self.accounts = next;
        Ok(())
    }
}

fn bad(path: &Path, reason: String) -> StoreError {
    StoreError::BadRecord {
        file: path.display().to_string(),
        reason,
    }
}

type Parsed = (Vec<Account>, HashMap<String, serde_json::Value>);

fn parse(path: &Path, bytes: &[u8]) -> Result<Parsed> {
    let file: FileIn =
        serde_json::from_slice(bytes).map_err(|error| bad(path, error.to_string()))?;
    if file.version > VERSION {
        return Err(bad(
            path,
            format!("version {} is from a newer envio", file.version),
        ));
    }
    let mut accounts = Vec::new();
    let mut raw = HashMap::new();
    for value in file.accounts {
        // Without a usable key an entry cannot be named, kept or removed.
        let key = value
            .get("key")
            .and_then(|key| key.as_str())
            .filter(|key| is_key(key))
            .ok_or_else(|| bad(path, "an account has no valid key".into()))?
            .to_owned();
        let account = match serde_json::from_value::<Account>(value.clone()) {
            Ok(account) => match pin_problem(&account) {
                None => account,
                Some(reason) => stand_in(&value, key.clone(), reason),
            },
            Err(_) => stand_in(&value, key.clone(), unreadable(&value)),
        };
        if account.broken.is_some() {
            raw.insert(key, value);
        }
        accounts.push(account);
    }
    check(path, &accounts)?;
    Ok((accounts, raw))
}

/// A fixed reason naming which part failed, never a value from the file.
fn unreadable(value: &serde_json::Value) -> String {
    let settings = value.get("settings");
    let server_bad = |which: &str| {
        settings
            .and_then(|settings| settings.get(which))
            .is_none_or(|server| serde_json::from_value::<Server>(server.clone()).is_err())
    };
    if server_bad("imap") {
        "unreadable IMAP server settings".to_owned()
    } else if server_bad("smtp") {
        "unreadable SMTP server settings".to_owned()
    } else {
        "unreadable account settings".to_owned()
    }
}

/// A stand-in for an entry that did not parse: its key and whatever name,
/// email and label it has, empty elsewhere.
fn stand_in(value: &serde_json::Value, key: String, reason: String) -> Account {
    let text = |field: &str| {
        value
            .get("settings")
            .and_then(|settings| settings.get(field))
            .and_then(|field| field.as_str())
            .unwrap_or_default()
            .to_owned()
    };
    let server = || Server {
        host: String::new(),
        port: 0,
        security: Security::default(),
        pin: None,
    };
    Account {
        key,
        settings: AccountSettings {
            name: text("name"),
            email: text("email"),
            username: String::new(),
            imap: server(),
            smtp: server(),
            auth: None,
        },
        label: value
            .get("label")
            .and_then(|label| label.as_str())
            .map(str::to_owned),
        broken: Some(reason),
    }
}

fn pin_problem(account: &Account) -> Option<String> {
    let settings = &account.settings;
    [("imap", &settings.imap.pin), ("smtp", &settings.smtp.pin)]
        .into_iter()
        .find_map(|(which, pin)| {
            pin.as_deref()
                .filter(|pin| !is_pin(pin))
                .map(|_| format!("malformed certificate pin for {which}"))
        })
}

fn check(path: &Path, accounts: &[Account]) -> Result<()> {
    let bad = |reason: String| bad(path, reason);
    let mut seen = std::collections::HashSet::new();
    for account in accounts {
        if !is_key(&account.key) {
            return Err(bad(format!(
                "account key {:?} is not a valid key",
                account.key
            )));
        }
        if !seen.insert(account.key.as_str()) {
            return Err(bad(format!("account key {} appears twice", account.key)));
        }
        if account.broken.is_none() && pin_problem(account).is_some() {
            return Err(bad(format!(
                "account {} has a malformed certificate pin",
                account.key
            )));
        }
    }
    Ok(())
}

fn is_pin(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}
