//! The live keyword check Edit makes before it links a draft to its
//! original (#324 T4): one `UID FETCH <uid> (FLAGS)`.

use envio_store::Message;

use super::Local;
use super::folders::apply_flags;
use crate::error::Result;
use crate::plan::pending::Pending;
use crate::server::MailServer;

/// Asks the server now for the flags and keywords of `message` and puts
/// them in the cache (a pending flag change of the journal still wins, as in
/// a sync; a `\Deleted` message leaves the cache). Returns the message's
/// keywords as the server just reported them; `None` when it cannot say:
/// the message is gone or marked `\Deleted`, or the folder's UIDVALIDITY
/// changed (the UID means another message, or none).
pub async fn refresh_flags(
    server: &mut impl MailServer,
    local: &Local<'_>,
    message: &Message,
) -> Result<Option<Vec<String>>> {
    let folder = local.db.folder(message.folder)?;
    let Some(known) = folder.sync.map(|sync| sync.uidvalidity) else {
        return Ok(None);
    };
    let state = server.select(&folder.path).await?;
    if state.uid_validity != known {
        return Ok(None);
    }
    let updates = server.fetch_flags(&[message.uid], None).await?;
    let pending = Pending::of(
        local.journal.pending(),
        &folder.account,
        &folder.path,
        known,
    );
    apply_flags(local, folder.id, &updates, &pending)?;
    Ok(updates
        .into_iter()
        .find(|u| u.uid == message.uid && !u.flags.deleted)
        .map(|u| u.flags.keywords))
}
