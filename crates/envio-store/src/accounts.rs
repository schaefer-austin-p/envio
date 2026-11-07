//! Account settings in `accounts.json` in the data folder, not in the cache,
//! so a rebuilt cache keeps them. Passwords are never here (Plan 6 keeps
//! them in their own 0600 file).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic::write_atomic;
use crate::error::{Result, StoreError};
use crate::ids::fresh_key;

const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Server {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSettings {
    /// The name shown on sent mail.
    pub name: String,
    pub email: String,
    pub username: String,
    pub imap: Server,
    pub smtp: Server,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    /// Never changes; the cache, journal and Outbox name the account by it.
    pub key: String,
    pub settings: AccountSettings,
}

#[derive(Deserialize)]
struct FileIn {
    version: u32,
    accounts: Vec<Account>,
}

#[derive(Serialize)]
struct FileOut<'a> {
    version: u32,
    accounts: &'a [Account],
}

pub struct Accounts {
    path: PathBuf,
    accounts: Vec<Account>,
}

impl Accounts {
    /// Reads `path`; a missing file means no accounts yet.
    pub fn open(path: &Path) -> Result<Self> {
        let accounts = match fs::read(path) {
            Ok(bytes) => parse(path, &bytes)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            path: path.to_owned(),
            accounts,
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
        };
        self.change(|accounts| accounts.push(account))?;
        Ok(key)
    }

    pub fn update(&mut self, key: &str, settings: AccountSettings) -> Result<()> {
        let index = self.index(key)?;
        self.change(|accounts| accounts[index].settings = settings)
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
    fn change(&mut self, edit: impl FnOnce(&mut Vec<Account>)) -> Result<()> {
        let mut next = self.accounts.clone();
        edit(&mut next);
        let file = FileOut {
            version: VERSION,
            accounts: &next,
        };
        let bytes = serde_json::to_vec_pretty(&file).expect("account settings always serialize");
        write_atomic(&self.path, &bytes)?;
        self.accounts = next;
        Ok(())
    }
}

fn parse(path: &Path, bytes: &[u8]) -> Result<Vec<Account>> {
    let bad = |reason: String| StoreError::BadRecord {
        file: path.display().to_string(),
        reason,
    };
    let file: FileIn = serde_json::from_slice(bytes).map_err(|error| bad(error.to_string()))?;
    if file.version > VERSION {
        return Err(bad(format!(
            "version {} is from a newer envio",
            file.version
        )));
    }
    Ok(file.accounts)
}
