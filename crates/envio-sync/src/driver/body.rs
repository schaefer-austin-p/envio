//! Fetching a body when a message is opened. A message above
//! [`PARTIAL_ABOVE`] comes in parts (its text and small inline images; the
//! attachments stay on the server and are fetched one by one when the user
//! asks), so opening a 40 MB mail never holds it whole in memory.

use envio_store::{BodyState, Message, MessageRowId};

use super::Local;
use crate::convert::with_body;
use crate::error::Result;
use crate::server::MailServer;

/// Messages bigger than this (RFC822.SIZE, bytes) are fetched in parts.
pub const PARTIAL_ABOVE: u32 = 10 * 1024 * 1024;

/// Whether a message of `size` is fetched in parts. A size of 0 (cached
/// before migration 0002) is not known to be big.
pub fn is_big(size: u32) -> bool {
    size > PARTIAL_ABOVE
}

/// Downloads a message's body when it's opened: written to disk, then the
/// row gets its preview and is marked downloaded (partial for a big
/// message). `None` when the message is gone from the server (its row is
/// dropped) or the folder needs a sync first (its UIDVALIDITY changed).
pub async fn fetch_body(
    server: &mut impl MailServer,
    local: &mut Local<'_>,
    id: MessageRowId,
) -> Result<Option<Vec<u8>>> {
    fetch(server, local, id, false).await
}

/// Like [`fetch_body`], but always the whole message: a forward needs the
/// attachments. The whole message of a big one is for the caller only, never
/// cached: the cache keeps its partial body (or none), so opening it again
/// stays small.
pub async fn fetch_whole_body(
    server: &mut impl MailServer,
    local: &mut Local<'_>,
    id: MessageRowId,
) -> Result<Option<Vec<u8>>> {
    fetch(server, local, id, true).await
}

async fn fetch(
    server: &mut impl MailServer,
    local: &mut Local<'_>,
    id: MessageRowId,
    whole: bool,
) -> Result<Option<Vec<u8>>> {
    let message = local.db.message(id)?;
    let folder = local.db.folder(message.folder)?;
    let state = server.select(&folder.path).await?;
    if folder.sync.map(|sync| sync.uidvalidity) != Some(state.uid_validity) {
        return Ok(None);
    }
    let size = usize::try_from(message.size).unwrap_or(usize::MAX);
    let partial = !whole && is_big(message.size);
    let fetched = if partial {
        server.fetch_partial(message.uid).await?
    } else {
        server.fetch_body(message.uid, size).await?
    };
    let Some(eml) = fetched else {
        local
            .db
            .step(|db| db.delete_messages(folder.id, &[message.uid]))?;
        return Ok(None);
    };
    if whole && is_big(message.size) {
        return Ok(Some(eml));
    }
    let kept = if partial {
        BodyState::Partial
    } else {
        BodyState::Downloaded
    };
    store_body(local, &message, &eml, kept)?;
    Ok(Some(eml))
}

/// One part of a message, as a message of its own (see
/// [`MailServer::fetch_section`]): not stored, the caller writes it where the
/// user asked. `size` is the part's. `None` when the message or part is gone
/// or the folder needs a sync first.
pub async fn fetch_part(
    server: &mut impl MailServer,
    local: &mut Local<'_>,
    id: MessageRowId,
    (section, size): (&str, usize),
) -> Result<Option<Vec<u8>>> {
    let message = local.db.message(id)?;
    let folder = local.db.folder(message.folder)?;
    let state = server.select(&folder.path).await?;
    if folder.sync.map(|sync| sync.uidvalidity) != Some(state.uid_validity) {
        return Ok(None);
    }
    Ok(server.fetch_section(message.uid, section, size).await?)
}

/// Writes a downloaded body to disk, then gives the row its preview and
/// marks it `state`.
pub(super) fn store_body(
    local: &Local<'_>,
    message: &Message,
    eml: &[u8],
    state: BodyState,
) -> Result<()> {
    local.bodies.write(message.folder, message.uid, eml)?;
    let row = with_body(message, eml);
    local.db.step(|db| {
        db.upsert_message(message.folder, &row)?;
        db.set_has_attachments(message.id, row.envelope.has_attachments)?;
        db.set_body_state(message.id, state)
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_size_above_the_threshold_is_big() {
        assert!(!is_big(0));
        assert!(!is_big(PARTIAL_ABOVE));
        assert!(is_big(PARTIAL_ABOVE + 1));
    }
}
