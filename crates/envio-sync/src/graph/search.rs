//! Server searches, always in the last selected folder (the replay and
//! drafts drivers select the folder they mean). A hit that no refresh has
//! mapped yet gets a low-range uid, and its flags go into the map at once so
//! a summary read right after shows the real state.

use envio_net::graph::model::GMeta;
use envio_net::{NetError, Result};
use envio_store::{Flags, FolderId, MetaChange};

use super::{GraphServer, storage};

/// The flag bits of a search hit.
fn bits_of(m: &GMeta) -> u8 {
    [
        (m.is_read, Flags::SEEN),
        (m.flagged, Flags::FLAGGED),
        (m.is_draft, Flags::DRAFT),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .fold(0, |bits, (_, flag)| bits | u8::from(flag))
}

impl GraphServer {
    pub(super) async fn find_id(&mut self, message_id: &str) -> Result<Vec<u32>> {
        let folder = self.selected()?;
        let hits = self
            .client
            .find_message_id(&folder.graph_id, message_id)
            .await?;
        self.resolve(folder.id, &hits)
    }

    pub(super) async fn find_text(&mut self, text: &str) -> Result<Vec<u32>> {
        let folder = self.selected()?;
        let hits = self.client.search(&folder.graph_id, text).await?;
        self.resolve(folder.id, &hits)
    }

    fn resolve(&mut self, folder: FolderId, hits: &[GMeta]) -> Result<Vec<u32>> {
        let mut out = Vec::new();
        for m in hits {
            if let Some(uid) = self.map.uid_of(folder, &m.id).map_err(storage)? {
                out.push(uid);
                continue;
            }
            let Some(uid) = self
                .map
                .allocate_low(folder, &m.id, m.received_ms)
                .map_err(storage)?
            else {
                // The low range ran out: a new generation. The driver's
                // next select sees it; this call retries later.
                self.map.reset(folder).map_err(storage)?;
                self.pending.remove(&folder);
                return Err(NetError::Busy("the folder was reset".into()));
            };
            let change = MetaChange {
                graph_id: m.id.clone(),
                flags: bits_of(m),
                categories: m.categories.clone(),
            };
            self.map
                .apply(folder, std::slice::from_ref(&change), &[])
                .map_err(storage)?;
            out.push(uid);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_flags_map_to_bits() {
        let m = GMeta {
            id: "x".into(),
            received_ms: 0,
            is_read: true,
            flagged: false,
            categories: vec![],
            is_draft: true,
        };
        assert_eq!(bits_of(&m), u8::from(Flags::SEEN) | u8::from(Flags::DRAFT));
    }
}
