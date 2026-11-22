//! Looking a path up: its Graph id and its folder row. The in-memory table
//! is filled by a folder walk and completed from the cache file, so a new
//! `GraphServer` after a reconnect works at once.

use envio_net::imap::{FolderState, Role};
use envio_net::{NetError, Result};
use envio_store::FolderId;

use super::{GraphServer, storage};

#[derive(Debug, Clone)]
pub(super) struct Entry {
    pub graph_id: String,
    /// `None` until `commit_folders` or the cache file gives it.
    pub row: Option<FolderId>,
    pub role: Option<Role>,
}

/// The folder `select` chose; the message calls work in it.
#[derive(Debug, Clone)]
pub(super) struct Selected {
    pub graph_id: String,
    pub id: FolderId,
}

impl GraphServer {
    /// The entry for `path`. An empty table or an unknown path does one
    /// folder walk first; still unknown is `Refused`. The row id is read from
    /// the cache file if it is missing.
    pub(super) async fn entry(&mut self, path: &str) -> Result<Entry> {
        if !self.paths.contains_key(path) {
            self.walk().await?;
        }
        let entry = self
            .paths
            .get_mut(path)
            .ok_or_else(|| NetError::Refused(format!("no such folder: {path}")))?;
        if entry.row.is_none() {
            entry.row = self.map.folder_row_of(&entry.graph_id).map_err(storage)?;
        }
        Ok(entry.clone())
    }

    /// Like `entry`, with the row id required: a folder created since the
    /// last `commit_folders` is `Busy`, so the next folder sync commits it
    /// and the retry works (it never counts as a refusal).
    pub(super) async fn folder_of(&mut self, path: &str) -> Result<(String, FolderId)> {
        let entry = self.entry(path).await?;
        match entry.row {
            Some(id) => Ok((entry.graph_id, id)),
            None => Err(NetError::Busy("folder not listed yet".into())),
        }
    }

    /// The folder `select` chose; none yet is a driver bug.
    pub(super) fn selected(&self) -> Result<Selected> {
        self.selected
            .clone()
            .ok_or_else(|| NetError::Refused("no folder selected".into()))
    }

    pub(super) async fn select_path(&mut self, path: &str) -> Result<FolderState> {
        let (graph_id, id) = self.folder_of(path).await?;
        let state = self
            .map
            .folder(id)
            .map_err(storage)?
            .ok_or_else(|| NetError::Busy("folder not listed yet".into()))?;
        let exists = self.map.count(id).map_err(storage)?;
        self.selected = Some(Selected { graph_id, id });
        Ok(FolderState {
            uid_validity: state.generation,
            uid_next: state.uid_next,
            exists,
            highest_modseq: Some(state.modseq),
        })
    }
}
