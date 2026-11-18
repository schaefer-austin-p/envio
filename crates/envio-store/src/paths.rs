//! Where the store's files live. The cache folder (database, bodies) can be
//! deleted and rebuilt from the server; the data folder holds what isn't on
//! the server yet, so clearing the cache can't lose it.

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorePaths {
    cache: PathBuf,
    data: PathBuf,
}

impl StorePaths {
    /// On the phone: `~/.cache/envio.aps` and `~/.local/share/envio.aps`.
    pub fn new(cache: impl Into<PathBuf>, data: impl Into<PathBuf>) -> Self {
        Self {
            cache: cache.into(),
            data: data.into(),
        }
    }

    pub fn db(&self) -> PathBuf {
        self.cache.join("envio.db")
    }

    pub fn bodies(&self) -> PathBuf {
        self.cache.join("bodies")
    }

    /// Inline (`cid:`) images of opened messages, `<dir>/<row id>/<n>.<ext>`.
    /// Rewritten on every open and emptied at start.
    pub fn inline(&self) -> PathBuf {
        self.cache.join("inline")
    }

    pub fn accounts(&self) -> PathBuf {
        self.data.join("accounts.json")
    }

    pub fn journal(&self) -> PathBuf {
        self.data.join("actions.journal")
    }

    pub fn outbox(&self) -> PathBuf {
        self.data.join("outbox")
    }

    pub fn settings(&self) -> PathBuf {
        self.data.join("settings.json")
    }

    pub fn contacts(&self) -> PathBuf {
        self.data.join("contacts.json")
    }

    pub fn invites(&self) -> PathBuf {
        self.data.join("invites.json")
    }

    /// Copies of attached files, one folder per draft id. Inside the drafts
    /// folder but not a `.json` file, so draft scans ignore it.
    pub fn draft_files(&self) -> PathBuf {
        self.drafts().join("files")
    }

    pub fn drafts(&self) -> PathBuf {
        self.data.join("drafts")
    }
}
