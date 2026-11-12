//! Several bodies in one session, downloaded before they are opened (#102).

use envio_store::{FolderId, MessageRowId};

use super::Local;
use super::folders::store_body;
use crate::error::Result;
use crate::server::MailServer;

/// Downloads the bodies of `ids` (all in `folder`) with one SELECT. Each is
/// stored as `fetch_body` stores an opened one. A message that is gone from
/// the server is `None` and its row is dropped; if the folder's UIDVALIDITY
/// changed every message is `None` and the folder needs a sync first.
///
/// A connection error after some bodies were stored returns those (the
/// error comes again with the next call); with none stored it is the
/// call's error.
pub async fn prefetch_bodies(
    server: &mut impl MailServer,
    local: &mut Local<'_>,
    folder: FolderId,
    ids: &[MessageRowId],
) -> Result<Vec<(MessageRowId, Option<Vec<u8>>)>> {
    let folder = local.db.folder(folder)?;
    let state = server.select(&folder.path).await?;
    let mut out = Vec::new();
    if folder.sync.map(|sync| sync.uidvalidity) != Some(state.uid_validity) {
        return Ok(ids.iter().map(|id| (*id, None)).collect());
    }
    for id in ids {
        let message = local.db.message(*id)?;
        let size = usize::try_from(message.size).unwrap_or(usize::MAX);
        let fetched = match server.fetch_body(message.uid, size).await {
            Ok(fetched) => fetched,
            Err(_) if !out.is_empty() => break,
            Err(e) => return Err(e.into()),
        };
        match &fetched {
            Some(eml) => store_body(local, &message, eml)?,
            None => {
                local
                    .db
                    .step(|db| db.delete_messages(folder.id, &[message.uid]))?;
            }
        }
        out.push((*id, fetched));
    }
    Ok(out)
}
