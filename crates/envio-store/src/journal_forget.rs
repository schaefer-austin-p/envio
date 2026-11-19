//! Forget intents: the Delete and Move actions that reached the server and
//! whose local draft file is still to be removed (#304). A sidecar of JSON
//! lines beside the journal, written before the action is marked done, so a
//! crash or a failed run between the commit and the forget leaves the intent
//! for the next run. An intent whose action is still pending is not due.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::atomic::write_atomic;
use crate::error::Result;
use crate::journal::Action;
use crate::private::open_private_append;

pub(crate) struct Intents {
    path: PathBuf,
    actions: Vec<Action>,
}

impl Intents {
    /// Reads the sidecar; a line that is torn or unreadable is dropped.
    pub fn open(journal: &Path) -> Result<Self> {
        let mut name = journal.as_os_str().to_owned();
        name.push(".forget");
        let path = PathBuf::from(name);
        let actions = match fs::read(&path) {
            Ok(bytes) => bytes
                .split(|&byte| byte == b'\n')
                .filter_map(|line| serde_json::from_slice(line).ok())
                .collect(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self { path, actions })
    }

    pub fn all(&self) -> &[Action] {
        &self.actions
    }

    /// Records an intent durably (once per action).
    pub fn add(&mut self, action: &Action) -> Result<()> {
        if self.actions.iter().any(|a| a.seq == action.seq) {
            return Ok(());
        }
        let mut line = serde_json::to_vec(action).expect("actions always serialize");
        line.push(b'\n');
        let mut file = open_private_append(&self.path)?;
        file.write_all(&line)?;
        file.sync_data()?;
        self.actions.push(action.clone());
        Ok(())
    }

    /// Drops the intents with these sequence numbers.
    pub fn settle(&mut self, seqs: &[u64]) -> Result<()> {
        let kept: Vec<&Action> = self
            .actions
            .iter()
            .filter(|a| !seqs.contains(&a.seq))
            .collect();
        if kept.len() == self.actions.len() {
            return Ok(());
        }
        if kept.is_empty() {
            match fs::remove_file(&self.path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        } else {
            let mut bytes = Vec::new();
            for action in &kept {
                bytes.extend(serde_json::to_vec(action).expect("actions always serialize"));
                bytes.push(b'\n');
            }
            write_atomic(&self.path, &bytes)?;
        }
        self.actions.retain(|a| !seqs.contains(&a.seq));
        Ok(())
    }
}
