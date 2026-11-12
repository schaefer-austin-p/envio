//! "Load older": the next batch of messages below the oldest cached one.

use std::collections::BTreeSet;

use envio_net::NetError;
use envio_store::{FolderId, MessageRowId};

use super::Local;
use super::folders::store_summaries;
use crate::error::{Result, SyncError};
use crate::plan::older::plan_older;
use crate::plan::pending::Pending;
use crate::server::MailServer;

/// One batch of older mail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Older {
    /// The rows cached by this call, newest first.
    pub fetched: Vec<MessageRowId>,
    /// The lowest UID this call reached: the folder's new kept floor.
    pub floor: u32,
    /// Nothing older is left on the server.
    pub exhausted: bool,
}

/// Fetches the next batch (of [`crate::plan::sync::BATCH`]) of messages
/// below the folder's oldest cached UID, newest first, in one transaction.
/// The caller remembers `floor` so the next folder sync keeps these rows.
///
/// A folder whose UIDVALIDITY changed since its last sync is refused (as
/// `search_server` does): the cached UIDs name other messages, and the
/// next sync resets the folder.
pub async fn load_older(
    server: &mut impl MailServer,
    local: &mut Local<'_>,
    folder: FolderId,
) -> Result<Older> {
    let row = local.db.folder(folder)?;
    let state = server.select(&row.path).await?;
    if row
        .sync
        .is_some_and(|sync| sync.uidvalidity != state.uid_validity)
    {
        return Err(SyncError::Net(NetError::Refused(String::new())));
    }
    let cached_uids = local.db.uids(folder)?;
    let oldest = cached_uids.first().copied().unwrap_or(state.uid_next);
    let below = server.search_uids_below(oldest).await?;
    let cached: BTreeSet<u32> = cached_uids.into_iter().collect();
    let plan = plan_older(&below, &cached);
    let floor = plan.fetch.last().copied().unwrap_or(oldest);
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
        floor,
        exhausted: plan.exhausted,
    })
}
