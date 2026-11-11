//! The user's actions on messages and the Undo window around them.

use std::collections::BTreeSet;

use envio_store::{ActionKind, Flags, Folder, FolderId, FolderRole, MessageRowId};
use envio_sync::UNDO_MS;
use envio_sync::driver::{self, Undo};

use super::Worker;
use crate::api::command::{UndoId, UserAction};
use crate::api::event::Event;
use crate::clock::now_ms;
use crate::problem::{self, Problem};
use crate::text;
use crate::undo::Offer;

/// The journal entry for `action`, given the message's account's folders.
/// `None` when it has no target there.
pub(crate) fn action_kind(action: UserAction, folders: &[Folder]) -> Option<ActionKind> {
    let flags = |add: Flags, remove: Flags| Some(ActionKind::SetFlags { add, remove });
    let move_to = |found: Option<&Folder>| found.map(|f| ActionKind::Move { to: f.path.clone() });
    match action {
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
}

impl Done {
    fn count(&self) -> usize {
        self.undos.len() + self.kept
    }
}

impl Worker<'_> {
    pub(super) fn act(&mut self, ids: &[MessageRowId], action: UserAction) {
        let now = now_ms();
        let mut done = Done {
            now,
            ..Done::default()
        };
        for id in ids {
            self.act_one(*id, action, &mut done);
        }
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

    fn offer_undo(&mut self, action: UserAction, done: Done, now: i64) {
        let shown = !done.undos.is_empty();
        let text = text::undo::offer(action, done.count());
        let expires_at_ms = now + UNDO_MS;
        let undo = self.undo.offer(Offer {
            undos: done.undos,
            accounts: done.accounts,
            folders: done.folders,
            expires_at_ms,
        });
        // With nothing to take back the entry only times the replay.
        if shown {
            self.emit(Event::UndoOffer {
                undo,
                text,
                expires_at_ms,
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
        let Some(kind) = action_kind(action, &folders) else {
            done.refused.get_or_insert_with(|| match action {
                UserAction::Archive => text::undo::no_archive(),
                _ => text::undo::not_done(),
            });
            return;
        };
        let newest = self.store.journal.pending().map(|a| a.seq).max();
        match driver::record(&mut self.store.local(), id, kind, done.now) {
            Ok(undo) => done.undos.push(undo),
            Err(error) => {
                log::warn!("recording an action: {error}");
                // An error after the journal write leaves the action
                // pending: it is done, just not undoable (#41).
                if self.store.journal.pending().map(|a| a.seq).max() == newest {
                    done.refused.get_or_insert_with(text::undo::not_done);
                    return;
                }
                done.kept += 1;
            }
        }
        done.accounts.insert(folder.account);
        done.folders.insert(folder.id);
    }

    pub(super) fn undo_action(&mut self, undo: UndoId) {
        let now = now_ms();
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
        let mut undone = 0;
        let mut late = 0;
        let mut failed = false;
        for item in &offer.undos {
            match driver::undo(&mut self.store.local(), item) {
                Ok(true) => undone += 1,
                Ok(false) => late += 1,
                Err(error) => {
                    log::warn!("undoing an action: {error}");
                    failed = true;
                }
            }
        }
        if undone > 0 {
            self.emit(Event::MessagesChanged {
                folders: offer.folders.iter().copied().collect(),
            });
            self.check_flags_in(&offer.folders);
        }
        self.emit(Event::UndoClosed {
            undo,
            undone: undone > 0,
        });
        if failed {
            // Those stay pending: replay them without waiting for a poll.
            self.want_replay(&offer.accounts);
            self.emit(Event::Notice {
                text: text::undo::cannot_undo(),
            });
        } else if late > 0 {
            self.emit(Event::Notice {
                text: text::undo::too_late(),
            });
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
            if !offer.undos.is_empty() {
                self.emit(Event::UndoClosed {
                    undo,
                    undone: false,
                });
            }
            self.want_replay(&offer.accounts);
        }
        closed
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
        assert_eq!(action_kind(UserAction::Archive, &folders), None);
        let with = [folder(2, "All Mail", FolderRole::Archive)];
        assert_eq!(
            action_kind(UserAction::Archive, &with),
            Some(ActionKind::Move {
                to: "All Mail".into()
            })
        );
    }

    #[test]
    fn mark_read_sets_seen() {
        assert_eq!(
            action_kind(UserAction::MarkRead(true), &[]),
            Some(ActionKind::SetFlags {
                add: Flags::SEEN,
                remove: Flags::empty()
            })
        );
        assert_eq!(
            action_kind(UserAction::MarkRead(false), &[]),
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
        worker.undo_action(offered(&sink));
        let wants = &worker.live[&seeded.account].wants;
        assert!(wants.folders.contains(&seeded.folder));
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
            undos: vec![undo],
            accounts: BTreeSet::from([seeded.account.clone()]),
            folders: BTreeSet::from([seeded.folder]),
            expires_at_ms: now_ms() - 1,
        });
        worker.undo_action(id);
        let closes = sink
            .events()
            .iter()
            .filter(|e| matches!(e, Event::UndoClosed { undo, .. } if *undo == id))
            .count();
        assert_eq!(closes, 1);
    }
}
