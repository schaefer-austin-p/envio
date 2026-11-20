//! One folder's sync, planned from what the server and the cache hold.
//! The cache keeps the chosen window (30 days by default, by arrival date)
//! and, in this run, whatever "Load older" fetched below it. New messages are
//! fetched newest first in batches, one transaction each, and the folder's
//! final state is stored last, so a cut connection resumes with nothing
//! lost and nothing fetched twice. Pure.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use envio_net::imap::{FlagUpdate, FolderState, Summary};
use envio_store::{Flags, NewMessage, SyncState};

use crate::convert::{keyword_list, new_message, store_flags};
use crate::plan::pending::Pending;

/// Messages per fetch and per transaction.
pub const BATCH: usize = 50;

/// The `SEARCH SINCE` argument for a window of `days` at `now_ms`; `None`
/// (window All) means no SINCE: search everything.
pub fn window_start(now_ms: i64, days: Option<u32>) -> Option<i64> {
    days.map(|days| now_ms / 1_000 - i64::from(days) * 86_400)
}

/// The UIDs "Load older" planned in this run (fetched, or skipped because
/// the mail is \Deleted or on its way out), always from outside the sync
/// window. Cached rows among them are kept wherever their UIDs sit: old
/// mail can have been appended after new mail (#94). A server-side deletion
/// of a window row still goes, as that UID was never planned.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Kept {
    uids: Arc<BTreeSet<u32>>,
}

impl Kept {
    pub fn holds(&self, uid: u32) -> bool {
        self.uids.contains(&uid)
    }

    /// This set and `planned`.
    #[must_use]
    pub fn with(&self, planned: &[u32]) -> Self {
        let mut uids = (*self.uids).clone();
        uids.extend(planned.iter().copied());
        Self {
            uids: Arc::new(uids),
        }
    }
}

/// What sync knows about a folder before planning.
#[derive(Debug, Clone, Copy)]
pub struct FolderView<'a> {
    /// From SELECT.
    pub server: FolderState,
    /// The server's UIDs inside the window, from `search_since`.
    pub window: &'a [u32],
    /// The cache's state for the folder, `None` before the first sync.
    pub cached: Option<SyncState>,
    /// The cache's UIDs for the folder.
    pub cached_uids: &'a [u32],
    /// What "Load older" fetched in this run: those cached rows are kept
    /// outside the window. `None` after a restart.
    pub kept: Option<&'a Kept>,
    pub condstore: bool,
    /// Some cached row inside the window (a cached UID the server lists in
    /// it) has unknown keywords: only a full flag check can fill them.
    pub unknown_keywords: bool,
}

/// Which flags to ask the server for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagCheck {
    /// Nothing changed since the last sync (or nothing is cached).
    Skip,
    /// CONDSTORE: only messages changed after this MODSEQ.
    ChangedSince(u64),
    /// Every cached message in the window.
    All,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderPlan {
    /// UIDVALIDITY changed: drop the folder's cache first.
    pub reset: bool,
    /// Stored in the first step of a first sync or after a reset, so a cut
    /// sync resumes against the right UIDVALIDITY. Its `uidnext` is 0:
    /// the sync isn't finished.
    pub begin: Option<SyncState>,
    /// Cached UIDs to drop: gone from the server or out of the window.
    pub remove: Vec<u32>,
    /// Cached UIDs whose flags to check.
    pub check: Vec<u32>,
    pub flags: FlagCheck,
    /// UIDs to fetch, newest first, in batches of at most [`BATCH`].
    pub fetch: Vec<Vec<u32>>,
    /// Stored last.
    pub finish: SyncState,
}

pub fn plan_folder(view: &FolderView<'_>, pending: &Pending) -> FolderPlan {
    let server = view.server;
    let reset = view
        .cached
        .is_some_and(|cached| cached.uidvalidity != server.uid_validity);
    let fresh = reset || view.cached.is_none();
    let cached: BTreeSet<u32> = if reset {
        BTreeSet::new()
    } else {
        view.cached_uids.iter().copied().collect()
    };
    let window: BTreeSet<u32> = view.window.iter().copied().collect();
    let check: Vec<u32> = cached.intersection(&window).copied().collect();
    let new: Vec<u32> = window
        .iter()
        .rev()
        .copied()
        .filter(|uid| !cached.contains(uid) && !pending.is_hidden(*uid))
        .collect();
    FolderPlan {
        reset,
        begin: fresh.then_some(SyncState {
            uidvalidity: server.uid_validity,
            uidnext: 0,
            highestmodseq: None,
        }),
        remove: cached
            .difference(&window)
            .copied()
            .filter(|uid| !view.kept.is_some_and(|kept| kept.holds(*uid)))
            .collect(),
        flags: flag_check(view, check.is_empty()),
        check,
        fetch: new.chunks(BATCH).map(<[u32]>::to_vec).collect(),
        finish: SyncState {
            uidvalidity: server.uid_validity,
            uidnext: server.uid_next,
            highestmodseq: server.highest_modseq.filter(|_| view.condstore),
        },
    }
}

fn flag_check(view: &FolderView<'_>, nothing_cached: bool) -> FlagCheck {
    if nothing_cached {
        return FlagCheck::Skip;
    }
    if view.unknown_keywords {
        return FlagCheck::All;
    }
    let last = view.cached.and_then(|cached| cached.highestmodseq);
    match (view.condstore, last, view.server.highest_modseq) {
        (true, Some(last), Some(now)) if last == now => FlagCheck::Skip,
        (true, Some(last), Some(_)) => FlagCheck::ChangedSince(last),
        _ => FlagCheck::All,
    }
}

/// What a flag check changes in the cache.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct FlagChanges {
    pub set: Vec<(u32, Flags)>,
    /// Marked \Deleted on the server: not shown, so not cached.
    pub remove: Vec<u32>,
    /// UIDs whose keyword list differs from the cache's, or whose cached
    /// keywords are unknown.
    pub keywords: Vec<(u32, Vec<String>)>,
}

/// `cached` is the folder's (UID, flags) from the cache; `cached_keywords`
/// its (UID, keywords), `None` for unknown (also for a UID not listed).
pub fn plan_flags(
    cached: &[(u32, Flags)],
    cached_keywords: &[(u32, Option<Vec<String>>)],
    updates: &[FlagUpdate],
    pending: &Pending,
) -> FlagChanges {
    let mut changes = FlagChanges::default();
    // The first entry per UID, as a linear search would find it.
    let mut by_uid: HashMap<u32, &Flags> = HashMap::with_capacity(cached.len());
    for (uid, flags) in cached {
        by_uid.entry(*uid).or_insert(flags);
    }
    let mut known: HashMap<u32, &Vec<String>> = HashMap::with_capacity(cached_keywords.len());
    for (uid, words) in cached_keywords {
        if let Some(words) = words {
            known.entry(*uid).or_insert(words);
        }
    }
    for update in updates {
        let Some(now) = by_uid.get(&update.uid) else {
            continue;
        };
        if update.flags.deleted {
            changes.remove.push(update.uid);
            continue;
        }
        let wanted = pending.flags(update.uid, store_flags(&update.flags));
        if wanted != **now {
            changes.set.push((update.uid, wanted));
        }
        // Pending changes are flag bits only: keywords come from the server.
        let words = keyword_list(&update.flags);
        if known.get(&update.uid) != Some(&&words) {
            changes.keywords.push((update.uid, words));
        }
    }
    changes
}

/// The rows to cache for a fetched batch: not \Deleted, not on its way
/// out, with pending flag changes on top.
pub fn rows(summaries: &[Summary], pending: &Pending) -> Vec<NewMessage> {
    summaries
        .iter()
        .filter(|summary| !summary.flags.deleted)
        .map(|summary| (summary, new_message(summary)))
        .filter(|(summary, row)| !pending.hides(summary.uid, row.envelope.message_id.as_deref()))
        .map(|(summary, mut row)| {
            row.flags = pending.flags(summary.uid, row.flags);
            row
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use envio_net::imap::{Flag, MessageFlags};
    use envio_store::{Action, ActionKind, MessageRef};

    use super::*;

    fn server(uid_validity: u32, uid_next: u32, highest_modseq: Option<u64>) -> FolderState {
        FolderState {
            uid_validity,
            uid_next,
            exists: 0,
            highest_modseq,
        }
    }

    fn state(uidvalidity: u32, uidnext: u32, highestmodseq: Option<u64>) -> SyncState {
        SyncState {
            uidvalidity,
            uidnext,
            highestmodseq,
        }
    }

    fn view<'a>(
        server: FolderState,
        window: &'a [u32],
        cached: Option<SyncState>,
        cached_uids: &'a [u32],
    ) -> FolderView<'a> {
        FolderView {
            server,
            window,
            cached,
            cached_uids,
            kept: None,
            condstore: true,
            unknown_keywords: false,
        }
    }

    fn with_unknown(mut view: FolderView<'_>) -> FolderView<'_> {
        view.unknown_keywords = true;
        view
    }

    fn with_kept<'a>(mut view: FolderView<'a>, kept: &'a Kept) -> FolderView<'a> {
        view.kept = Some(kept);
        view
    }

    /// What Load older planned: UIDs 20 to 29.
    fn planned() -> Kept {
        Kept::default().with(&(20..30).collect::<Vec<u32>>())
    }

    #[test]
    fn a_first_sync_fetches_the_window_newest_first_in_batches() {
        let window: Vec<u32> = (1..=120).collect();
        let plan = plan_folder(
            &view(server(7, 121, Some(500)), &window, None, &[]),
            &Pending::default(),
        );
        assert!(!plan.reset);
        assert_eq!(plan.begin, Some(state(7, 0, None)));
        assert_eq!(plan.flags, FlagCheck::Skip);
        assert_eq!(plan.fetch.len(), 3);
        assert_eq!(plan.fetch[0].first(), Some(&120));
        assert_eq!(plan.fetch[0].len(), BATCH);
        assert_eq!(plan.fetch[2], (1..=20).rev().collect::<Vec<_>>());
        assert_eq!(plan.finish, state(7, 121, Some(500)));
    }

    #[test]
    fn a_resumed_sync_fetches_only_what_is_missing() {
        let window = [1, 2, 3, 4];
        let plan = plan_folder(
            &view(
                server(7, 5, Some(9)),
                &window,
                Some(state(7, 0, None)),
                &[3, 4],
            ),
            &Pending::default(),
        );
        assert_eq!(plan.begin, None);
        assert_eq!(plan.fetch, [vec![2, 1]]);
        assert_eq!(plan.check, [3, 4]);
        assert_eq!(plan.flags, FlagCheck::All);
    }

    #[test]
    fn a_new_uidvalidity_resets_the_folder() {
        let plan = plan_folder(
            &view(
                server(8, 3, None),
                &[1, 2],
                Some(state(7, 40, None)),
                &[30, 31],
            ),
            &Pending::default(),
        );
        assert!(plan.reset);
        assert_eq!(plan.begin, Some(state(8, 0, None)));
        assert!(plan.remove.is_empty());
        assert!(plan.check.is_empty());
        assert_eq!(plan.fetch, [vec![2, 1]]);
    }

    #[test]
    fn condstore_skips_or_narrows_the_flag_check() {
        let cached = Some(state(7, 10, Some(50)));
        let same = plan_folder(
            &view(server(7, 10, Some(50)), &[5], cached, &[5]),
            &Pending::default(),
        );
        assert_eq!(same.flags, FlagCheck::Skip);
        assert!(same.fetch.is_empty());
        let moved = plan_folder(
            &view(server(7, 10, Some(60)), &[5], cached, &[5]),
            &Pending::default(),
        );
        assert_eq!(moved.flags, FlagCheck::ChangedSince(50));
        let mut plain = view(server(7, 10, Some(60)), &[5], cached, &[5]);
        plain.condstore = false;
        let plan = plan_folder(&plain, &Pending::default());
        assert_eq!(plan.flags, FlagCheck::All);
        assert_eq!(plan.finish.highestmodseq, None);
    }

    #[test]
    fn unknown_keywords_force_a_full_flag_check_but_not_a_new_modseq() {
        let cached = Some(state(7, 10, Some(50)));
        for now in [50, 60] {
            let plan = plan_folder(
                &with_unknown(view(server(7, 10, Some(now)), &[5], cached, &[5])),
                &Pending::default(),
            );
            assert_eq!(plan.flags, FlagCheck::All, "modseq {now}");
            assert_eq!(plan.finish.highestmodseq, Some(now));
        }
    }

    #[test]
    fn nothing_cached_still_skips_with_unknown_keywords() {
        let plan = plan_folder(
            &with_unknown(view(
                server(7, 10, Some(50)),
                &[5],
                Some(state(7, 10, Some(50))),
                &[],
            )),
            &Pending::default(),
        );
        assert_eq!(plan.flags, FlagCheck::Skip);
    }

    #[test]
    fn messages_gone_or_out_of_the_window_are_removed() {
        let plan = plan_folder(
            &view(
                server(7, 20, None),
                &[10, 12],
                Some(state(7, 13, None)),
                &[2, 10, 11],
            ),
            &Pending::default(),
        );
        assert_eq!(plan.remove, [2, 11]);
        assert_eq!(plan.check, [10]);
        assert_eq!(plan.fetch, [vec![12]]);
    }

    fn delete(uid: u32) -> Action {
        Action {
            seq: 1,
            at_ms: 0,
            target: MessageRef {
                account: "1700".into(),
                folder: "INBOX".into(),
                uidvalidity: 7,
                uid,
                message_id: None,
            },
            kind: ActionKind::Delete,
        }
    }

    #[test]
    fn a_message_with_a_pending_delete_is_not_fetched_again() {
        let actions = [delete(12)];
        let pending = Pending::of(&actions, "1700", "INBOX", 7);
        let plan = plan_folder(
            &view(
                server(7, 20, None),
                &[10, 12, 13],
                Some(state(7, 13, None)),
                &[10],
            ),
            &pending,
        );
        assert_eq!(plan.fetch, [vec![13]]);
    }

    fn update(uid: u32, flags: &[Flag]) -> FlagUpdate {
        FlagUpdate {
            uid,
            flags: MessageFlags::from_flags(flags.iter().cloned()),
            modseq: None,
        }
    }

    #[test]
    fn flag_updates_change_only_what_differs_and_drop_deleted_messages() {
        let cached = [(1, Flags::SEEN), (2, Flags::empty()), (3, Flags::empty())];
        let updates = [
            update(1, &[Flag::Seen]),
            update(2, &[Flag::Flagged]),
            update(3, &[Flag::Deleted]),
            update(99, &[Flag::Seen]),
        ];
        let changes = plan_flags(&cached, &known(&cached), &updates, &Pending::default());
        assert_eq!(changes.set, [(2, Flags::FLAGGED)]);
        assert_eq!(changes.remove, [3]);
    }

    #[test]
    fn a_flag_check_of_200k_messages_is_not_quadratic() {
        // 200k so that a linear search per update (~2e10 comparisons) breaks
        // the bound: the old loop took 30 s in a debug test build, this 0.1 s.
        let cached: Vec<(u32, Flags)> = (1..=200_000).map(|uid| (uid, Flags::empty())).collect();
        let updates: Vec<FlagUpdate> = (1..=200_000)
            .map(|uid| update(uid, &[Flag::Seen]))
            .collect();
        let started = std::time::Instant::now();
        let changes = plan_flags(&cached, &known(&cached), &updates, &Pending::default());
        assert_eq!(changes.set.len(), 200_000);
        assert_eq!(changes.set[199_999], (200_000, Flags::SEEN));
        assert!(started.elapsed().as_secs() < 5, "{:?}", started.elapsed());
    }

    #[test]
    fn a_pending_flag_change_survives_a_flag_check() {
        let mut unread = delete(1);
        unread.kind = ActionKind::SetFlags {
            add: Flags::empty(),
            remove: Flags::SEEN,
        };
        let actions = [unread];
        let pending = Pending::of(&actions, "1700", "INBOX", 7);
        let changes = plan_flags(
            &[(1, Flags::empty())],
            &[(1, Some(vec![]))],
            &[update(1, &[Flag::Seen])],
            &pending,
        );
        assert_eq!(changes, FlagChanges::default());
    }

    /// Cached keywords known to be none, for every cached UID.
    fn known(cached: &[(u32, Flags)]) -> Vec<(u32, Option<Vec<String>>)> {
        cached.iter().map(|(uid, _)| (*uid, Some(vec![]))).collect()
    }

    fn kw(word: &str) -> Flag {
        Flag::Keyword(word.into())
    }

    #[test]
    fn a_keyword_only_change_is_in_keywords_and_not_in_set() {
        let changes = plan_flags(
            &[(1, Flags::SEEN)],
            &[(1, Some(vec![]))],
            &[update(1, &[Flag::Seen, kw("$Forwarded")])],
            &Pending::default(),
        );
        assert!(changes.set.is_empty());
        assert_eq!(changes.keywords, [(1, vec!["$Forwarded".to_owned()])]);
    }

    #[test]
    fn a_change_in_both_is_in_both_and_unchanged_keywords_give_nothing() {
        let cached = [(1, Flags::empty()), (2, Flags::empty())];
        let kept = Some(vec!["$a".to_owned()]);
        let changes = plan_flags(
            &cached,
            &[(1, kept.clone()), (2, kept)],
            &[update(1, &[Flag::Seen, kw("$b")]), update(2, &[kw("$a")])],
            &Pending::default(),
        );
        assert_eq!(changes.set, [(1, Flags::SEEN)]);
        assert_eq!(changes.keywords, [(1, vec!["$b".to_owned()])]);
    }

    #[test]
    fn unknown_cached_keywords_give_an_entry_even_for_an_empty_list() {
        let changes = plan_flags(
            &[(1, Flags::empty()), (2, Flags::empty())],
            &[(1, None)],
            &[update(1, &[]), update(2, &[])],
            &Pending::default(),
        );
        assert_eq!(changes.keywords, [(1, vec![]), (2, vec![])]);
        assert!(changes.set.is_empty());
    }

    #[test]
    fn a_deleted_update_only_removes() {
        let changes = plan_flags(
            &[(1, Flags::empty())],
            &[(1, None)],
            &[update(1, &[Flag::Deleted, kw("$a")])],
            &Pending::default(),
        );
        assert_eq!(changes.remove, [1]);
        assert!(changes.keywords.is_empty() && changes.set.is_empty());
    }

    #[test]
    fn rows_skip_deleted_and_hidden_messages() {
        let summary = |uid, flags: &[Flag]| Summary {
            uid,
            flags: MessageFlags::from_flags(flags.iter().cloned()),
            size: 10,
            internal_date: Some(1_800_000_000),
            modseq: None,
            header: b"Subject: x\r\n\r\n".to_vec(),
        };
        let actions = [delete(2)];
        let pending = Pending::of(&actions, "1700", "INBOX", 7);
        let rows = rows(
            &[
                summary(1, &[Flag::Seen]),
                summary(2, &[]),
                summary(3, &[Flag::Deleted]),
            ],
            &pending,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].uid, rows[0].flags), (1, Flags::SEEN));
    }

    #[test]
    fn rows_hide_a_message_deleted_under_the_old_uidvalidity() {
        let mut old = delete(2);
        old.target.uidvalidity = 6;
        old.target.message_id = Some("<gone@example.org>".into());
        let actions = [old];
        let pending = Pending::of(&actions, "1700", "INBOX", 7);
        let summary = |uid, id: &str| Summary {
            uid,
            flags: MessageFlags::from_flags([]),
            size: 10,
            internal_date: Some(1_800_000_000),
            modseq: None,
            header: format!("Message-ID: {id}\r\nSubject: x\r\n\r\n").into_bytes(),
        };
        let kept = rows(
            &[
                summary(1, "<gone@example.org>"),
                summary(2, "<other@example.org>"),
            ],
            &pending,
        );
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].uid, 2);
    }

    fn moved(uid: u32) -> Action {
        let mut action = delete(uid);
        action.kind = ActionKind::Move {
            to: "Archive".into(),
        };
        action
    }

    #[test]
    fn a_message_with_a_pending_move_is_not_fetched_or_cached() {
        let actions = [moved(12)];
        let pending = Pending::of(&actions, "1700", "INBOX", 7);
        let plan = plan_folder(
            &view(
                server(7, 20, None),
                &[10, 12, 13],
                Some(state(7, 13, None)),
                &[10],
            ),
            &pending,
        );
        assert_eq!(plan.fetch, [vec![13]]);
        let summary = |uid| Summary {
            uid,
            flags: MessageFlags::from_flags([]),
            size: 10,
            internal_date: Some(1_800_000_000),
            modseq: None,
            header: b"Subject: x\r\n\r\n".to_vec(),
        };
        let kept = rows(&[summary(12), summary(13)], &pending);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].uid, 13);
    }

    #[test]
    fn rows_carry_pending_flags_over_the_servers() {
        let mut unread = delete(1);
        unread.kind = ActionKind::SetFlags {
            add: Flags::FLAGGED,
            remove: Flags::SEEN,
        };
        let actions = [unread];
        let pending = Pending::of(&actions, "1700", "INBOX", 7);
        let summary = Summary {
            uid: 1,
            flags: MessageFlags::from_flags([Flag::Seen]),
            size: 10,
            internal_date: Some(1_800_000_000),
            modseq: None,
            header: b"Subject: x\r\n\r\n".to_vec(),
        };
        let kept = rows(&[summary], &pending);
        assert_eq!(kept[0].flags, Flags::FLAGGED);
    }

    #[test]
    fn the_window_starts_the_chosen_days_back() {
        assert_eq!(
            window_start(1_800_000_000_000, Some(30)),
            Some(1_800_000_000 - 2_592_000)
        );
        assert_eq!(
            window_start(1_800_000_000_000, Some(365)),
            Some(1_800_000_000 - 365 * 86_400)
        );
    }

    #[test]
    fn window_all_searches_everything() {
        assert_eq!(window_start(1_800_000_000_000, None), None);
    }

    fn kept_view<'a>(cached_uids: &'a [u32], kept: &'a Kept) -> FolderView<'a> {
        with_kept(
            view(
                server(7, 40, None),
                &[30, 31, 33],
                Some(state(7, 34, None)),
                cached_uids,
            ),
            kept,
        )
    }

    #[test]
    fn planned_rows_survive_the_window() {
        let kept = planned();
        let plan = plan_folder(&kept_view(&[10, 20, 25, 30], &kept), &Pending::default());
        // 20 and 25 are older than the window but were planned.
        assert_eq!(plan.remove, [10]);
    }

    #[test]
    fn planned_rows_survive_wherever_their_uids_sit() {
        // Old mail appended after the window's: UIDs above it (#94).
        let kept = Kept::default().with(&[100, 101]);
        let plan = plan_folder(&kept_view(&[5, 30, 100, 101], &kept), &Pending::default());
        assert_eq!(plan.remove, [5]);
    }

    #[test]
    fn rows_never_planned_are_removed() {
        let kept = planned();
        let plan = plan_folder(&kept_view(&[5, 19, 20], &kept), &Pending::default());
        assert_eq!(plan.remove, [5, 19]);
    }

    #[test]
    fn in_window_deletions_are_removed_with_kept_rows() {
        let kept = planned();
        // 32 is inside the window's range but gone from the server.
        let plan = plan_folder(&kept_view(&[25, 30, 32], &kept), &Pending::default());
        assert_eq!(plan.remove, [32]);
    }

    #[test]
    fn a_window_deletion_below_the_windows_oldest_uid_is_removed_with_kept_rows() {
        let kept = planned();
        // Window [30, 31, 33] lost 30 in webmail: it is now [31, 33].
        let plan = plan_folder(
            &with_kept(
                view(
                    server(7, 40, None),
                    &[31, 33],
                    Some(state(7, 34, None)),
                    &[25, 30, 31, 33],
                ),
                &kept,
            ),
            &Pending::default(),
        );
        assert_eq!(plan.remove, [30]);
    }

    #[test]
    fn an_emptied_window_removes_every_window_row_with_kept_rows() {
        let kept = planned();
        let plan = plan_folder(
            &with_kept(
                view(
                    server(7, 40, None),
                    &[],
                    Some(state(7, 34, None)),
                    &[25, 30, 31, 33],
                ),
                &kept,
            ),
            &Pending::default(),
        );
        assert_eq!(plan.remove, [30, 31, 33]);
    }

    #[test]
    fn kept_rows_are_not_flag_checked() {
        let kept = planned();
        let plan = plan_folder(&kept_view(&[25, 30], &kept), &Pending::default());
        assert_eq!(plan.check, [30]);
    }
}
