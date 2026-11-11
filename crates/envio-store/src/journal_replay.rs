//! Reading the journal file back: version check, skipping what this build
//! can't read, and the torn-tail rule. Pure; no file access.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::error::{Result, StoreError};
use crate::journal::{Action, Record, VERSION};

/// Why a record was left out of replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// An action kind a newer Envio added.
    UnknownKind,
    /// A record version newer than this build reads.
    UnknownVersion(u32),
}

/// A record left out of replay and kept on disk. Holds no message content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    /// Line number in the file, from 1.
    pub line: usize,
    /// The action's sequence number, when the line shows one.
    pub seq: Option<u64>,
    pub reason: SkipReason,
    pub(crate) raw: Vec<u8>,
}

pub(crate) struct Replay {
    pub pending: BTreeMap<u64, Action>,
    pub next_seq: u64,
    pub torn: bool,
    pub skipped: Vec<Skipped>,
}

impl Default for Replay {
    fn default() -> Self {
        Self {
            pending: BTreeMap::new(),
            next_seq: 1,
            torn: false,
            skipped: Vec::new(),
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
                self.skipped.retain(|skipped| skipped.seq != Some(seq));
            }
            Record::Next { seq } => self.next_seq = self.next_seq.max(seq),
        }
    }
}

const KINDS: [&str; 3] = ["set_flags", "move", "delete"];

fn skip_reason(value: &Value) -> Option<SkipReason> {
    let version = value.get("v").map_or(Some(1), Value::as_u64)?;
    if version > u64::from(VERSION) {
        return Some(SkipReason::UnknownVersion(
            u32::try_from(version).unwrap_or(u32::MAX),
        ));
    }
    let kind = value.pointer("/action/kind/kind").and_then(Value::as_str)?;
    (value.get("op").and_then(Value::as_str) == Some("add") && !KINDS.contains(&kind))
        .then_some(SkipReason::UnknownKind)
}

pub(crate) fn replay(path: &Path, bytes: &[u8]) -> Result<Replay> {
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
        let bad = |reason: String| StoreError::BadRecord {
            file: format!("{}:{}", path.display(), index + 1),
            reason,
        };
        let value: Value = serde_json::from_slice(line).map_err(|error| bad(error.to_string()))?;
        // Format invariant: a done line means "this seq is finished" in every
        // version, and a next line only raises the counter, so both apply
        // whatever their version. Only add lines can be skipped.
        let seq = value.get("seq").and_then(Value::as_u64);
        match (value.get("op").and_then(Value::as_str), seq) {
            (Some("done"), Some(seq)) => {
                replay.apply(Record::Done { seq });
                continue;
            }
            (Some("next"), Some(seq)) => {
                replay.apply(Record::Next { seq });
                continue;
            }
            _ => {}
        }
        if let Some(reason) = skip_reason(&value) {
            let seq = value.pointer("/action/seq").and_then(Value::as_u64);
            if let Some(seq) = seq {
                replay.next_seq = replay.next_seq.max(seq.saturating_add(1));
            }
            replay.skipped.push(Skipped {
                line: index + 1,
                seq,
                reason,
                raw: line.to_vec(),
            });
            continue;
        }
        let record = serde_json::from_value(value).map_err(|error| bad(error.to_string()))?;
        replay.apply(record);
    }
    Ok(replay)
}
