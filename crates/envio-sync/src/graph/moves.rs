//! Moving, deleting and filing messages. A moved or deleted message leaves
//! the source folder's map at once, so the planner's next pass does not
//! meet its uid again; the target maps the message at its own next refresh.

use envio_net::imap::Role;
use envio_net::{NetError, Result};

use super::{GraphServer, storage};

impl GraphServer {
    pub(super) async fn move_uids(&mut self, uids: &[u32], target: &str) -> Result<()> {
        let target = self.entry(target).await?.graph_id;
        let folder = self.selected()?;
        let ids = self.map.ids_of(folder.id, uids).map_err(storage)?;
        let mut done = Vec::new();
        let mut outcome = Ok(());
        for (_, graph_id) in ids {
            match self.client.move_to(&graph_id, &target).await {
                Ok(Some(_)) => done.push(graph_id),
                // A 404 is the message or the target folder. Only a message
                // that is really gone leaves the map; a deleted target
                // (stale paths) keeps the row and the message visible.
                Ok(None) => match self.client.get_categories(&graph_id).await {
                    Ok(None) => done.push(graph_id),
                    Ok(Some(_)) => {
                        outcome = Err(NetError::Refused("the target folder is gone".into()));
                        break;
                    }
                    Err(e) => {
                        outcome = Err(e);
                        break;
                    }
                },
                Err(e) => {
                    outcome = Err(e);
                    break;
                }
            }
        }
        self.forget(folder.id, &done)?;
        outcome
    }

    pub(super) async fn delete_uids(&mut self, uids: &[u32]) -> Result<()> {
        let folder = self.selected()?;
        let ids = self.map.ids_of(folder.id, uids).map_err(storage)?;
        let mut done = Vec::new();
        let mut outcome = Ok(());
        for (_, graph_id) in ids {
            match self.client.permanent_delete(&graph_id).await {
                Ok(()) => done.push(graph_id),
                Err(e) => {
                    outcome = Err(e);
                    break;
                }
            }
        }
        self.forget(folder.id, &done)?;
        outcome
    }

    fn forget(&mut self, folder: envio_store::FolderId, ids: &[String]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        self.map.apply(folder, &[], ids).map_err(storage)
    }

    /// Graph files nothing by itself but drafts: a Sent copy is made by the
    /// send, so any other folder is refused.
    pub(super) async fn file(&mut self, folder: &str, eml: &[u8]) -> Result<()> {
        let entry = self.entry(folder).await?;
        if entry.role != Some(Role::Drafts) {
            return Err(NetError::Refused("Graph files drafts only".into()));
        }
        self.client.create_mime(&entry.graph_id, eml).await?;
        Ok(())
    }
}
