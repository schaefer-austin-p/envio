//! Which bodies the worker downloads before they are opened (#102): the
//! Inbox's recent, small ones that are not cached and not given up on.

use envio_store::{Folder, FolderId, FolderRole, MessageRowId};
use envio_sync::plan::sync::window_start;

use super::Worker;
use super::plan::{PREFETCH_BATCH, PREFETCH_DAYS, PREFETCH_MAX_SIZE};

impl Worker<'_> {
    /// The Inbox, if the next prefetch batch is not empty. Whether it may be
    /// fetched now (foreground, not paused, disk) is `plan::prefetch::step`'s
    /// rule alone. Only a synced
    /// folder (its UIDVALIDITY is known) can be fetched from.
    pub(super) fn prefetch_folder(
        &self,
        account: &str,
        folders: &[Folder],
        now_ms: i64,
    ) -> Option<FolderId> {
        let inbox = folders
            .iter()
            .find(|f| f.role == FolderRole::Inbox && f.sync.is_some())?;
        let off = self
            .live
            .get(account)
            .is_some_and(|live| live.prefetch_off.contains(&inbox.id));
        (!off && !self.prefetch_batch(account, inbox.id, now_ms).is_empty()).then_some(inbox.id)
    }

    /// The next messages to download, newest first. Never older than the
    /// sync window keeps bodies, or the sweep would drop them again.
    pub(super) fn prefetch_batch(
        &self,
        account: &str,
        folder: FolderId,
        now_ms: i64,
    ) -> Vec<MessageRowId> {
        let skip = self.live.get(account).map(|live| &live.prefetch_skip);
        let skipped = skip.map_or(0, |skip| skip.len());
        let week = window_start(now_ms, Some(PREFETCH_DAYS));
        let kept = window_start(now_ms, self.store.settings.sync_window());
        let since = week.into_iter().chain(kept).max().unwrap_or(i64::MIN);
        let limit = PREFETCH_BATCH.saturating_add(u32::try_from(skipped).unwrap_or(u32::MAX));
        match self
            .store
            .db
            .prefetch_candidates(folder, since, PREFETCH_MAX_SIZE, limit)
        {
            Ok(ids) => ids
                .into_iter()
                .filter(|id| !skip.is_some_and(|skip| skip.contains(id)))
                .take(PREFETCH_BATCH as usize)
                .collect(),
            Err(e) => {
                log::warn!("listing bodies to prefetch: {e}");
                Vec::new()
            }
        }
    }
}
