//! Forget intents: the Delete and Move actions that reached the server and
//! whose local draft file is still to be removed (#304). A sidecar of JSON
//! lines beside the journal, written before the action is marked done, so a
//! crash or a failed run between the commit and the forget leaves the intent
//! for the next run. An intent whose action is still pending is not due.

use std::fs::{self, File};
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
        let (actions, clean) = match fs::read(&path) {
            Ok(bytes) => {
                let lines: Vec<&[u8]> = bytes
                    .split(|&byte| byte == b'\n')
                    .filter(|line| !line.is_empty())
                    .collect();
                let actions: Vec<Action> = lines
                    .iter()
                    .filter_map(|line| serde_json::from_slice(line).ok())
                    .collect();
                let clean =
                    actions.len() == lines.len() && bytes.last().is_none_or(|&b| b == b'\n');
                (actions, clean)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => (Vec::new(), true),
            Err(error) => return Err(error.into()),
        };
        let intents = Self { path, actions };
        // A torn line would swallow the next one appended to it: rewrite.
        if !clean {
            intents.rewrite()?;
        }
        Ok(intents)
    }

    fn rewrite(&self) -> Result<()> {
        if self.actions.is_empty() {
            return match fs::remove_file(&self.path) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error.into()),
                _ => Ok(()),
            };
        }
        let mut bytes = Vec::new();
        for action in &self.actions {
            bytes.extend(serde_json::to_vec(action).expect("actions always serialize"));
            bytes.push(b'\n');
        }
        Ok(write_atomic(&self.path, &bytes)?)
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
        let existed = self.path.exists();
        let mut file = open_private_append(&self.path)?;
        file.write_all(&line)?;
        file.sync_data()?;
        if !existed && let Some(dir) = self.path.parent().filter(|d| !d.as_os_str().is_empty()) {
            File::open(dir)?.sync_all()?;
        }
        self.actions.push(action.clone());
        Ok(())
    }

    /// Drops the intents with these sequence numbers.
    pub fn settle(&mut self, seqs: &[u64]) -> Result<()> {
        if !self.actions.iter().any(|a| seqs.contains(&a.seq)) {
            return Ok(());
        }
        let kept = Self {
            path: self.path.clone(),
            actions: self
                .actions
                .iter()
                .filter(|a| !seqs.contains(&a.seq))
                .cloned()
                .collect(),
        };
        kept.rewrite()?;
        self.actions.retain(|a| !seqs.contains(&a.seq));
        Ok(())
    }
}
