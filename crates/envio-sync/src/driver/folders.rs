//! Folder list and folder sync, and fetching one body on open.

use envio_net::imap::{FlagUpdate, Summary};
use envio_store::{BodyState, Folder, FolderId, MessageRowId};

use super::Local;
use crate::convert::with_body;
use crate::error::Result;
use crate::plan::folders::plan_folders;
use crate::plan::pending::Pending;
use crate::plan::sync::{
    FlagCheck, FolderPlan, FolderView, Kept, plan_flags, plan_folder, rows, window_start,
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
    /// Batches remain: call `sync_folder` again for the same folder. The
    /// folder's sync state is not finished until a call returns `false`.
    pub more: bool,
}

/// Batches (of [`crate::plan::sync::BATCH`] messages) fetched per call, so
/// a big first sync lets other commands run between parts.
pub const BATCHES_PER_STEP: usize = 2;

/// Syncs one folder: reset on a new UIDVALIDITY, drop what's gone, update
/// flags, fetch what's new (newest first), then store the folder's state.
/// At most [`BATCHES_PER_STEP`] batches are fetched per call; when more
/// remain the result has `more: true` and the state is left unfinished.
/// The next call plans again from the cache, so it fetches only what is
/// missing (`begin` is a no-op then: the stored UIDVALIDITY matches).
///
/// `window_days` is the sync window (`None` = all mail); `kept` is what
/// "Load older" fetched in this run: those cached rows are kept.
#[allow(clippy::too_many_arguments)] // the signature the plan fixes
pub async fn sync_folder(
    server: &mut impl MailServer,
    local: &mut Local<'_>,
    id: FolderId,
    now_ms: i64,
    window_days: Option<u32>,
    kept: Option<Kept>,
) -> Result<Synced> {
    let folder = local.db.folder(id)?;
    let state = server.select(&folder.path).await?;
    let window = match window_start(now_ms, window_days) {
        Some(since) => server.search_since(since).await?,
        None => server.search_all().await?,
    };
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
            kept,
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
    for batch in plan.fetch.iter().take(BATCHES_PER_STEP) {
        let summaries = server.fetch_summaries(batch).await?;
        synced.fetched += store_summaries(local, id, &summaries, &pending)?.len();
    }
    synced.more = plan.fetch.len() > BATCHES_PER_STEP;
    if !synced.more {
        local.db.step(|db| db.set_sync_state(id, &plan.finish))?;
    }
    Ok(synced)
}

/// Caches one fetched batch in a single transaction (and remembers its
/// senders and recipients for autocomplete); returns the rows' ids.
pub(super) fn store_summaries(
    local: &Local<'_>,
    id: FolderId,
    summaries: &[Summary],
    pending: &Pending,
) -> Result<Vec<MessageRowId>> {
    let rows = rows(summaries, pending);
    Ok(local.db.step(|db| {
        let mut ids = Vec::with_capacity(rows.len());
        for row in &rows {
            ids.push(db.upsert_message(id, row)?);
        }
        // For autocomplete: every sender and recipient, as of the
        // message's date.
        for summary in summaries {
            let (found, when) = crate::convert::seen_addresses(summary);
            db.remember_addresses(&found, when)?;
        }
        Ok(ids)
    })?)
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
    apply_flags(local, id, &updates, pending)
}

/// Stores the server's flags for cached messages, pending changes on top;
/// \Deleted ones leave the cache. Returns how many changed.
pub(super) fn apply_flags(
    local: &Local<'_>,
    id: FolderId,
    updates: &[FlagUpdate],
    pending: &Pending,
) -> Result<usize> {
    let changes = plan_flags(&local.db.flags_by_uid(id)?, updates, pending);
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
        db.set_has_attachments(id, row.envelope.has_attachments)?;
        db.set_body_state(id, BodyState::Downloaded)
    })?;
    Ok(Some(eml))
}
