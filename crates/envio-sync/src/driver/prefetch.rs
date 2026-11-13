//! Several bodies in one session, downloaded before they are opened (#102).

use envio_store::{FolderId, MessageRowId};

use super::Local;
use super::folders::store_body;
use crate::error::Result;
use crate::server::MailServer;

/// What `prefetch_bodies` got before it stopped.
#[derive(Debug)]
pub struct Prefetched {
    /// The messages handled, in order: `Some` is stored, `None` is gone from
    /// the server (its row is dropped).
    pub done: Vec<(MessageRowId, Option<Vec<u8>>)>,
    /// Why it stopped before the end of `ids`. The message it stopped at is
    /// the one after `done`. The caller handles `done` first, then fails the
    /// step with this, so a lost connection is not reused.
    pub error: Option<crate::error::SyncError>,
}

/// Downloads the bodies of `ids` (all in `folder`) with one SELECT. Each is
/// stored as `fetch_body` stores an opened one. A message that is gone from
/// the server is `None` and its row is dropped; if the folder's UIDVALIDITY
/// changed every message is `None` (rows kept) and the folder needs a sync
/// first.
///
/// An error before the first body (the SELECT) is the call's error; one
/// after comes back in `Prefetched::error` with what was done until then.
pub async fn prefetch_bodies(
    server: &mut impl MailServer,
    local: &mut Local<'_>,
    folder: FolderId,
    ids: &[MessageRowId],
) -> Result<Prefetched> {
    let folder = local.db.folder(folder)?;
    let state = server.select(&folder.path).await?;
    let mut out = Prefetched {
        done: Vec::new(),
        error: None,
    };
    if folder.sync.map(|sync| sync.uidvalidity) != Some(state.uid_validity) {
        out.done = ids.iter().map(|id| (*id, None)).collect();
        return Ok(out);
    }
    for id in ids {
        match prefetch_one(server, local, &folder, *id).await {
            Ok(fetched) => out.done.push((*id, fetched)),
            Err(e) => {
                out.error = Some(e);
                break;
            }
        }
    }
    Ok(out)
}

async fn prefetch_one(
    server: &mut impl MailServer,
    local: &mut Local<'_>,
    folder: &envio_store::Folder,
    id: MessageRowId,
) -> Result<Option<Vec<u8>>> {
    let message = local.db.message(id)?;
    let size = usize::try_from(message.size).unwrap_or(usize::MAX);
    let fetched = server.fetch_body(message.uid, size).await?;
    match &fetched {
        Some(eml) => store_body(local, &message, eml)?,
        None => {
            local
                .db
                .step(|db| db.delete_messages(folder.id, &[message.uid]))?;
        }
    }
    Ok(fetched)
}
