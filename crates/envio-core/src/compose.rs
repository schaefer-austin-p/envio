//! Compose without I/O: checking a draft, converting it, and pacing saves.

use std::collections::BTreeMap;

use envio_mime::{Address, Outgoing, OutgoingAttachment};
use envio_store::Draft;

use crate::api::command::{ComposeId, DraftInput};

/// A changed draft is on disk at most this long after its first unsaved
/// change.
pub const DRAFT_SAVE_MS: i64 = 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeError {
    NoRecipients,
    /// The address as typed.
    BadAddress(String),
    NoAccount,
}

fn parse_all(list: &[String]) -> Result<Vec<Address>, ComposeError> {
    list.iter()
        .map(|text| Address::parse(text).ok_or_else(|| ComposeError::BadAddress(text.clone())))
        .collect()
}

/// Every recipient parsed, to then cc then bcc. A message needs at least
/// one; Bcc alone is enough.
pub fn check(draft: &DraftInput) -> Result<Vec<Address>, ComposeError> {
    if draft.to.is_empty() && draft.cc.is_empty() && draft.bcc.is_empty() {
        return Err(ComposeError::NoRecipients);
    }
    let mut all = parse_all(&draft.to)?;
    all.extend(parse_all(&draft.cc)?);
    all.extend(parse_all(&draft.bcc)?);
    Ok(all)
}

pub fn to_store(draft: &DraftInput, now_ms: i64) -> Draft {
    Draft {
        account: draft.account.clone(),
        to: draft.to.clone(),
        cc: draft.cc.clone(),
        bcc: draft.bcc.clone(),
        subject: draft.subject.clone(),
        body: draft.body.clone(),
        in_reply_to: draft.in_reply_to.clone(),
        references: draft.references.clone(),
        attachments: draft.attachments.clone(),
        updated_at_ms: now_ms,
    }
}

pub fn from_store(id: &str, draft: &Draft) -> DraftInput {
    DraftInput {
        account: draft.account.clone(),
        draft_id: Some(id.to_owned()),
        to: draft.to.clone(),
        cc: draft.cc.clone(),
        bcc: draft.bcc.clone(),
        subject: draft.subject.clone(),
        body: draft.body.clone(),
        in_reply_to: draft.in_reply_to.clone(),
        references: draft.references.clone(),
        attachments: draft.attachments.clone(),
    }
}

/// The message to build; `attachments` are the files already read.
pub fn to_outgoing(
    draft: &DraftInput,
    from: Address,
    attachments: Vec<OutgoingAttachment>,
) -> Result<Outgoing, ComposeError> {
    check(draft)?;
    Ok(Outgoing {
        from,
        to: parse_all(&draft.to)?,
        cc: parse_all(&draft.cc)?,
        bcc: parse_all(&draft.bcc)?,
        subject: draft.subject.clone(),
        text: draft.body.clone(),
        in_reply_to: draft.in_reply_to.clone(),
        references: draft.references.clone(),
        attachments,
    })
}

/// Paces draft saves: the first unsaved change of a compose is due
/// `DRAFT_SAVE_MS` later; more changes replace the content but keep that
/// time, so typing without a pause still saves every second.
#[derive(Default)]
pub struct SaveThrottle {
    pending: BTreeMap<ComposeId, (i64, DraftInput)>,
}

impl SaveThrottle {
    pub fn changed(&mut self, compose: ComposeId, draft: DraftInput, now_ms: i64) {
        match self.pending.get_mut(&compose) {
            Some(entry) => entry.1 = draft,
            None => {
                self.pending
                    .insert(compose, (now_ms + DRAFT_SAVE_MS, draft));
            }
        }
    }

    /// The drafts whose time has come; they are no longer pending.
    pub fn due(&mut self, now_ms: i64) -> Vec<(ComposeId, DraftInput)> {
        let ready: Vec<ComposeId> = self
            .pending
            .iter()
            .filter(|(_, (due, _))| *due <= now_ms)
            .map(|(id, _)| *id)
            .collect();
        ready
            .into_iter()
            .filter_map(|id| self.take(id).map(|d| (id, d)))
            .collect()
    }

    /// Puts back a draft that could not be saved, due a second from now.
    /// A newer change that came meanwhile wins and keeps its own time.
    pub fn retry(&mut self, compose: ComposeId, draft: DraftInput, now_ms: i64) {
        self.pending
            .entry(compose)
            .or_insert((now_ms + DRAFT_SAVE_MS, draft));
    }

    pub fn take(&mut self, compose: ComposeId) -> Option<DraftInput> {
        self.pending.remove(&compose).map(|(_, draft)| draft)
    }

    pub fn take_all(&mut self) -> Vec<(ComposeId, DraftInput)> {
        std::mem::take(&mut self.pending)
            .into_iter()
            .map(|(id, (_, draft))| (id, draft))
            .collect()
    }

    pub fn next_due(&self) -> Option<i64> {
        self.pending.values().map(|(due, _)| *due).min()
    }

    /// Forgets the pending saves of an account being removed.
    pub fn drop_account(&mut self, account: &str) {
        self.pending
            .retain(|_, (_, draft)| draft.account != account);
    }
}

/// `YYYY-MM-DD HH:MM` in UTC; empty for no date.
pub fn date_text(unix_secs: Option<i64>) -> String {
    let Some(secs) = unix_secs else {
        return String::new();
    };
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    // Civil date from a day count (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rest / 3_600,
        rest % 3_600 / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(to: &[&str], bcc: &[&str]) -> DraftInput {
        DraftInput {
            to: to.iter().map(|s| (*s).to_owned()).collect(),
            bcc: bcc.iter().map(|s| (*s).to_owned()).collect(),
            ..DraftInput::default()
        }
    }

    fn marked(n: &str) -> DraftInput {
        DraftInput {
            body: n.into(),
            ..DraftInput::default()
        }
    }

    #[test]
    fn no_recipients_is_refused() {
        assert_eq!(check(&input(&[], &[])), Err(ComposeError::NoRecipients));
    }

    #[test]
    fn a_bcc_only_message_is_allowed() {
        let found = check(&input(&[], &["hidden@example.org"])).unwrap();
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn a_bad_address_is_named() {
        assert_eq!(
            check(&input(&["ada@example.org", "nonsense"], &[])),
            Err(ComposeError::BadAddress("nonsense".into()))
        );
    }

    #[test]
    fn typing_continuously_still_saves_every_second() {
        let mut t = SaveThrottle::default();
        let c = ComposeId(1);
        t.changed(c, marked("a"), 0);
        // A change every 100 ms must not push the save back.
        for ms in (100..=1_000).step_by(100) {
            t.changed(c, marked(&ms.to_string()), ms);
        }
        assert_eq!(t.next_due(), Some(DRAFT_SAVE_MS));
        assert!(t.due(DRAFT_SAVE_MS - 1).is_empty());
        // The latest content is what is saved.
        assert_eq!(t.due(DRAFT_SAVE_MS), vec![(c, marked("1000"))]);
        // The next change starts a new second.
        t.changed(c, marked("b"), 1_050);
        assert_eq!(t.next_due(), Some(1_050 + DRAFT_SAVE_MS));
    }

    #[test]
    fn a_quiet_change_saves_after_one_second() {
        let mut t = SaveThrottle::default();
        t.changed(ComposeId(1), marked("a"), 500);
        assert_eq!(t.next_due(), Some(1_500));
        assert!(t.due(1_499).is_empty());
        assert_eq!(t.due(1_500).len(), 1);
        assert_eq!(t.next_due(), None);
    }

    #[test]
    fn a_retry_keeps_a_newer_change() {
        let mut t = SaveThrottle::default();
        let c = ComposeId(1);
        t.retry(c, marked("old"), 100);
        assert_eq!(t.next_due(), Some(100 + DRAFT_SAVE_MS));
        t.retry(c, marked("older"), 900);
        assert_eq!(t.take(c), Some(marked("old")));
        t.changed(c, marked("new"), 200);
        t.retry(c, marked("old"), 300);
        assert_eq!(t.take(c), Some(marked("new")));
    }

    #[test]
    fn take_all_returns_every_pending_draft() {
        let mut t = SaveThrottle::default();
        t.changed(ComposeId(1), marked("a"), 0);
        t.changed(ComposeId(2), marked("b"), 10);
        assert_eq!(t.take_all().len(), 2);
        assert_eq!(t.next_due(), None);
        assert_eq!(t.take(ComposeId(1)), None);
    }

    #[test]
    fn a_draft_round_trips_with_its_attachments() {
        let mut d = input(&["a@example.org"], &[]);
        d.attachments = vec!["/tmp/x.txt".into()];
        d.draft_id = Some("k".into());
        assert_eq!(from_store("k", &to_store(&d, 5)), d);
    }

    #[test]
    fn dates_are_written_in_utc() {
        assert_eq!(date_text(Some(0)), "1970-01-01 00:00");
        assert_eq!(date_text(Some(1_709_210_096)), "2024-02-29 12:34");
        assert_eq!(date_text(None), "");
    }
}
