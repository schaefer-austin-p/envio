//! Searches that reach beyond the retention window: the whole folder (sync
//! window "All") and the UIDs below the oldest cached one ("Load older").

use super::ImapSession;
use super::query;
use crate::error::Result;

impl ImapSession {
    /// Every UID in the selected folder (`UID SEARCH ALL`), ascending.
    pub async fn search_all(&mut self) -> Result<Vec<u32>> {
        self.search("ALL").await
    }

    /// The UIDs below `uid` in the selected folder (`UID SEARCH UID
    /// 1:<uid-1>`), ascending. Empty without a round trip when `uid <= 1`.
    pub async fn search_uids_below(&mut self, uid: u32) -> Result<Vec<u32>> {
        match query::search_uids_below(uid) {
            Some(key) => self.search(&key).await,
            None => Ok(Vec::new()),
        }
    }
}
