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

    pub fn accounts(&self) -> PathBuf {
        self.data.join("accounts.json")
    }

    pub fn journal(&self) -> PathBuf {
        self.data.join("actions.journal")
    }

    pub fn outbox(&self) -> PathBuf {
        self.data.join("outbox")
    }

    pub fn drafts(&self) -> PathBuf {
        self.data.join("drafts")
    }
}
