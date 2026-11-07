//! The Outbox: messages frozen as `.eml` files until the server accepts
//! them. Each item is `<id>.eml` plus `<id>.json` (its state). The `.json`
//! is written last and removed first, so an `.eml` alone is a half-finished
//! enqueue, and the draft it came from still exists.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic::{is_temp, write_atomic};
use crate::error::{Result, StoreError};
use crate::ids::{fresh_key, is_key};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SendState {
    Queued,
    /// Set just before SMTP starts.
    Sending,
    /// Envio stopped while `Sending`. Never resent on its own: core searches
    /// Sent for the Message-ID, otherwise the user chooses.
    MayHaveBeenSent,
    Failed {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxMeta {
    pub account: String,
    pub message_id: String,
    pub queued_at_ms: i64,
    pub state: SendState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewOutboxItem {
    pub account: String,
    pub message_id: String,
    pub queued_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxItem {
    pub id: String,
    pub meta: OutboxMeta,
}

pub struct Outbox {
    dir: PathBuf,
}

impl Outbox {
    /// Opens the Outbox; call once at startup. An item left `Sending` means
    /// envio stopped mid-send, so it becomes `MayHaveBeenSent`.
    pub fn open(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir)?;
        let outbox = Self {
            dir: dir.to_owned(),
        };
        outbox.clear_leftovers()?;
        for item in outbox.items()? {
            if item.meta.state == SendState::Sending {
                outbox.set_state(&item.id, SendState::MayHaveBeenSent)?;
            }
        }
        Ok(outbox)
    }

    /// Freezes `eml` into the Outbox as `Queued`.
    pub fn enqueue(&self, new: NewOutboxItem, eml: &[u8]) -> Result<OutboxItem> {
        let id = fresh_key(new.queued_at_ms, |id| {
            self.path(id, "json").exists() || self.path(id, "eml").exists()
        });
        write_atomic(&self.path(&id, "eml"), eml)?;
        let meta = OutboxMeta {
            account: new.account,
            message_id: new.message_id,
            queued_at_ms: new.queued_at_ms,
            state: SendState::Queued,
        };
        self.write_meta(&id, &meta)?;
        Ok(OutboxItem { id, meta })
    }

    /// Every item, oldest first.
    pub fn items(&self) -> Result<Vec<OutboxItem>> {
        let mut items = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let name = entry?.file_name();
            let Some(id) = name.to_str().and_then(|name| name.strip_suffix(".json")) else {
                continue;
            };
            if is_key(id) {
                items.push(OutboxItem {
                    id: id.to_owned(),
                    meta: self.meta(id)?,
                });
            }
        }
        items.sort_by(|a, b| (a.meta.queued_at_ms, &a.id).cmp(&(b.meta.queued_at_ms, &b.id)));
        Ok(items)
    }

    pub fn eml(&self, id: &str) -> Result<Vec<u8>> {
        check(id)?;
        fs::read(self.path(id, "eml")).map_err(StoreError::missing_or_io)
    }

    pub fn set_state(&self, id: &str, state: SendState) -> Result<()> {
        check(id)?;
        let meta = OutboxMeta {
            state,
            ..self.meta(id)?
        };
        self.write_meta(id, &meta)
    }

    /// Removes an item once the server accepted it, or the user deleted it.
    pub fn remove(&self, id: &str) -> Result<()> {
        check(id)?;
        fs::remove_file(self.path(id, "json")).map_err(StoreError::missing_or_io)?;
        match fs::remove_file(self.path(id, "eml")) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        }
    }

    fn path(&self, id: &str, extension: &str) -> PathBuf {
        self.dir.join(format!("{id}.{extension}"))
    }

    fn meta(&self, id: &str) -> Result<OutboxMeta> {
        let path = self.path(id, "json");
        let bytes = fs::read(&path).map_err(StoreError::missing_or_io)?;
        serde_json::from_slice(&bytes).map_err(|error| StoreError::BadRecord {
            file: path.display().to_string(),
            reason: error.to_string(),
        })
    }

    fn write_meta(&self, id: &str, meta: &OutboxMeta) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(meta).expect("outbox state always serializes");
        Ok(write_atomic(&self.path(id, "json"), &bytes)?)
    }

    /// Deletes interrupted writes and `.eml` files without a `.json`.
    fn clear_leftovers(&self) -> Result<()> {
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let orphan = name
                .strip_suffix(".eml")
                .is_some_and(|id| is_key(id) && !self.path(id, "json").exists());
            if is_temp(&name) || orphan {
                fs::remove_file(entry.path())?;
            }
        }
        Ok(())
    }
}

fn check(id: &str) -> Result<()> {
    if is_key(id) {
        Ok(())
    } else {
        Err(StoreError::NotFound)
    }
}
