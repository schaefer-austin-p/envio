//! Folder list and folder sync, and fetching one body on open.

use envio_store::{BodyState, Folder, FolderId, MessageRowId};

use super::Local;
use crate::convert::with_body;
use crate::error::Result;
use crate::plan::folders::plan_folders;
use crate::plan::pending::Pending;
use crate::plan::sync::{
    FlagCheck, FolderPlan, FolderView, plan_flags, plan_folder, rows, window_start,
};
use crate::server::MailServer;

/// Brings the account's folder list in line with the server's and returns
/// it.
pub async fn sync_folders(
    server: &mut impl MailServer,
    local: &mut Local<'_>,
    account: &str,
) -> Result<Vec<Folder>> {
    let listed = server.list_folders().await?;
    let changes = plan_folders(&listed, &local.db.folders(account)?);
    local.db.step(|db| {
        for (path, role) in &changes.upsert {
            db.upsert_folder(account, path, *role)?;
        }
        for id in &changes.remove {
            db.remove_folder(*id)?;
        }
        Ok(())
    })?;
    for id in &changes.remove {
        local.bodies.remove_folder(*id)?;
    }
    Ok(local.db.folders(account)?)
}

/// What one folder sync changed, for core's notices and counts.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Synced {
    pub fetched: usize,
    pub removed: usize,
    pub flags_changed: usize,
}

/// Syncs one folder: reset on a new UIDVALIDITY, drop what's gone, update
/// flags, fetch what's new (newest first), then store the folder's state.
pub async fn sync_folder(
    server: &mut impl MailServer,
    local: &mut Local<'_>,
    id: FolderId,
    now_ms: i64,
) -> Result<Synced> {
    let folder = local.db.folder(id)?;
    let state = server.select(&folder.path).await?;
    let window = server.search_since(window_start(now_ms)).await?;
    let pending = Pending::of(
        local.journal.pending(),
        &folder.account,
        &folder.path,
        state.uid_validity,
    );
    let cached_uids = local.db.uids(id)?;
    let plan = plan_folder(
        &FolderView {
            server: state,
            window: &window,
            cached: folder.sync,
            cached_uids: &cached_uids,
            condstore: server.caps().condstore,
        },
        &pending,
    );
    begin(local, id, &plan)?;
    let mut synced = Synced {
        removed: local.db.step(|db| db.delete_messages(id, &plan.remove))?,
        ..Synced::default()
    };
    synced.flags_changed = check_flags(server, local, id, (&plan, &pending)).await?;
    for batch in &plan.fetch {
        let summaries = server.fetch_summaries(batch).await?;
        let rows = rows(&summaries, &pending);
        local.db.step(|db| {
            for row in &rows {
                db.upsert_message(id, row)?;
            }
            Ok(())
        })?;
        synced.fetched += rows.len();
    }
    local.db.step(|db| db.set_sync_state(id, &plan.finish))?;
    Ok(synced)
}

fn begin(local: &Local<'_>, id: FolderId, plan: &FolderPlan) -> Result<()> {
    let Some(begin) = plan.begin else {
        return Ok(());
    };
    local.db.step(|db| {
        if plan.reset {
            db.reset_folder(id)?;
        }
        db.set_sync_state(id, &begin)
    })?;
    if plan.reset {
        local.bodies.remove_folder(id)?;
    }
    Ok(())
}

/// Applies the server's flag changes; returns how many cached messages
/// changed.
async fn check_flags(
    server: &mut impl MailServer,
    local: &Local<'_>,
    id: FolderId,
    (plan, pending): (&FolderPlan, &Pending),
) -> Result<usize> {
    let since = match plan.flags {
        FlagCheck::Skip => return Ok(0),
        FlagCheck::ChangedSince(modseq) => Some(modseq),
        FlagCheck::All => None,
    };
    let updates = server.fetch_flags(&plan.check, since).await?;
    let changes = plan_flags(&local.db.flags_by_uid(id)?, &updates, pending);
    local.db.step(|db| {
        for (uid, flags) in &changes.set {
            db.set_flags(id, *uid, *flags)?;
        }
        db.delete_messages(id, &changes.remove)
    })?;
    Ok(changes.set.len() + changes.remove.len())
}

/// Downloads a message's body when it's opened: written to disk, then the
/// row gets its preview and is marked downloaded. `None` when the message
/// is gone from the server (its row is dropped) or the folder needs a sync
/// first (its UIDVALIDITY changed).
pub async fn fetch_body(
    server: &mut impl MailServer,
    local: &mut Local<'_>,
    id: MessageRowId,
) -> Result<Option<Vec<u8>>> {
    let message = local.db.message(id)?;
    let folder = local.db.folder(message.folder)?;
    let state = server.select(&folder.path).await?;
    if folder.sync.map(|sync| sync.uidvalidity) != Some(state.uid_validity) {
        return Ok(None);
    }
    let size = usize::try_from(message.size).unwrap_or(usize::MAX);
    let Some(eml) = server.fetch_body(message.uid, size).await? else {
        local
            .db
            .step(|db| db.delete_messages(folder.id, &[message.uid]))?;
        return Ok(None);
    };
    local.bodies.write(folder.id, message.uid, &eml)?;
    let row = with_body(&message, &eml);
    local.db.step(|db| {
        db.upsert_message(folder.id, &row)?;
        db.set_body_state(id, BodyState::Downloaded)
    })?;
    Ok(Some(eml))
}
