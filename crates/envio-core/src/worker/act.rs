//! The user's actions on messages and the Undo window around them.

use std::collections::BTreeSet;

use envio_store::{ActionKind, Flags, Folder, FolderId, FolderRole, MessageRowId};
use envio_sync::UNDO_MS;
use envio_sync::driver::{self, Undo};

use super::Worker;
use super::act_local::offered_with;
use crate::api::command::{UndoId, UserAction};
use crate::api::event::{Event, UndoScope};
use crate::clock::now_ms;
use crate::problem::{self, Problem};
use crate::text;
use crate::undo::{Offer, Offered};

/// The journal entry for `action` on a message of a `from`-role folder,
/// given the message's account's folders. `None` when it has no target
/// there. From Drafts only Delete and a Move to Trash are kinds: any other
/// way out would leave the local draft file behind (#228).
pub(crate) fn action_kind(
    action: UserAction,
    from: FolderRole,
    folders: &[Folder],
) -> Option<ActionKind> {
    let flags = |add: Flags, remove: Flags| Some(ActionKind::SetFlags { add, remove });
    let move_to = |found: Option<&Folder>| found.map(|f| ActionKind::Move { to: f.path.clone() });
    let to_trash = |to: FolderId| {
        folders
            .iter()
            .any(|f| f.id == to && f.role == FolderRole::Trash)
    };
    match action {
        UserAction::Archive if from == FolderRole::Drafts => None,
        UserAction::Move { to } if from == FolderRole::Drafts && !to_trash(to) => None,
        UserAction::MarkRead(true) => flags(Flags::SEEN, Flags::empty()),
        UserAction::MarkRead(false) => flags(Flags::empty(), Flags::SEEN),
        UserAction::Flag(true) => flags(Flags::FLAGGED, Flags::empty()),
        UserAction::Flag(false) => flags(Flags::empty(), Flags::FLAGGED),
        UserAction::Delete => Some(ActionKind::Delete),
        UserAction::Archive => move_to(folders.iter().find(|f| f.role == FolderRole::Archive)),
        UserAction::Move { to } => move_to(folders.iter().find(|f| f.id == to)),
    }
}

/// What one `Act` has done so far.
#[derive(Default)]
struct Done {
    now: i64,
    undos: Vec<Undo>,
    /// Recorded, but without an Undo (the cache write failed, #41).
    kept: usize,
    accounts: BTreeSet<String>,
    folders: BTreeSet<FolderId>,
    /// The notice for the first message left alone, if any.
    refused: Option<String>,
    /// Local drafts held for the window (see `act_local`).
    local: Vec<String>,
}

impl Done {
    fn count(&self) -> usize {
        self.undos.len() + self.kept + self.local.len()
    }
}

impl Worker<'_> {
    pub(super) fn act(&mut self, ids: &[MessageRowId], action: UserAction) {
        let now = now_ms();
        let mut done = Done {
            now,
            ..Done::default()
        };
        for id in ids.iter().filter(|id| id.0 >= 0) {
            self.act_one(*id, action, &mut done);
        }
        done.local = self.hold_local(ids, action);
        if !done.folders.is_empty() {
            self.emit(Event::MessagesChanged {
                folders: done.folders.iter().copied().collect(),
            });
        }
        if let Some(text) = done.refused.take() {
            self.emit(Event::Notice { text });
        }
        if done.kept > 0 {
            self.emit(Event::Notice {
                text: text::undo::cannot_undo(),
            });
        }
        if done.count() > 0 {
            self.offer_undo(action, done, now);
        }
    }

    /// Opening a message marks it read. Recorded like any flag change, but
    /// the user did not ask for it, so there is no Undo offer and no notice.
    pub(super) fn mark_opened_read(&mut self, id: MessageRowId) {
        match self.store.db.message(id) {
            Ok(message) if !message.flags.contains(Flags::SEEN) => {}
            _ => return,
        }
        let mut done = Done {
            now: now_ms(),
            ..Done::default()
        };
        self.act_one(id, UserAction::MarkRead(true), &mut done);
        if !done.folders.is_empty() {
            self.emit(Event::MessagesChanged {
                folders: done.folders.iter().copied().collect(),
            });
        }
        // The journal holds the action back for the Undo window; an offer
        // with nothing to take back is what runs the replay when it ends.
        if !done.accounts.is_empty() {
            let now = done.now;
            done.undos.clear();
            self.offer_undo(UserAction::MarkRead(true), done, now);
        }
    }

    fn offer_undo(&mut self, action: UserAction, done: Done, now: i64) {
        let shown = !done.undos.is_empty() || !done.local.is_empty();
        let text = text::undo::offer(action, done.count());
        let expires_at_ms = now + UNDO_MS;
        let actions = Offered::Actions {
            undos: done.undos,
            accounts: done.accounts,
            folders: done.folders,
        };
        let undo = self.undo.offer(Offer {
            what: offered_with(done.local, actions),
            expires_at_ms,
        });
        // With nothing to take back the entry only times the replay.
        if shown {
            self.emit(Event::UndoOffer {
                undo,
                text,
                expires_at_ms,
                scope: UndoScope::List,
            });
        }
    }

    fn act_one(&mut self, id: MessageRowId, action: UserAction, done: &mut Done) {
        let found = self.store.db.message(id).and_then(|message| {
            let folder = self.store.db.folder(message.folder)?;
            let folders = self.store.db.folders(&folder.account)?;
            Ok((folder, folders))
        });
        let (folder, folders) = match found {
            Ok(found) => found,
            Err(error) => {
                done.refused.get_or_insert_with(|| failed_text(&error));
                return;
            }
        };
        let Some(kind) = action_kind(action, folder.role, &folders) else {
            let from_drafts = folder.role == FolderRole::Drafts;
            done.refused.get_or_insert_with(|| match action {
                UserAction::Archive | UserAction::Move { .. } if from_drafts => {
                    text::undo::drafts_delete_only()
                }
                UserAction::Archive => text::undo::no_archive(),
                _ => text::undo::not_done(),
            });
            return;
        };
        let newest = self.store.journal.pending().map(|a| a.seq).max();
        self.arm_journal_hook();
        match driver::record(&mut self.store.local(), id, kind, done.now) {
            Ok(undo) => done.undos.push(undo),
            Err(error) => {
                log::warn!("recording an action: {error}");
                if only_maybe_saved(&error) {
                    done.refused
                        .get_or_insert_with(text::notice::change_may_still_apply);
                    return;
                }
                // An error after the journal write leaves the action
                // pending: it is done, just not undoable (#41).
                if self.store.journal.pending().map(|a| a.seq).max() == newest {
                    done.refused.get_or_insert_with(|| refusal_text(&error));
                    return;
                }
                done.kept += 1;
            }
        }
        done.accounts.insert(folder.account);
        done.folders.insert(folder.id);
    }

    /// `now` is when the user pressed Undo, not when the worker got to it.
    pub(super) fn undo_action(&mut self, undo: UndoId, now: i64) {
        let just_closed = self.expire_undos(now);
        let Some(offer) = self.undo.take(undo, now) else {
            // An offer that ended a moment ago has been closed just now.
            if !just_closed.contains(&undo) {
                self.emit(Event::UndoClosed {
                    undo,
                    undone: false,
                });
            }
            self.emit(Event::Notice {
                text: text::undo::too_late(),
            });
            return;
        };
        let (restored, what) = self.restore_local(offer.what);
        let (undos, accounts, folders) = match what {
            Offered::Actions {
                undos,
                accounts,
                folders,
            } => (undos, accounts, folders),
            answer @ Offered::Answer { .. } => return self.undo_answer(undo, answer),
            Offered::LocalDrafts { .. } => {
                // `restore_local` left no drafts in it.
                self.emit(Event::UndoClosed {
                    undo,
                    undone: restored,
                });
                return;
            }
        };
        let mut undone = 0;
        let mut late = 0;
        let mut failed = false;
        let mut may_still_apply = false;
        self.arm_journal_hook();
        for item in &undos {
            match driver::undo(&mut self.store.local(), item) {
                Ok(true) => undone += 1,
                Ok(false) => late += 1,
                Err(error) => {
                    log::warn!("undoing an action: {error}");
                    failed = true;
                    may_still_apply |= only_maybe_saved(&error);
                }
            }
        }
        if undone > 0 {
            self.emit(Event::MessagesChanged {
                folders: folders.iter().copied().collect(),
            });
            self.check_flags_in(&folders);
        }
        self.emit(Event::UndoClosed {
            undo,
            undone: undone > 0 || restored,
        });
        if failed {
            // Those stay pending: replay them without waiting for a poll.
            self.want_replay(&accounts);
            self.emit(Event::Notice {
                text: if may_still_apply {
                    text::notice::undo_may_still_apply()
                } else {
                    text::undo::cannot_undo()
                },
            });
        } else if late > 0 {
            self.emit(Event::Notice {
                text: text::undo::too_late(),
            });
        }
    }

    /// Test hook: makes the next journal write fail to sync, if asked to.
    fn arm_journal_hook(&mut self) {
        #[cfg(feature = "test-hooks")]
        if crate::hooks::take_journal_failure(&self.store.paths.journal()) {
            self.store.journal.fail_next_sync();
        }
        #[cfg(feature = "test-hooks")]
        if crate::hooks::take_compact_failure(&self.store.paths.journal()) {
            self.store.journal.fail_next_compact();
        }
    }

    /// Syncs the folders again: an undone flag change leaves the cache
    /// possibly ahead of the server, and only a flag check shows it (#41).
    pub(super) fn check_flags_in(&mut self, folders: &BTreeSet<FolderId>) {
        for id in folders {
            let Ok(folder) = self.store.db.folder(*id) else {
                continue;
            };
            if let Some(live) = self.live.get_mut(&folder.account) {
                live.wants.folders.insert(*id);
            }
        }
    }

    fn want_replay(&mut self, accounts: &BTreeSet<String>) {
        for account in accounts {
            if let Some(live) = self.live.get_mut(account) {
                live.wants.replay = true;
            }
        }
    }

    /// Closes the offers whose window has ended, and has their accounts
    /// replay what the window held back. Returns the ids it closed.
    pub(super) fn expire_undos(&mut self, now: i64) -> Vec<UndoId> {
        let mut closed = Vec::new();
        for (undo, offer) in self.undo.expire(now) {
            closed.push(undo);
            self.close_offer(undo, offer.what);
        }
        closed
    }

    /// What the end of one offer's window does.
    fn close_offer(&mut self, undo: UndoId, what: Offered) {
        match what {
            Offered::Actions {
                undos, accounts, ..
            } => {
                if !undos.is_empty() {
                    self.emit(Event::UndoClosed {
                        undo,
                        undone: false,
                    });
                }
                self.want_replay(&accounts);
            }
            // The held reply becomes due on its own.
            Offered::Answer { .. } => self.emit(Event::UndoClosed {
                undo,
                undone: false,
            }),
            Offered::LocalDrafts { ids, rest } => {
                self.delete_local(&ids);
                match rest {
                    Some(rest) => self.close_offer(undo, *rest),
                    None => self.emit(Event::UndoClosed {
                        undo,
                        undone: false,
                    }),
                }
            }
        }
    }
}

/// The journal line may be on disk without being durable, and the disk is
/// not the reason (disk-full text wins).
fn only_maybe_saved(error: &envio_sync::SyncError) -> bool {
    matches!(error, envio_sync::SyncError::Store(e) if e.may_still_apply() && !e.is_disk_full())
}

/// What the user is told when an action could not be recorded at all.
fn refusal_text(error: &envio_sync::SyncError) -> String {
    match error {
        _ if only_maybe_saved(error) => text::notice::change_may_still_apply(),
        envio_sync::SyncError::Store(e) if e.is_disk_full() => text::notice::disk_full(),
        _ => text::undo::not_done(),
    }
}

fn failed_text(error: &envio_store::StoreError) -> String {
    log::warn!("finding a message to act on: {error}");
    match problem::from_store(error) {
        Problem::DiskFull => text::notice::disk_full(),
        _ => text::undo::not_done(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::command::UserAction;
    use envio_store::{ActionKind, Flags, Folder, FolderId, FolderRole};

    #[test]
    fn a_full_disk_while_recording_says_the_disk_is_full() {
        let full = std::io::Error::from(std::io::ErrorKind::StorageFull);
        let error = envio_sync::SyncError::Store(envio_store::StoreError::Io(full));
        assert_eq!(refusal_text(&error), text::notice::disk_full());
        let other = envio_sync::SyncError::Store(envio_store::StoreError::NotFound);
        assert_eq!(refusal_text(&other), text::undo::not_done());
    }

    fn folder(id: i64, path: &str, role: FolderRole) -> Folder {
        Folder {
            id: FolderId(id),
            account: "a".into(),
            path: path.into(),
            role,
            sync: None,
        }
    }

    #[test]
    fn archive_without_an_archive_folder_is_none() {
        let folders = [folder(1, "INBOX", FolderRole::Inbox)];
        assert_eq!(
            action_kind(UserAction::Archive, FolderRole::Inbox, &folders),
            None
        );
        let with = [folder(2, "All Mail", FolderRole::Archive)];
        assert_eq!(
            action_kind(UserAction::Archive, FolderRole::Inbox, &with),
            Some(ActionKind::Move {
                to: "All Mail".into()
            })
        );
    }

    #[test]
    fn from_drafts_only_delete_and_move_to_trash_are_kinds() {
        let folders = [
            folder(1, "Drafts", FolderRole::Drafts),
            folder(2, "All Mail", FolderRole::Archive),
            folder(3, "Trash", FolderRole::Trash),
            folder(4, "Misc", FolderRole::Other),
        ];
        let from = FolderRole::Drafts;
        assert_eq!(action_kind(UserAction::Archive, from, &folders), None);
        for to in [2, 4] {
            let mv = UserAction::Move { to: FolderId(to) };
            assert_eq!(action_kind(mv, from, &folders), None, "to {to}");
        }
        assert_eq!(
            action_kind(UserAction::Move { to: FolderId(3) }, from, &folders),
            Some(ActionKind::Move { to: "Trash".into() })
        );
        assert_eq!(
            action_kind(UserAction::Delete, from, &folders),
            Some(ActionKind::Delete)
        );
        // From the Inbox nothing changes.
        let inbox = FolderRole::Inbox;
        assert!(action_kind(UserAction::Archive, inbox, &folders).is_some());
        let mv = UserAction::Move { to: FolderId(4) };
        assert!(action_kind(mv, inbox, &folders).is_some());
    }

    #[test]
    fn mark_read_sets_seen() {
        assert_eq!(
            action_kind(UserAction::MarkRead(true), FolderRole::Inbox, &[]),
            Some(ActionKind::SetFlags {
                add: Flags::SEEN,
                remove: Flags::empty()
            })
        );
        assert_eq!(
            action_kind(UserAction::MarkRead(false), FolderRole::Inbox, &[]),
            Some(ActionKind::SetFlags {
                add: Flags::empty(),
                remove: Flags::SEEN
            })
        );
    }
}

#[cfg(test)]
mod worker_tests {
    use super::*;
    use crate::worker::testing::{Sink, open, seed};

    fn offered(sink: &Sink) -> UndoId {
        sink.events()
            .iter()
            .find_map(|e| match e {
                Event::UndoOffer { undo, .. } => Some(*undo),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn a_flag_undo_queues_a_flag_check_of_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        worker.act(&[seeded.message], UserAction::Flag(true));
        assert!(worker.live[&seeded.account].wants.folders.is_empty());
        worker.undo_action(offered(&sink), now_ms());
        let wants = &worker.live[&seeded.account].wants;
        assert!(wants.folders.contains(&seeded.folder));
    }

    #[test]
    fn expiry_sets_replay() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        worker.act(&[seeded.message], UserAction::Flag(true));
        assert!(!worker.live[&seeded.account].wants.replay);
        // Inside the window the action is held back.
        let _ = worker.expire_undos(now_ms());
        assert!(!worker.live[&seeded.account].wants.replay);
        let closed = worker.expire_undos(now_ms() + 60_000);
        assert_eq!(closed.len(), 1);
        assert!(worker.live[&seeded.account].wants.replay);
    }

    #[test]
    fn opening_a_message_replays_when_the_window_ends() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        worker.mark_opened_read(seeded.message);
        let pending: Vec<_> = worker.store.journal.pending().collect();
        assert!(
            pending.iter().any(|a| matches!(
                &a.kind,
                ActionKind::SetFlags { add, .. } if add.contains(Flags::SEEN)
            )),
            "a pending SEEN entry"
        );
        assert!(!worker.live[&seeded.account].wants.replay);
        let _ = worker.expire_undos(now_ms());
        assert!(!worker.live[&seeded.account].wants.replay);
        let closed = worker.expire_undos(now_ms() + 60_000);
        assert_eq!(closed.len(), 1);
        assert!(worker.live[&seeded.account].wants.replay);
    }

    #[test]
    fn an_undo_pressed_inside_the_window_wins_though_the_worker_is_late() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let kind = ActionKind::SetFlags {
            add: Flags::SEEN,
            remove: Flags::empty(),
        };
        let undo = driver::record(&mut worker.store.local(), seeded.message, kind, 0).unwrap();
        // The window ended by the worker's clock, not by the user's.
        let id = worker.undo.offer(Offer {
            what: Offered::Actions {
                undos: vec![undo],
                accounts: BTreeSet::from([seeded.account.clone()]),
                folders: BTreeSet::from([seeded.folder]),
            },
            expires_at_ms: now_ms() - 1,
        });
        worker.undo_action(id, now_ms() - 100);
        assert!(
            sink.events()
                .iter()
                .any(|e| matches!(e, Event::UndoClosed { undo, undone: true } if *undo == id))
        );
    }

    #[test]
    fn a_turn_handles_queued_commands_before_closing_undo_offers() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let kind = ActionKind::SetFlags {
            add: Flags::SEEN,
            remove: Flags::empty(),
        };
        let undo = driver::record(&mut worker.store.local(), seeded.message, kind, 0).unwrap();
        let id = worker.undo.offer(Offer {
            what: Offered::Actions {
                undos: vec![undo],
                accounts: BTreeSet::from([seeded.account.clone()]),
                folders: BTreeSet::from([seeded.folder]),
            },
            expires_at_ms: now_ms() - 1,
        });
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        tx.send(crate::worker::Input::Command((
            crate::api::command::Command::Undo { undo: id },
            now_ms() - 100,
        )))
        .unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tx.send(crate::worker::Input::Command((
            crate::api::command::Command::Shutdown,
            now_ms(),
        )))
        .unwrap();
        runtime.block_on(worker.turn(&mut rx));
        assert!(
            sink.events()
                .iter()
                .any(|e| matches!(e, Event::UndoClosed { undo, undone: true } if *undo == id))
        );
    }

    #[test]
    fn an_undo_just_after_the_window_closes_the_offer_once() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let kind = ActionKind::SetFlags {
            add: Flags::SEEN,
            remove: Flags::empty(),
        };
        let undo = driver::record(&mut worker.store.local(), seeded.message, kind, 0).unwrap();
        let id = worker.undo.offer(Offer {
            what: Offered::Actions {
                undos: vec![undo],
                accounts: BTreeSet::from([seeded.account.clone()]),
                folders: BTreeSet::from([seeded.folder]),
            },
            expires_at_ms: now_ms() - 1,
        });
        worker.undo_action(id, now_ms());
        let closes = sink
            .events()
            .iter()
            .filter(|e| matches!(e, Event::UndoClosed { undo, .. } if *undo == id))
            .count();
        assert_eq!(closes, 1);
    }
}
