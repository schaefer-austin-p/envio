//! The action journal: user actions waiting to reach the server, as
//! append-only lines of JSON in the data folder, so a rebuilt cache can't
//! lose them. Each action is added once and later marked done; what was
//! added and not done is pending. A torn last line (a crash mid-write) is
//! dropped and the file rewritten, so the next line can't join onto it.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic::write_atomic;
use crate::error::{Result, StoreError};
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

#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Record {
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
        };
        if !existed && let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            File::open(dir)?.sync_all()?;
        }
        // Opening never writes: with a torn tail, `needs_compact` is set and
        // the next push or done rewrites the file first. A full disk can't
        // block startup that way.
        Ok(journal)
    }

    /// Records a new action and returns it with its sequence number.
    pub fn push(&mut self, at_ms: i64, target: MessageRef, kind: ActionKind) -> Result<Action> {
        let action = Action {
            seq: self.next_seq,
            at_ms,
            target,
            kind,
        };
        self.append(&Record::Add {
            action: action.clone(),
        })?;
        self.next_seq += 1;
        self.pending.insert(action.seq, action.clone());
        Ok(action)
    }

    /// Marks an action done: it reached the server, was undone, or was given up.
    pub fn done(&mut self, seq: u64) -> Result<()> {
        if !self.pending.contains_key(&seq) {
            return Err(StoreError::NotFound);
        }
        self.append(&Record::Done { seq })?;
        self.pending.remove(&seq);
        Ok(())
    }

    /// Pending actions, oldest first.
    pub fn pending(&self) -> impl Iterator<Item = &Action> {
        self.pending.values()
    }

    /// Rewrites the file with only the pending actions (e.g. at startup).
    pub fn compact(&mut self) -> Result<()> {
        let mut bytes = encode(&Record::Next { seq: self.next_seq });
        for action in self.pending.values() {
            bytes.extend(encode(&Record::Add {
                action: action.clone(),
            }));
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
        let written = self
            .file
            .write_all(&encode(record))
            .and_then(|()| self.file.sync_data());
        if written.is_err() {
            self.needs_compact = true;
        }
        Ok(written?)
    }
}

fn open_append(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

fn encode(record: &Record) -> Vec<u8> {
    let mut line = serde_json::to_vec(record).expect("journal records always serialize");
    line.push(b'\n');
    line
}

struct Replay {
    pending: BTreeMap<u64, Action>,
    next_seq: u64,
    torn: bool,
}

impl Default for Replay {
    fn default() -> Self {
        Self {
            pending: BTreeMap::new(),
            next_seq: 1,
            torn: false,
        }
    }
}

impl Replay {
    fn apply(&mut self, record: Record) {
        match record {
            Record::Add { action } => {
                self.next_seq = self.next_seq.max(action.seq + 1);
                self.pending.insert(action.seq, action);
            }
            Record::Done { seq } => {
                self.pending.remove(&seq);
            }
            Record::Next { seq } => self.next_seq = self.next_seq.max(seq),
        }
    }
}

fn replay(path: &Path, bytes: &[u8]) -> Result<Replay> {
    let complete = match bytes.iter().rposition(|&byte| byte == b'\n') {
        Some(end) => &bytes[..=end],
        None => &[][..],
    };
    let mut replay = Replay {
        torn: complete.len() != bytes.len(),
        ..Replay::default()
    };
    for (index, line) in complete.split(|&byte| byte == b'\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        let record = serde_json::from_slice(line).map_err(|error| StoreError::BadRecord {
            file: format!("{}:{}", path.display(), index + 1),
            reason: error.to_string(),
        })?;
        replay.apply(record);
    }
    Ok(replay)
}
