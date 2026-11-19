//! A draft's copy in the server's Drafts folder: what identifies it, and
//! the notes that keep a deletion waiting until the server has heard of it.
//! All of it lives with the drafts, outside the cache.

use std::fs;
use std::io;

use serde::{Deserialize, Serialize};

use crate::atomic::write_atomic;
use crate::drafts::{Draft, Drafts};
use crate::error::{Result, StoreError};
use crate::ids::{fresh_key, is_key};

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
    /// When an upload of this draft last ended the run (a lost
    /// connection): the next run tries the others first.
    pub tried_ms: i64,
    /// For a draft imported as a copy of a server message (#228): the
    /// usable Message-ID of that original, so a second Edit reopens the
    /// copy. The sync planner and `replaced()` ignore it.
    pub imported_from: Option<String>,
    /// The Message-IDs this draft had on the server before the current one
    /// (each upload replaces the copy with a new id), oldest first, at
    /// most `EARLIER_MAX`. A stale cached row of an older copy still finds
    /// the draft by one of them.
    pub earlier: Vec<String>,
    /// An imported copy that does not stand in for its original (set when
    /// it is imported, never derived from the bounded `earlier` list): the
    /// original stays on the server, and Edit says so each time.
    pub detached: bool,
    /// An import whose files are not all in yet. The sync planner leaves
    /// it alone and Edit discards it and imports again.
    pub incomplete: bool,
}

/// How many earlier Message-IDs a draft remembers.
pub const EARLIER_MAX: usize = 8;

impl ServerCopy {
    /// Whether `id` (trimmed) is any Message-ID this draft has or had on
    /// the server, or the one it was imported from.
    #[must_use]
    pub fn names(&self, id: &str) -> bool {
        [&self.message_id, &self.pending_id, &self.imported_from]
            .into_iter()
            .flatten()
            .chain(self.earlier.iter())
            .any(|own| own.trim() == id.trim())
    }

    /// An imported copy that does not stand in for its original: the
    /// original is not (and never was) this draft's server copy.
    #[must_use]
    pub fn is_detached_copy(&self) -> bool {
        self.detached
    }
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

    /// The server copy `message_id` of this draft was deleted by the user
    /// (and the server has it): removes the draft file and its files, and
    /// writes no note, since the copy is already gone and a note would
    /// expunge a copy the user restores later. Checked and done under the
    /// lock: false, with nothing removed, when the draft is gone, names
    /// another copy, or has a save the copy lacks (unsent work stays).
    pub fn forget_if_uploaded(&self, id: &str, message_id: &str) -> Result<bool> {
        if !is_key(id) {
            return Err(StoreError::NotFound);
        }
        let _held = self.held();
        let draft = match self.load(id) {
            Ok(draft) => draft,
            Err(StoreError::NotFound) => return Ok(false),
            Err(error) => return Err(error),
        };
        let server = &draft.server;
        if server.message_id.as_deref() != Some(message_id)
            || server.uploaded_ms != draft.updated_at_ms
        {
            return Ok(false);
        }
        self.remove_locked(id)?;
        Ok(true)
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
            // `imported_from` stays: the original has still one local copy.
            // The copy this one replaces joins `earlier`.
            let old = std::mem::take(&mut draft.server);
            let mut earlier = old.earlier;
            if let Some(previous) = old.message_id.filter(|p| p.trim() != message_id.trim()) {
                earlier.retain(|e| e.trim() != previous.trim());
                earlier.push(previous);
            }
            let excess = earlier.len().saturating_sub(EARLIER_MAX);
            earlier.drain(..excess);
            draft.server = ServerCopy {
                message_id: Some(message_id.to_owned()),
                pending_id: None,
                uploaded_ms,
                imported_from: old.imported_from,
                detached: old.detached,
                earlier,
                ..ServerCopy::default()
            };
        })
    }

    /// Like `create`, but writes `draft.server` as given instead of the
    /// "never uploaded" default. For a draft imported from the server.
    pub fn create_imported(&self, draft: &Draft) -> Result<String> {
        let _held = self.held();
        let id = fresh_key(draft.updated_at_ms, |id| self.path(id).exists());
        self.write(&id, draft)?;
        Ok(id)
    }

    /// Imports a server draft as a local one, all or nothing. `draft` is
    /// written first as a plain copy (`draft.server` must not name the
    /// original as its server copy), `add_files` then puts the files in
    /// (it may change the draft's attachments; they are saved), and only
    /// when everything is in place is the copy linked: `link` is the
    /// original's Message-ID, which the draft then stands in for. So a
    /// crash or a failure before the end can never leave a linked draft
    /// that lacks files its original has: the first change of a linked
    /// draft replaces (expunges) the original. Any failure takes the new
    /// draft back (no note). The id, and whether the link was made (not
    /// when something touched the draft first).
    pub fn import(
        &self,
        draft: &Draft,
        link: Option<&str>,
        add_files: impl FnOnce(&Self, &str, &mut Draft) -> Result<()>,
    ) -> Result<(String, bool)> {
        let mut first = draft.clone();
        first.server.detached = true;
        first.server.incomplete = true;
        let id = self.create_imported(&first)?;
        let done = (|| {
            let mut filled = draft.clone();
            add_files(self, &id, &mut filled)?;
            if filled.attachments != draft.attachments {
                // `save` keeps the server copy and `updated_at_ms` as written.
                self.save(&id, &filled)?;
            }
            self.finish_imported(&id, link)
        })();
        match done {
            Ok(linked) => Ok((id, linked)),
            Err(error) => {
                if let Err(again) = self.remove_imported(&id) {
                    log::warn!("taking back a half-imported draft {id}: {again}");
                }
                Err(error)
            }
        }
    }

    /// Ends an import: the draft is complete from now on, and when `link`
    /// (the original's Message-ID) is given it stands in for the original
    /// (`uploaded_ms` becomes its `updated_at_ms`, so it is not uploaded
    /// until changed) and is no longer detached. Whether the link was
    /// made: false when the draft is gone or an upload touched it meanwhile.
    fn finish_imported(&self, id: &str, link: Option<&str>) -> Result<bool> {
        if !is_key(id) {
            return Err(StoreError::NotFound);
        }
        let _held = self.held();
        let mut draft = match self.load(id) {
            Ok(draft) => draft,
            Err(StoreError::NotFound) => return Ok(false),
            Err(error) => return Err(error),
        };
        let server = &mut draft.server;
        server.incomplete = false;
        let untouched =
            server.message_id.is_none() && server.pending_id.is_none() && server.uploaded_ms == 0;
        let linked = match link {
            Some(original) if untouched => {
                server.message_id = Some(original.to_owned());
                server.uploaded_ms = draft.updated_at_ms;
                server.detached = false;
                true
            }
            _ => false,
        };
        self.write(id, &draft)?;
        Ok(linked)
    }

    /// Takes back a draft `create_imported` just made, when the import
    /// fails halfway. Unlike `delete` it leaves no note: the server copy
    /// the draft names is the original message, which must stay.
    pub fn remove_imported(&self, id: &str) -> Result<()> {
        if !is_key(id) {
            return Err(StoreError::NotFound);
        }
        let _held = self.held();
        self.remove_locked(id)
    }

    /// The id of a damaged draft file that still names `message_id` as its
    /// server copy (or its origin), in `account`. Such a file cannot be
    /// opened, but it is unsent work, so a second copy of the same message
    /// must not be made beside it.
    pub fn damaged_naming(&self, account: &str, message_id: &str) -> Result<Option<String>> {
        for id in self.damaged()? {
            let Some((owner, server)) = self.salvage_fields(&id) else {
                continue;
            };
            if owner == account && server.names(message_id) {
                return Ok(Some(id));
            }
        }
        Ok(None)
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

    /// An upload of this draft ended the run at `at_ms` (not counted as a
    /// failure). False when the draft is gone.
    pub fn mark_tried(&self, id: &str, at_ms: i64) -> Result<bool> {
        self.edit(id, |draft| draft.server.tried_ms = at_ms)
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
