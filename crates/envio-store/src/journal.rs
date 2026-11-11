//! The action journal: user actions waiting to reach the server, as
//! append-only lines of JSON in the data folder, so a rebuilt cache can't
//! lose them. Each action is added once and later marked done; what was
//! added and not done is pending. A torn last line (a crash mid-write) is
//! dropped and the file rewritten, so the next line can't join onto it.
//! Records carry a version (`v`, absent means 1); a record this build can't
//! read (newer version, unknown action kind) is skipped and reported by
//! `skipped`, and stays on disk so a newer build can still replay it.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic::write_atomic;
use crate::error::{Result, StoreError};
use crate::journal_replay::{Replay, Skipped, replay};
use crate::messages::Flags;

/// Names a message without cache row ids, so it still resolves after a
/// rebuild; after a UIDVALIDITY change, by Message-ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageRef {
    /// The account's key (see `Account::key`).
    pub account: String,
    /// The folder's path on the server.
    pub folder: String,
    pub uidvalidity: u32,
    pub uid: u32,
    pub message_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionKind {
    SetFlags {
        add: Flags,
        remove: Flags,
    },
    /// Move to the folder with this path.
    Move {
        to: String,
    },
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    pub seq: u64,
    /// When the user did it, in unix milliseconds (the Undo window starts here).
    pub at_ms: i64,
    pub target: MessageRef,
    pub kind: ActionKind,
}

/// The record version this build writes and reads.
pub(crate) const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub(crate) enum Record {
    Add {
        action: Action,
    },
    Done {
        seq: u64,
    },
    /// Written by `compact` so sequence numbers are never reused.
    Next {
        seq: u64,
    },
}

pub struct Journal {
    path: PathBuf,
    file: File,
    next_seq: u64,
    pending: BTreeMap<u64, Action>,
    /// An append failed part-way; rewrite before appending again.
    needs_compact: bool,
    /// Records this build can't read; kept on disk, never replayed.
    skipped: Vec<Skipped>,
    /// Test seam: the next sync fails after the line is written.
    fail_next_sync: bool,
}

impl Journal {
    pub fn open(path: &Path) -> Result<Self> {
        let existed = path.exists();
        if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            fs::create_dir_all(dir)?;
        }
        let replay = match fs::read(path) {
            Ok(bytes) => replay(path, &bytes)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Replay::default(),
            Err(error) => return Err(error.into()),
        };
        let journal = Self {
            path: path.to_owned(),
            file: open_append(path)?,
            next_seq: replay.next_seq,
            pending: replay.pending,
            needs_compact: replay.torn,
            skipped: replay.skipped,
            fail_next_sync: false,
        };
        if !existed && let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            File::open(dir)?.sync_all()?;
        }
        // Opening never writes: with a torn tail, `needs_compact` is set and
        // the next push or done rewrites the file first. A full disk can't
        // block startup that way.
        Ok(journal)
    }

    /// Records a new action and returns it with its sequence number. An
    /// error for which `may_still_apply` is true means the line reached the
    /// file but could not be made durable: it may replay after a crash.
    pub fn push(&mut self, at_ms: i64, target: MessageRef, kind: ActionKind) -> Result<Action> {
        let action = Action {
            seq: self.next_seq,
            at_ms,
            target,
            kind,
        };
        let written = self.append(&Record::Add {
            action: action.clone(),
        });
        // The line may be on disk even when the sync failed: that seq is
        // spent, so the next action never shares it.
        if matches!(&written, Ok(()) | Err(StoreError::MayStillApply(_))) {
            self.next_seq += 1;
        }
        written?;
        self.pending.insert(action.seq, action.clone());
        Ok(action)
    }

    /// Marks an action done: it reached the server, was undone, or was given up.
    pub fn done(&mut self, seq: u64) -> Result<()> {
        if !self.pending.contains_key(&seq) {
            return Err(StoreError::NotFound);
        }
        // On MayStillApply the done line may replay (Undo says "may still be
        // undone"); the action stays pending in memory.
        self.append(&Record::Done { seq })?;
        self.pending.remove(&seq);
        Ok(())
    }

    /// Pending actions, oldest first.
    pub fn pending(&self) -> impl Iterator<Item = &Action> {
        self.pending.values()
    }

    /// Records this build skipped on open (a newer Envio wrote them). They
    /// stay in the file; the list holds no message content.
    pub fn skipped(&self) -> &[Skipped] {
        &self.skipped
    }

    /// Test seam: makes the next append's sync fail after the write.
    #[doc(hidden)]
    pub fn fail_next_sync(&mut self) {
        self.fail_next_sync = true;
    }

    /// Rewrites the file with only the pending actions (e.g. at startup).
    pub fn compact(&mut self) -> Result<()> {
        let mut bytes = encode(&Record::Next { seq: self.next_seq });
        // Skipped lines and pending adds in sequence order, as they were.
        let mut lines: Vec<(u64, Vec<u8>)> = self
            .skipped
            .iter()
            .map(|skipped| {
                let mut raw = skipped.raw.clone();
                raw.push(b'\n');
                (skipped.seq.unwrap_or(0), raw)
            })
            .chain(self.pending.values().map(|action| {
                let record = Record::Add {
                    action: action.clone(),
                };
                (action.seq, encode(&record))
            }))
            .collect();
        lines.sort_by_key(|(seq, _)| *seq);
        for (_, line) in lines {
            bytes.extend(line);
        }
        // Stays set until the new file is open: if the rename lands but a later
        // step fails, `self.file` is the old unlinked inode and must not be used.
        self.needs_compact = true;
        write_atomic(&self.path, &bytes)?;
        self.file = open_append(&self.path)?;
        self.needs_compact = false;
        Ok(())
    }

    /// If the write or sync fails, the file may hold a torn or unacknowledged
    /// record, so the next call rewrites it from memory first; memory never
    /// holds a record that was not acknowledged, so it cannot replay later.
    fn append(&mut self, record: &Record) -> Result<()> {
        if self.needs_compact {
            self.compact()?;
        }
        if let Err(error) = self.file.write_all(&encode(record)) {
            // No complete line can be in the file: a short write has no
            // newline, so it is torn and dropped. This is a plain failure.
            self.needs_compact = true;
            return Err(error.into());
        }
        let synced = if std::mem::take(&mut self.fail_next_sync) {
            Err(io::Error::other("injected sync failure"))
        } else {
            self.file.sync_data()
        };
        if let Err(error) = synced {
            // The whole line is written but not known durable.
            self.needs_compact = true;
            return Err(StoreError::MayStillApply(error));
        }
        Ok(())
    }
}

fn open_append(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

#[derive(Serialize)]
struct Line<'a> {
    v: u32,
    #[serde(flatten)]
    record: &'a Record,
}

fn encode(record: &Record) -> Vec<u8> {
    let mut line =
        serde_json::to_vec(&Line { v: VERSION, record }).expect("journal records always serialize");
    line.push(b'\n');
    line
}
