//! "Load older": the next batch of messages the cache does not hold yet.

use std::collections::BTreeSet;

use envio_net::NetError;
use envio_store::{FolderId, MessageRowId};

use super::Local;
use super::folders::store_summaries;
use crate::error::{Result, SyncError};
use crate::plan::older::OlderQueue;
use crate::plan::pending::Pending;
use crate::plan::sync::{Kept, window_start};
use crate::server::MailServer;

/// Most batches one call fetches while they show nothing below the tail.
const MAX_BATCHES: usize = 10;
/// Rows dated below the tail that end the call: a lone outlier-dated
/// message in a batch of newer mail must not.
const MIN_BELOW: u32 = 10;

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
/// mail, so nothing is older), newest UID first, one transaction per batch.
/// Paged by UID
/// among the missing, not below the oldest cached UID: old mail may have
/// been appended after new mail (#94). The caller remembers `kept()` (and
/// passes it to the next call and to `sync_folder`) so the next sync keeps
/// these rows and a batch that stored nothing (all \Deleted or hidden) is
/// not planned again.
///
/// UID order is not date order (#254): a batch dated above `tail` (the date
/// of the list's last shown row) puts its rows above what the user sees. So
/// with a `tail` the call goes on to the next batch, up to [`MAX_BATCHES`],
/// until [`MIN_BELOW`] rows are dated strictly before it (the unified Inbox
/// passes its global tail to each folder); a batch that stores nothing counts
/// for none. Without a tail it fetches one batch, stored or not.
/// A failure after the first batch returns what was stored.
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
    tail: Option<i64>,
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
            // All first: mail arriving between the two is missing from `all`,
            // so it is left to the sync, never planned.
            let mut all = server.search_all().await?;
            let window: BTreeSet<u32> = server.search_since(since).await?.into_iter().collect();
            all.retain(|uid| !window.contains(uid));
            all
        }
        None => Vec::new(),
    };
    let base = match tail {
        Some(tail) => local.db.count_dated_before(folder, tail)?,
        None => 0,
    };
    let planned = kept.unwrap_or_default();
    // Read and sorted once per press: what the batches plan from.
    let cached: BTreeSet<u32> = local.db.uids(row.id)?.into_iter().collect();
    let mut queue = OlderQueue::new(&all, &cached, Some(&planned));
    let mut done: Vec<u32> = Vec::new();
    let mut fetched = Vec::new();
    let mut exhausted = false;
    for _ in 0..(if tail.is_some() { MAX_BATCHES } else { 1 }) {
        let step = batch(server, local, &row, state.uid_validity, &mut queue).await;
        let step = match step {
            Ok(step) => step,
            // Earlier batches are committed: report them, and let the next
            // press find the failure again.
            Err(error) if !fetched.is_empty() => {
                log::warn!("load older: stopped after a failure ({:?})", error.kind());
                exhausted = false;
                break;
            }
            Err(error) => return Err(error),
        };
        exhausted = step.exhausted;
        done.extend_from_slice(&step.fetch);
        let empty = step.stored.is_empty();
        fetched.extend(step.stored);
        // A batch that stored nothing (all \Deleted or hidden) shows
        // nothing below the tail either: the call goes on.
        let shown = match tail {
            Some(_) if empty => 0,
            Some(tail) => below(local, folder, tail, base),
            None => MIN_BELOW,
        };
        if exhausted || shown >= MIN_BELOW {
            break;
        }
    }
    Ok(Older {
        fetched,
        planned: planned.with(&done),
        exhausted,
    })
}

/// How many rows dated before `tail` the call has stored since `base`; a
/// failed count ends the call (`MIN_BELOW`).
fn below(local: &Local<'_>, folder: FolderId, tail: i64, base: u32) -> u32 {
    match local.db.count_dated_before(folder, tail) {
        Ok(count) => count.saturating_sub(base),
        Err(error) => {
            log::warn!("load older: counting rows failed ({error})");
            MIN_BELOW
        }
    }
}

/// One planned and stored batch.
struct Step {
    /// The UIDs planned, stored or not.
    fetch: Vec<u32>,
    stored: Vec<MessageRowId>,
    exhausted: bool,
}

/// Takes the next batch from the queue, fetches and stores it. A failure
/// ends the call, so the queue is not rolled back: the next press plans the
/// batch again.
async fn batch(
    server: &mut impl MailServer,
    local: &mut Local<'_>,
    row: &envio_store::Folder,
    uid_validity: u32,
    queue: &mut OlderQueue,
) -> Result<Step> {
    let plan = queue.next_batch();
    let mut stored = Vec::new();
    if !plan.fetch.is_empty() {
        let pending = Pending::of(
            local.journal.pending(),
            &row.account,
            &row.path,
            uid_validity,
        );
        let summaries = server.fetch_summaries(&plan.fetch).await?;
        stored = store_summaries(local, row.id, &summaries, &pending)?;
    }
    Ok(Step {
        fetch: plan.fetch,
        stored,
        exhausted: plan.exhausted,
    })
}
