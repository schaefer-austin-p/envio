//! "Search on server": the server's answer for one folder, cached.

use std::collections::BTreeSet;

use envio_net::NetError;
use envio_store::{FolderId, MessageRowId};

use super::Local;
use super::folders::store_summaries;
use crate::error::{Result, SyncError};
use crate::plan::pending::Pending;
use crate::plan::sync::BATCH;
use crate::server::MailServer;

/// Asks the server which messages of the folder contain `text`, fetches the
/// envelopes of those not cached (in batches of [`BATCH`]) and caches them
/// like synced rows, and returns the row ids of every match, cached or
/// fetched. A message older than the sync window stays cached until the
/// folder's next sync drops it (an open reader keeps its own copy).
///
/// A folder whose UIDVALIDITY changed since its last sync is refused: the
/// cached UIDs name other messages, and the next sync resets the folder.
pub async fn search_server(
    server: &mut impl MailServer,
    local: &mut Local<'_>,
    folder: FolderId,
    text: &str,
) -> Result<Vec<MessageRowId>> {
    let row = local.db.folder(folder)?;
    let state = server.select(&row.path).await?;
    if row
        .sync
        .is_some_and(|sync| sync.uidvalidity != state.uid_validity)
    {
        return Err(SyncError::Net(NetError::Refused(String::new())));
    }
    let found = server.search_text(text).await?;
    let cached: BTreeSet<u32> = local.db.uids(folder)?.into_iter().collect();
    let missing: Vec<u32> = found
        .iter()
        .copied()
        .filter(|uid| !cached.contains(uid))
        .collect();
    let pending = Pending::of(
        local.journal.pending(),
        &row.account,
        &row.path,
        state.uid_validity,
    );
    for batch in missing.chunks(BATCH) {
        let summaries = server.fetch_summaries(batch).await?;
        store_summaries(local, folder, &summaries, &pending)?;
    }
    Ok(local.db.ids_of(folder, &found)?)
}
