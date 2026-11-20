//! Load older: mail below the list's tail (a batch, or several while they
//! show nothing below it), a step per folder. For the unified Inbox the accounts' answers are gathered into one
//! `OlderLoaded`. A folder's floor (the lowest UID reached) is remembered for
//! the session so the next sync keeps these rows and the next batch starts
//! below them.

use std::collections::BTreeSet;

use envio_net::NetError;
use envio_net::imap::ImapSession;
use envio_store::{FolderId, FolderRole, ListRef};
use envio_sync::SyncError;
use envio_sync::driver;

use super::Worker;
use crate::api::event::Event;
use crate::clock::now_ms;
use crate::problem::{self, Problem};
use crate::servers::Service;
use crate::text;

/// A `LoadOlder` in progress: the folders that have not answered yet and
/// what the others fetched.
pub(crate) struct Load {
    /// The date of the list's last shown row.
    tail: Option<i64>,
    waiting: BTreeSet<FolderId>,
    fetched: usize,
    /// Every folder that answered has nothing older (and none failed).
    exhausted: bool,
    /// At least one folder answered.
    answered: bool,
    /// The accounts whose folder could not.
    failed: BTreeSet<String>,
    changed: Vec<FolderId>,
}

impl Worker<'_> {
    pub(super) fn load_older(&mut self, list: ListRef, tail: Option<i64>) {
        // The local Drafts entry (#303) has no server folder: nothing older
        // exists, so its row goes to Done at once, with no failure notice.
        if let ListRef::Folder(f) = list
            && self.local_folders.borrow().account_of(f).is_some()
        {
            self.emit(Event::OlderLoaded {
                list,
                fetched: 0,
                exhausted: true,
            });
            return;
        }
        let targets = self.older_targets(list);
        let mut load = Load {
            tail,
            waiting: targets.iter().map(|(_, folder)| *folder).collect(),
            fetched: 0,
            exhausted: true,
            answered: false,
            failed: BTreeSet::new(),
            changed: Vec::new(),
        };
        let now = now_ms();
        let disk_full = self.disk_full(now);
        for (account, folder) in targets {
            let ready = !disk_full
                && self
                    .live
                    .get(&account)
                    .is_some_and(|live| !live.health.is_paused() && live.health.can_run(now));
            if ready && let Some(live) = self.live.get_mut(&account) {
                live.wants.older.insert(folder);
            } else {
                load.waiting.remove(&folder);
                load.failed.insert(account.clone());
            }
        }
        self.loads.insert(list, load);
        self.older_check(list);
    }

    /// The (account, folder) pairs `list` loads older mail for.
    fn older_targets(&self, list: ListRef) -> Vec<(String, FolderId)> {
        match list {
            ListRef::Folder(folder) => match self.store.db.folder(folder) {
                Ok(f) => vec![(f.account, folder)],
                Err(e) => {
                    log::warn!("finding the folder to load older mail for: {e}");
                    Vec::new()
                }
            },
            ListRef::Inboxes => self
                .store
                .accounts
                .list()
                .iter()
                .filter_map(|a| Some((a.key.clone(), self.inbox_of(&a.key)?)))
                .collect(),
        }
    }

    /// The tail a step for `folder` uses, serving every list waiting for
    /// it: the oldest of their tails (a list with a newer one still gets
    /// rows below its own), `None` only if none has a tail.
    fn waiting_tail(&self, folder: FolderId) -> Option<i64> {
        self.loads
            .values()
            .filter(|load| load.waiting.contains(&folder))
            .filter_map(|load| load.tail)
            .min()
    }

    /// The step: older mail for `folder`.
    pub(super) async fn older_step(
        &mut self,
        session: &mut ImapSession,
        folder: FolderId,
    ) -> Result<(), Problem> {
        let floor = self.floors.get(&folder).cloned();
        let tail = self.waiting_tail(folder);
        let window = (now_ms(), self.store.settings.sync_window());
        match driver::load_older(
            session,
            &mut self.store.local(),
            folder,
            window,
            floor,
            tail,
        )
        .await
        {
            Ok(older) => {
                self.floors.insert(folder, older.kept());
                self.older_done(folder, older.fetched.len(), older.exhausted);
                Ok(())
            }
            Err(SyncError::Net(NetError::Refused(_))) => {
                // UIDVALIDITY changed: the folder is reset by its next sync,
                // and what the floor named is gone.
                self.floors.remove(&folder);
                Err(Problem::Refused(String::new()))
            }
            Err(e) => Err(problem::from_sync(&e, Service::Imap)),
        }
    }

    fn older_done(&mut self, folder: FolderId, fetched: usize, exhausted: bool) {
        let lists: Vec<ListRef> = self
            .loads
            .iter_mut()
            .filter_map(|(list, load)| {
                if !load.waiting.remove(&folder) {
                    return None;
                }
                load.fetched += fetched;
                load.exhausted &= exhausted;
                load.answered = true;
                load.changed.push(folder);
                Some(*list)
            })
            .collect();
        for list in lists {
            self.older_check(list);
        }
    }

    /// A step of `account` failed: every `LoadOlder` waiting for it gives
    /// up (nobody retries it; pressing the row again does).
    pub(super) fn older_account_failed(&mut self, account: &str) {
        self.older_account_gone(account, false);
    }

    /// `account` answers no more: with `removed` it is also left out of
    /// what the loads report, else it is named as one that failed.
    fn older_account_gone(&mut self, account: &str, removed: bool) {
        let mine: Vec<FolderId> = self
            .loads
            .values()
            .flat_map(|load| load.waiting.iter().copied())
            .filter(|folder| {
                self.store
                    .db
                    .folder(*folder)
                    .is_ok_and(|f| f.account == account)
            })
            .collect();
        if let Some(live) = self.live.get_mut(account) {
            live.wants.older.clear();
        }
        let mut lists = Vec::new();
        for (list, load) in &mut self.loads {
            let before = load.waiting.len();
            load.waiting.retain(|folder| !mine.contains(folder));
            if removed {
                load.failed.remove(account);
            }
            if load.waiting.len() != before {
                if !removed {
                    load.failed.insert(account.to_owned());
                }
                lists.push(*list);
            }
        }
        for list in lists {
            self.older_check(list);
        }
    }

    /// `account` is being removed: its folders' floors go, and a load
    /// waiting for it gives up. Called before its rows are deleted.
    pub(super) fn older_account_removed(&mut self, account: &str) {
        self.older_account_gone(account, true);
        for folder in self.store.db.folders(account).unwrap_or_default() {
            self.floors.remove(&folder.id);
        }
    }

    /// Whether a `LoadOlder` waits for `account`.
    pub(super) fn older_waiting(&self, account: &str) -> bool {
        self.loads
            .values()
            .flat_map(|load| load.waiting.iter())
            .any(|folder| {
                self.store
                    .db
                    .folder(*folder)
                    .is_ok_and(|f| f.account == account)
            })
    }

    /// Answers once every folder has: with the total, unless every one failed.
    fn older_check(&mut self, list: ListRef) {
        if self.loads.get(&list).is_none_or(|l| !l.waiting.is_empty()) {
            return;
        }
        let Some(load) = self.loads.remove(&list) else {
            return;
        };
        if !load.answered {
            self.emit(Event::OlderFailed {
                list,
                text: text::read::older_unavailable(),
            });
            return;
        }
        self.emit(Event::OlderLoaded {
            list,
            fetched: load.fetched,
            exhausted: load.exhausted && load.failed.is_empty(),
        });
        // The other accounts' batches count, but each tap would fail
        // silently for these: say which.
        if !load.failed.is_empty() {
            let names: Vec<String> = load
                .failed
                .iter()
                .filter_map(|key| self.store.accounts.get(key))
                .map(|a| a.settings.email.clone())
                .collect();
            self.emit(Event::Notice {
                text: text::read::older_unavailable_for(&names.join(", ")),
            });
        }
        if load.fetched > 0 {
            self.emit(Event::MessagesChanged {
                folders: load.changed,
            });
        }
    }

    /// The `OlderReset` for `folder`.
    pub(super) fn older_reset(&self, folder: FolderId) -> Event {
        let inbox = self
            .store
            .db
            .folder(folder)
            .is_ok_and(|f| f.role == FolderRole::Inbox);
        Event::OlderReset { folder, inbox }
    }

    /// The sync window changed: what Load older reached no longer applies,
    /// and every account syncs again under the new window.
    pub(super) fn window_changed(&mut self) {
        self.floors.clear();
        // Announced per folder once its resync has dropped the rows: before
        // that, a Load older still sees them and could end at "no older
        // messages" for good.
        let keys: Vec<String> = self
            .store
            .accounts
            .list()
            .iter()
            .map(|a| a.key.clone())
            .collect();
        for key in keys {
            for folder in self.store.db.folders(&key).unwrap_or_default() {
                self.reset_pending.insert(folder.id);
            }
        }
        for live in self.live.values_mut() {
            live.wants.full = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::testing::{Sink, open, seed};

    /// The removed account is the last one a load waits for (the others
    /// answered first): it is neither named nor held against "exhausted".
    #[test]
    fn removing_the_last_awaited_account_is_not_reported() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let list = ListRef::Inboxes;
        worker.loads.insert(
            list,
            Load {
                tail: None,
                waiting: BTreeSet::from([seeded.folder]),
                fetched: 9,
                exhausted: true,
                answered: true,
                failed: BTreeSet::new(),
                changed: vec![FolderId(99)],
            },
        );
        worker.remove_account(&seeded.account, true);
        let events = sink.events();
        let loaded = events
            .iter()
            .find_map(|e| match e {
                Event::OlderLoaded {
                    fetched, exhausted, ..
                } => Some((*fetched, *exhausted)),
                _ => None,
            })
            .expect("the load is answered");
        assert_eq!(loaded, (9, true));
        assert!(
            !events.iter().any(|e| matches!(e, Event::Notice { .. })),
            "{events:?}"
        );
    }

    /// Two lists wait on one folder: the step uses the older tail, whatever
    /// the order they were added in.
    #[test]
    fn two_waiting_loads_share_the_oldest_tail() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let load = |tail| Load {
            tail,
            waiting: BTreeSet::from([seeded.folder]),
            fetched: 0,
            exhausted: true,
            answered: false,
            failed: BTreeSet::new(),
            changed: Vec::new(),
        };
        let (newer, older) = (Some(2_000), Some(1_000));
        // The inserts go both ways, and with and without a tail-less list.
        for lists in [
            [newer, older, None],
            [older, newer, None],
            [None, older, newer],
        ] {
            let mut worker = open(&config, &sink);
            worker.loads.insert(ListRef::Inboxes, load(lists[0]));
            worker
                .loads
                .insert(ListRef::Folder(seeded.folder), load(lists[1]));
            worker
                .loads
                .insert(ListRef::Folder(FolderId(99)), load(lists[2]));
            assert_eq!(worker.waiting_tail(seeded.folder), older);
        }
        let mut worker = open(&config, &sink);
        worker.loads.insert(ListRef::Inboxes, load(None));
        assert_eq!(worker.waiting_tail(seeded.folder), None);
    }
}
