//! A draft's copy in the server's Drafts folder: what identifies it, and
//! the notes that keep a deletion waiting until the server has heard of it.
//! All of it lives with the drafts, outside the cache.

use std::fs;
use std::io;

use serde::{Deserialize, Serialize};

use crate::atomic::write_atomic;
use crate::drafts::{Draft, Drafts};
use crate::error::{Result, StoreError};
use crate::ids::is_key;

/// Kept in the draft file. Old files without it load as "never uploaded".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerCopy {
    /// Message-ID (with angle brackets) of the copy the server holds.
    pub message_id: Option<String>,
    /// The Message-ID of an upload that has begun and not been confirmed:
    /// its APPEND may have reached the server, so a retry looks for it
    /// before it appends again.
    pub pending_id: Option<String>,
    /// The `updated_at_ms` of the draft text the server copy has.
    pub uploaded_ms: i64,
    /// The `updated_at_ms` of the save whose upload last failed, and how
    /// many times in a row: the planner stops retrying that save.
    pub failed_ms: i64,
    pub failures: u32,
}

/// A deleted draft whose copy on the server is still to go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerDelete {
    /// The draft's id; names the note.
    pub id: String,
    pub account: String,
    /// Every Message-ID the draft may have on the server.
    pub message_ids: Vec<String>,
}

const NOTE: &str = "gone";

impl Drafts {
    fn note_file(&self, id: &str) -> std::path::PathBuf {
        self.dir.join(format!("{id}.{NOTE}"))
    }

    /// Writes the deletion note for `id`'s server copy, if it has one.
    /// Called by `delete` with the lock held.
    pub(crate) fn note_server_copy(&self, id: &str) -> Result<()> {
        let (account, server) = match self.load(id) {
            Ok(draft) => (draft.account, draft.server),
            // A damaged file still names its server copy, usually.
            Err(StoreError::BadRecord { .. }) => match self.salvage_fields(id) {
                Some(found) => found,
                None => {
                    log::warn!(
                        "draft {id}: the file is damaged and names no server copy; \
                         a copy in Drafts, if any, stays there"
                    );
                    return Ok(());
                }
            },
            Err(_) => return Ok(()),
        };
        let message_ids: Vec<String> = [&server.message_id, &server.pending_id]
            .into_iter()
            .flatten()
            .cloned()
            .collect();
        if message_ids.is_empty() {
            return Ok(());
        }
        let note = ServerDelete {
            id: id.to_owned(),
            account,
            message_ids,
        };
        let bytes = serde_json::to_vec(&note).map_err(|error| StoreError::BadRecord {
            file: self.note_file(id).display().to_string(),
            reason: error.to_string(),
        })?;
        Ok(write_atomic(&self.note_file(id), &bytes)?)
    }

    /// Server copies of deleted drafts that still wait to be deleted, by
    /// draft id. An unreadable note is skipped.
    pub fn deletes(&self) -> Result<Vec<ServerDelete>> {
        let mut notes = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let name = entry?.file_name();
            let Some(id) = name
                .to_str()
                .and_then(|name| name.strip_suffix(&format!(".{NOTE}")))
            else {
                continue;
            };
            if !is_key(id) {
                continue;
            }
            let read = fs::read(self.dir.join(&name)).ok();
            if let Some(note) = read.and_then(|bytes| serde_json::from_slice(&bytes).ok()) {
                notes.push(note);
            }
        }
        notes.sort_by(|a: &ServerDelete, b| a.id.cmp(&b.id));
        Ok(notes)
    }

    /// The server copy is gone (or cannot be reached any more): drops the note.
    pub fn deleted_on_server(&self, id: &str) -> Result<()> {
        if !is_key(id) {
            return Err(StoreError::NotFound);
        }
        match fs::remove_file(self.note_file(id)) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        }
    }

    /// Drops every note of `account` (the account is being removed).
    pub fn forget_deletes(&self, account: &str) -> Result<()> {
        for note in self.deletes()? {
            if note.account == account {
                self.deleted_on_server(&note.id)?;
            }
        }
        Ok(())
    }

    /// An upload is about to begin with `message_id`. False when the draft
    /// is gone.
    pub fn mark_uploading(&self, id: &str, message_id: &str) -> Result<bool> {
        self.edit(id, |draft| {
            draft.server.pending_id = Some(message_id.to_owned());
        })
    }

    /// The server has the draft text stamped `uploaded_ms` as `message_id`.
    /// False when the draft is gone meanwhile (the caller deletes the copy).
    pub fn mark_uploaded(&self, id: &str, message_id: &str, uploaded_ms: i64) -> Result<bool> {
        self.edit(id, |draft| {
            draft.server = ServerCopy {
                message_id: Some(message_id.to_owned()),
                pending_id: None,
                uploaded_ms,
                ..ServerCopy::default()
            };
        })
    }

    /// The upload of the save stamped `saved_ms` failed: counts `weight`
    /// more failures of that save (a newer save starts again from zero).
    /// False when the draft is gone.
    pub fn mark_failed(&self, id: &str, saved_ms: i64, weight: u32) -> Result<bool> {
        self.edit(id, |draft| {
            let server = &mut draft.server;
            let before = if server.failed_ms == saved_ms {
                server.failures
            } else {
                0
            };
            server.failed_ms = saved_ms;
            server.failures = before.saturating_add(weight);
        })
    }

    fn edit(&self, id: &str, change: impl FnOnce(&mut Draft)) -> Result<bool> {
        if !is_key(id) {
            return Err(StoreError::NotFound);
        }
        let _held = self.held();
        let mut draft = match self.load(id) {
            Ok(draft) => draft,
            Err(StoreError::NotFound) => return Ok(false),
            Err(error) => return Err(error),
        };
        change(&mut draft);
        self.write(id, &draft)?;
        Ok(true)
    }
}
