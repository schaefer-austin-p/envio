//! Folder list and folder sync, and fetching one body on open.

use envio_mime::Address;
use envio_net::imap::{FlagUpdate, Summary};
use envio_store::{Flags, Folder, FolderId, FolderRole, MessageRowId};

use super::Local;
use crate::error::Result;
use crate::newmail::{self, NewMail, NewRow};
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
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Synced {
    pub fetched: usize,
    pub removed: usize,
    pub flags_changed: usize,
    /// Batches remain: call `sync_folder` again for the same folder. The
    /// folder's sync state is not finished until a call returns `false`.
    pub more: bool,
    /// New mail this call stored; always empty for folders but the Inbox.
    /// A first sync, a UIDVALIDITY reset and a sync that is still
    /// unfinished from one never count. Reported once, by the call that
    /// finishes the sync (`more` false), for everything cached from the
    /// old UIDNEXT up, even when earlier calls stored it.
    pub new_mail: NewMail,
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
    let how = Options {
        now_ms,
        window_days,
        kept,
        own: &[],
    };
    sync_folder_as(server, local, id, how).await
}

/// What [`sync_folder`] takes besides the folder, plus the account's own
/// addresses.
#[derive(Debug, Clone)]
pub struct Options<'a> {
    pub now_ms: i64,
    pub window_days: Option<u32>,
    pub kept: Option<Kept>,
    pub own: &'a [Address],
}

/// [`sync_folder`] that also knows the account's own addresses, so Inbox
/// mail they sent does not count as new (`Synced::new_mail`). The sync
/// step cannot find them itself: the store keeps no identities, so the
/// caller passes them in.
pub async fn sync_folder_as(
    server: &mut impl MailServer,
    local: &mut Local<'_>,
    id: FolderId,
    Options {
        now_ms,
        window_days,
        kept,
        own,
    }: Options<'_>,
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
            kept: kept.as_ref(),
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
        // Before the state is stored: if this fails the old UIDNEXT stays
        // and the retry reports the same messages.
        if folder.role == FolderRole::Inbox {
            let prev = folder.sync.map(|s| s.uidnext).filter(|next| *next > 0);
            let stored = new_rows(local, id, prev, plan.finish.uidnext)?;
            synced.new_mail = newmail::count(prev, plan.reset, &stored, own);
        }
        local.db.step(|db| db.set_sync_state(id, &plan.finish))?;
    }
    Ok(synced)
}

/// The cached rows from the old UIDNEXT up, as the new-mail rule reads
/// them. Read from the cache once the sync is finished, so a sync split
/// over several calls, or cut and retried, still reports everything it
/// stored, once.
fn new_rows(local: &Local<'_>, id: FolderId, prev: Option<u32>, next: u32) -> Result<Vec<NewRow>> {
    let Some(prev) = prev else {
        return Ok(Vec::new());
    };
    Ok(local
        .db
        .messages_in_uid_range(id, prev, next)?
        .into_iter()
        .map(|m| NewRow {
            uid: m.uid,
            seen: m.flags.contains(Flags::SEEN),
            from: first_address(&m.envelope.from),
            subject: m.envelope.subject,
        })
        .collect())
}

/// The first address of a cached, comma-joined From display
/// (`Name <a@x>, b@y`); commas inside quotes or `<>` do not split.
fn first_address(shown: &str) -> Address {
    let (mut quoted, mut angled) = (false, false);
    let end = shown
        .char_indices()
        .find(|&(_, c)| {
            match c {
                '"' => quoted = !quoted,
                '<' if !quoted => angled = true,
                '>' if !quoted => angled = false,
                _ => {}
            }
            c == ',' && !quoted && !angled
        })
        .map_or(shown.len(), |(at, _)| at);
    Address::parse(&shown[..end]).unwrap_or_default()
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
