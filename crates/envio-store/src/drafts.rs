//! Drafts: what compose holds, one JSON file per draft in the data folder,
//! replaced atomically on every save. Core debounces saves to about 1 s.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic::{is_temp, write_atomic};
use crate::error::{Result, StoreError};
use crate::ids::{fresh_key, is_key};

/// Missing fields load as empty, so older and newer drafts both open.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Draft {
    pub account: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub updated_at_ms: i64,
}

pub struct Drafts {
    dir: PathBuf,
}

impl Drafts {
    /// Opens the drafts folder and clears interrupted saves.
    pub fn open(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir)?;
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if entry.file_name().to_str().is_some_and(is_temp) {
                fs::remove_file(entry.path())?;
            }
        }
        Ok(Self {
            dir: dir.to_owned(),
        })
    }

    /// Saves a new draft and returns its id.
    pub fn create(&self, draft: &Draft) -> Result<String> {
        let id = fresh_key(draft.updated_at_ms, |id| self.path(id).exists());
        self.save(&id, draft)?;
        Ok(id)
    }

    pub fn save(&self, id: &str, draft: &Draft) -> Result<()> {
        check(id)?;
        let bytes = serde_json::to_vec_pretty(draft).expect("drafts always serialize");
        Ok(write_atomic(&self.path(id), &bytes)?)
    }

    pub fn load(&self, id: &str) -> Result<Draft> {
        check(id)?;
        let path = self.path(id);
        let bytes = fs::read(&path).map_err(StoreError::missing_or_io)?;
        serde_json::from_slice(&bytes).map_err(|error| StoreError::BadRecord {
            file: path.display().to_string(),
            reason: error.to_string(),
        })
    }

    /// Every draft, most recently changed first.
    pub fn list(&self) -> Result<Vec<(String, Draft)>> {
        let mut drafts = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let name = entry?.file_name();
            let Some(id) = name.to_str().and_then(|name| name.strip_suffix(".json")) else {
                continue;
            };
            if is_key(id) {
                drafts.push((id.to_owned(), self.load(id)?));
            }
        }
        drafts
            .sort_by(|(a_id, a), (b_id, b)| (b.updated_at_ms, b_id).cmp(&(a.updated_at_ms, a_id)));
        Ok(drafts)
    }

    /// Deletes a draft (sent, or discarded by the user).
    pub fn delete(&self, id: &str) -> Result<()> {
        check(id)?;
        fs::remove_file(self.path(id)).map_err(StoreError::missing_or_io)
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }
}

fn check(id: &str) -> Result<()> {
    if is_key(id) {
        Ok(())
    } else {
        Err(StoreError::NotFound)
    }
}
