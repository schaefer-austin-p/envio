//! "Load older": the next batch of messages the cache does not hold yet.

use std::collections::BTreeSet;

use envio_net::NetError;
use envio_store::{FolderId, MessageRowId};

use super::Local;
use super::folders::store_summaries;
use crate::error::{Result, SyncError};
use crate::plan::older::plan_older;
use crate::plan::pending::Pending;
use crate::plan::sync::{Kept, window_start};
use crate::server::MailServer;

/// One batch of older mail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Older {
    /// The rows cached by this call, newest first.
    pub fetched: Vec<MessageRowId>,
    /// Every UID planned in this run up to and including this batch,
    /// stored or not.
    pub planned: Kept,
    /// Nothing is left on the server that the cache does not hold.
    pub exhausted: bool,
}

impl Older {
    /// What a sync keeps, and what the next batch passes back in.
    pub fn kept(&self) -> Kept {
        self.planned.clone()
    }
}

/// Fetches the next batch (of [`crate::plan::sync::BATCH`]) of messages the
/// folder's cache lacks outside the sync window (`window_days`, `None` = all
/// mail, so nothing is older), newest UID first, in one transaction. Paged by UID
/// among the missing, not below the oldest cached UID: old mail may have
/// been appended after new mail (#94). The caller remembers `kept()` (and
/// passes it to the next call and to `sync_folder`) so the next sync keeps
/// these rows and a batch that stored nothing (all \Deleted or hidden) is
/// not planned again.
///
/// A folder whose UIDVALIDITY changed since its last sync is refused (as
/// `search_server` does): the cached UIDs name other messages, and the
/// next sync resets the folder.
#[allow(clippy::too_many_arguments)] // like `sync_folder`: the window as a (now, days) pair
pub async fn load_older(
    server: &mut impl MailServer,
    local: &mut Local<'_>,
    folder: FolderId,
    (now_ms, window_days): (i64, Option<u32>),
    kept: Option<Kept>,
) -> Result<Older> {
    let row = local.db.folder(folder)?;
    let state = server.select(&row.path).await?;
    if row
        .sync
        .is_some_and(|sync| sync.uidvalidity != state.uid_validity)
    {
        return Err(SyncError::Net(NetError::Refused(String::new())));
    }
    // The window's mail is the sync's, not Load older's: planning it would
    // keep its rows after a server-side deletion.
    let all = match window_start(now_ms, window_days) {
        Some(since) => {
            let window: BTreeSet<u32> = server.search_since(since).await?.into_iter().collect();
            let mut all = server.search_all().await?;
            all.retain(|uid| !window.contains(uid));
            all
        }
        None => Vec::new(),
    };
    let cached: BTreeSet<u32> = local.db.uids(folder)?.into_iter().collect();
    let plan = plan_older(&all, &cached, kept.as_ref());
    let planned = kept.unwrap_or_default().with(&plan.fetch);
    let mut fetched = Vec::new();
    if !plan.fetch.is_empty() {
        let pending = Pending::of(
            local.journal.pending(),
            &row.account,
            &row.path,
            state.uid_validity,
        );
        let summaries = server.fetch_summaries(&plan.fetch).await?;
        fetched = store_summaries(local, folder, &summaries, &pending)?;
    }
    Ok(Older {
        fetched,
        planned,
        exhausted: plan.exhausted,
    })
}
