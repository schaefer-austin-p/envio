//! The delta refresh: drains a folder's delta feed into the uid map. The
//! result waits in `pending` until `commit_folder` stores the link, so a
//! sync that is cut short drains the same link again; allocating an id that
//! is mapped already keeps its uid, so that replay is harmless.

use std::collections::{BTreeMap, HashSet};

use envio_net::graph::model::{DeltaNext, DeltaPage, GChange};
use envio_net::{NetError, Result};
use envio_store::{Allocation, FolderId, MetaChange};

use super::meta::{fold, merged};
use super::{GraphServer, Pending, storage};

/// Resets one refresh may do (an expired link, then uid exhaustion) before
/// it gives up.
const MAX_RESETS: u8 = 2;

/// Everything one delta walk said.
struct Drained {
    changed: BTreeMap<String, GChange>,
    removed: Vec<String>,
    link: String,
}

impl GraphServer {
    pub(super) async fn refresh_path(&mut self, path: &str) -> Result<()> {
        let (graph_id, id) = self.folder_of(path).await?;
        if self.pending.contains_key(&id) {
            return Ok(());
        }
        for _ in 0..=MAX_RESETS {
            let state = self
                .map
                .folder(id)
                .map_err(storage)?
                .ok_or_else(|| NetError::Busy("folder not listed yet".into()))?;
            let link = (!state.delta_link.is_empty()).then_some(state.delta_link);
            let Some(drained) = self.drain(&graph_id, link.as_deref()).await? else {
                // The link expired: a new generation, then walk from the start.
                self.map.reset(id).map_err(storage)?;
                continue;
            };
            if self.store(id, &drained)? {
                return Ok(());
            }
            // uid space exhausted: `store` reset the folder; start over.
        }
        Err(NetError::Protocol("Graph folder kept resetting".into()))
    }

    /// Follows the pages. `None`: the link is expired.
    async fn drain(&mut self, graph_id: &str, mut link: Option<&str>) -> Result<Option<Drained>> {
        let first = link.is_none();
        let mut changed: BTreeMap<String, GChange> = BTreeMap::new();
        let mut removed: Vec<String> = Vec::new();
        let mut next_link;
        loop {
            match self.client.delta(graph_id, link).await? {
                DeltaPage::Expired if first => {
                    return Err(NetError::Protocol("a new delta walk was expired".into()));
                }
                DeltaPage::Expired => return Ok(None),
                DeltaPage::Page {
                    changed: items,
                    removed: gone,
                    next,
                } => {
                    for id in gone {
                        changed.remove(&id);
                        removed.push(id);
                    }
                    for item in items {
                        match changed.get_mut(&item.id) {
                            Some(earlier) => fold(earlier, item),
                            None => {
                                changed.insert(item.id.clone(), item);
                            }
                        }
                    }
                    match next {
                        DeltaNext::More(l) => {
                            next_link = l;
                            link = Some(&next_link);
                        }
                        DeltaNext::Done(l) => {
                            removed.retain(|id| !changed.contains_key(id));
                            return Ok(Some(Drained {
                                changed,
                                removed,
                                link: l,
                            }));
                        }
                    }
                }
            }
        }
    }

    /// Writes the drained delta to the map. `false`: the uid space ran out,
    /// the folder was reset and the caller drains again.
    fn store(&mut self, id: FolderId, drained: &Drained) -> Result<bool> {
        let mut fresh: Vec<(String, i64)> = Vec::new();
        let mut known: HashSet<&str> = HashSet::new();
        for change in drained.changed.values() {
            match self.map.uid_of(id, &change.id).map_err(storage)? {
                Some(_) => {
                    known.insert(change.id.as_str());
                }
                // An item for an id nothing mapped, without a received time,
                // cannot be placed; the next full walk brings it.
                None => fresh.extend(change.received_ms.map(|ms| (change.id.clone(), ms))),
            }
        }
        if !fresh.is_empty()
            && self.map.allocate(id, &fresh).map_err(storage)? == Allocation::Exhausted
        {
            self.map.reset(id).map_err(storage)?;
            return Ok(false);
        }
        let mut metas = Vec::new();
        for change in drained.changed.values() {
            let Some(uid) = self.map.uid_of(id, &change.id).map_err(storage)? else {
                continue;
            };
            let old = if known.contains(change.id.as_str()) {
                self.map
                    .row(id, uid)
                    .map_err(storage)?
                    .map_or((0, Vec::new()), |r| (r.flags, r.categories))
            } else {
                (0, Vec::new())
            };
            let (flags, categories) = merged(&old, change);
            metas.push(MetaChange {
                graph_id: change.id.clone(),
                flags,
                categories,
            });
        }
        self.map
            .apply(id, &metas, &drained.removed)
            .map_err(storage)?;
        let modseq = self
            .map
            .folder(id)
            .map_err(storage)?
            .map_or(0, |f| f.modseq);
        self.pending.insert(
            id,
            Pending {
                link: drained.link.clone(),
                modseq,
            },
        );
        Ok(true)
    }
}
