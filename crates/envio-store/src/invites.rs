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
    /// The RECURRENCE-ID value of one occurrence; empty for the whole series
    /// (also what a file written before this field reads as).
    #[serde(default)]
    pub recurrence_id: String,
    pub sequence: u32,
    /// `accepted`, `tentative` or `declined`.
    pub answer: String,
    pub answered_at_ms: i64,
    /// When the answer may be dropped (30 days later): the event's end;
    /// `i64::MAX` for a series with no end, which only a later answer or
    /// `forget_invite` removes.
    pub event_end_unix: i64,
    pub reply_message_id: String,
    /// The answer that was really sent before this one (its own `previous`
    /// empty), for when this one's reply never goes out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<Box<InviteAnswer>>,
}

/// Missing file: none. A damaged file is moved aside, like the other user
/// files.
pub fn load_invites(path: &Path) -> Result<Vec<InviteAnswer>> {
    load(path)
}

/// Remembers `answer`, replacing the earlier one for the same (account, uid, recurrence id),
/// and drops answers whose event ended more than 30 days before `now_ms`.
pub fn record_invite(path: &Path, answer: InviteAnswer, now_ms: i64) -> Result<()> {
    let mut all = load_invites(path)?;
    all.retain(|a| !same(a, &answer.account, &answer.uid, &answer.recurrence_id));
    all.push(answer);
    let cutoff_unix = (now_ms - KEEP_AFTER_END_MS).div_euclid(1000);
    all.retain(|a| a.event_end_unix >= cutoff_unix);
    save(path, &all)
}

pub fn forget_invite(path: &Path, account: &str, uid: &str, recurrence_id: &str) -> Result<()> {
    let mut all = load_invites(path)?;
    let before = all.len();
    all.retain(|a| !same(a, account, uid, recurrence_id));
    if all.len() == before {
        return Ok(());
    }
    save(path, &all)
}

fn same(a: &InviteAnswer, account: &str, uid: &str, recurrence_id: &str) -> bool {
    a.account == account && a.uid == uid && a.recurrence_id == recurrence_id
}
