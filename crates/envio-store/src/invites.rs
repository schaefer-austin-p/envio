//! Remembered invite answers: `invites.json` in the data folder. User data
//! (it says which invites were answered and how), so it lives outside the
//! SQLite cache and is written atomically.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::userdata::{load, save};

/// How long an answer is kept after its event ended.
const KEEP_AFTER_END_MS: i64 = 30 * 86_400_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InviteAnswer {
    pub account: String,
    pub uid: String,
    pub sequence: u32,
    /// `accepted`, `tentative` or `declined`.
    pub answer: String,
    pub answered_at_ms: i64,
    pub event_end_unix: i64,
    pub reply_message_id: String,
}

/// Missing file: none. A damaged file is moved aside, like the other user
/// files.
pub fn load_invites(path: &Path) -> Result<Vec<InviteAnswer>> {
    load(path)
}

/// Remembers `answer`, replacing the earlier one for the same (account, uid),
/// and drops answers whose event ended more than 30 days before `now_ms`.
pub fn record_invite(path: &Path, answer: InviteAnswer, now_ms: i64) -> Result<()> {
    let mut all = load_invites(path)?;
    all.retain(|a| !(a.account == answer.account && a.uid == answer.uid));
    all.push(answer);
    let cutoff_unix = (now_ms - KEEP_AFTER_END_MS).div_euclid(1000);
    all.retain(|a| a.event_end_unix >= cutoff_unix);
    save(path, &all)
}

pub fn forget_invite(path: &Path, account: &str, uid: &str) -> Result<()> {
    let mut all = load_invites(path)?;
    let before = all.len();
    all.retain(|a| !(a.account == account && a.uid == uid));
    if all.len() == before {
        return Ok(());
    }
    save(path, &all)
}
