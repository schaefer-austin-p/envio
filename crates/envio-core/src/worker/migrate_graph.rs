//! The one-shot move of an Outlook.com account from IMAP to Graph (#342).
//! The steps and their order are `migrate_graph::plan`; this file does the
//! I/O. Every step tolerates having run before, and the step reached is
//! saved in `AccountSettings.migration` before the next one starts, so a
//! restart resumes.
//!
//! The Outbox, the draft files and local-only state are never touched. Only
//! the cache (rows and bodies) goes, and the IMAP token and servers at the
//! end.

use envio_net::oauth::ProviderKind;
use envio_store::{AccountSettings, BackendKind, Migration, SendState};
use envio_sync::UNDO_MS;
use envio_sync::plan::replay::is_due;

use super::Worker;
use super::conn::{Conn, Live};
use crate::api::event::Event;
use crate::api::view::{MigrationStage, MigrationState};
use crate::health::AccountStatus;
use crate::migrate_graph::plan::{self, CLEAR, MigrationStep, REPLAY, SWITCH};
use crate::text;

#[cfg(test)]
mod tests;

impl Worker<'_> {
    /// Starts the move of `account`: step 1, no failed runs.
    pub(crate) fn migrate_begin(&mut self, account: &str) -> bool {
        let Some(entry) = self.store.accounts.get(account) else {
            return false;
        };
        if entry.settings.migration.is_some() || entry.settings.backend != BackendKind::Imap {
            return false;
        }
        let mut settings = entry.settings.clone();
        settings.migration = Some(Migration {
            step: REPLAY,
            failures: 0,
        });
        self.store_account(account, settings)
    }

    /// Looks at every account that is moving and does what is due.
    pub(crate) fn migrate_all(&mut self, now: i64) {
        let moving: Vec<String> = self
            .store
            .accounts
            .list()
            .iter()
            .filter(|a| a.settings.migration.is_some())
            .map(|a| a.key.clone())
            .collect();
        for account in moving {
            self.migrate(&account, now);
        }
        self.retell_dropped();
    }

    fn migrate(&mut self, account: &str, now: i64) {
        let Some(m) = self
            .store
            .accounts
            .get(account)
            .and_then(|a| a.settings.migration)
        else {
            return;
        };
        let pending = self.pending_of(account);
        let accepted = self.accepted_of(account);
        let blocked = self
            .live
            .get(account)
            .is_some_and(|live| !live.health.can_run(now));
        let step = plan::next(&m, pending == 0, accepted > 0, blocked);
        let waits = self.replay_wait(account, now);
        match step {
            MigrationStep::Replay | MigrationStep::Wait => {
                if let Some(live) = self.live.get_mut(account) {
                    // Only when an action is due: while the Undo window or a
                    // backoff holds them all, `migration_wake` has the turn
                    // look again at the due time instead of spinning.
                    live.wants.replay |= step == MigrationStep::Replay && waits.is_none();
                    // Once per tick: a refused Sent copy is no error, so
                    // asking every turn would spin; `migration_wake` has the
                    // turn look again at the next tick.
                    if accepted > 0 && now >= plan::file_due_ms(live.file_armed_ms) {
                        live.wants.file = true;
                        live.file_armed_ms = now;
                    }
                }
            }
            MigrationStep::Stuck => {}
            MigrationStep::Clear => {
                let Some(dropped) = self.migrate_clear(account) else {
                    return;
                };
                self.migrate_switch(account, dropped);
                return;
            }
            MigrationStep::Switch => {
                self.migrate_switch(account, 0);
                return;
            }
            MigrationStep::Done => {
                self.set_migration(account, None);
                return;
            }
        }
        self.show_migration((account, &m), step, (pending, accepted));
    }

    /// When the pending actions of `account` are next due, if none is now.
    fn replay_wait(&self, account: &str, now: i64) -> Option<i64> {
        let mut earliest: Option<i64> = None;
        for action in self.store.journal.pending() {
            if action.target.account != account {
                continue;
            }
            if is_due(action, &self.store.schedule, now) {
                return None;
            }
            let undo_ends = action.at_ms + UNDO_MS;
            let at = if undo_ends > now {
                Some(undo_ends)
            } else {
                self.store.schedule.next_due_after(now)
            };
            earliest = match (earliest, at) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
        }
        earliest
    }

    /// When filing is next asked for, if an `Accepted` item waits and the
    /// tick has not come yet.
    fn file_wait(&self, account: &str, now: i64) -> Option<i64> {
        let at = plan::file_due_ms(self.live.get(account)?.file_armed_ms);
        (at > now && self.accepted_of(account) > 0).then_some(at)
    }

    /// The earliest time a moving account's pending actions fall due, for
    /// the turn's wake timer.
    pub(super) fn migration_wake(&self, now: i64) -> Option<i64> {
        self.store
            .accounts
            .list()
            .iter()
            .filter(|a| a.settings.migration.is_some_and(|m| m.step == REPLAY))
            .flat_map(|a| [self.replay_wait(&a.key, now), self.file_wait(&a.key, now)])
            .flatten()
            .min()
    }

    /// A step 1 run failed: counted, and saved so a restart keeps it.
    pub(crate) fn migrate_failed(&mut self, account: &str) {
        let moving = self
            .store
            .accounts
            .get(account)
            .and_then(|a| a.settings.migration)
            .filter(|m| m.step == REPLAY);
        if let Some(m) = moving {
            self.set_migration(
                account,
                Some(Migration {
                    failures: m.failures + 1,
                    ..m
                }),
            );
        }
    }

    /// A filing run left sent mail unfiled. It counts only with an empty
    /// Journal: with pending actions, Replay owns the counter and a refused
    /// Sent copy must not make them Stuck.
    pub(crate) fn migrate_file_failed(&mut self, account: &str) {
        if self.pending_of(account) == 0 {
            self.migrate_failed(account);
        }
    }

    /// "Switch anyway": the pending actions are dropped at the next step.
    pub(crate) fn switch_anyway(&mut self, account: &str) {
        let stuck = self
            .store
            .accounts
            .get(account)
            .and_then(|a| a.settings.migration)
            .is_some_and(|m| plan::can_switch_anyway(&m));
        if stuck {
            self.set_migration(
                account,
                Some(Migration {
                    step: CLEAR,
                    failures: 0,
                }),
            );
            self.migrate(account, crate::clock::now_ms());
        }
    }

    /// Step 2: the pending actions are dropped (they name IMAP messages),
    /// then the cache's bodies and rows. `None` when a write failed; the
    /// next tick tries again. Returns how many actions were dropped.
    fn migrate_clear(&mut self, account: &str) -> Option<usize> {
        if self.migration_step(account) != Some(CLEAR) && !self.save_step(account, CLEAR) {
            return None;
        }
        let seqs: Vec<u64> = self
            .store
            .journal
            .pending()
            .filter(|a| a.target.account == account)
            .map(|a| a.seq)
            .collect();
        for seq in &seqs {
            if let Err(error) = self.store.journal.done(*seq) {
                log::warn!("account {account}: dropping an action for the move: {error}");
                return None;
            }
        }
        self.clear_cache(account)?;
        Some(seqs.len())
    }

    /// The cache's bodies, then its rows. Bodies first: a crash between the
    /// two leaves rows whose bodies are gone, which the next run clears again.
    fn clear_cache(&mut self, account: &str) -> Option<()> {
        let folders = self.store.db.folders(account).ok()?;
        for folder in &folders {
            self.store.bodies.remove_folder(folder.id).ok()?;
        }
        // Another account's open message keeps its inline images.
        let shown = self
            .store
            .open_message
            .filter(|id| {
                let db = &self.store.db;
                db.message(*id)
                    .and_then(|m| db.folder(m.folder))
                    .is_ok_and(|f| f.account != account)
            })
            .map(|id| id.0.to_string());
        self.store.db.remove_account_cache(account).ok()?;
        super::inline::clear_except(&self.store.paths.inline(), shown.as_deref());
        self.open_failed(Some(account));
        // Open lists drop the rows that are gone.
        self.emit(Event::FoldersChanged {
            account: account.to_owned(),
        });
        self.emit(Event::MessagesChanged {
            folders: folders.iter().map(|f| f.id).collect(),
        });
        Some(())
    }

    /// Steps 3 and 4: the backend, the IMAP token and servers, then a fresh
    /// `Live`.
    fn migrate_switch(&mut self, account: &str, dropped: usize) {
        if self.migration_step(account) != Some(SWITCH) && !self.save_step(account, SWITCH) {
            return;
        }
        let Some(entry) = self.store.accounts.get(account) else {
            return;
        };
        let mut settings: AccountSettings = entry.settings.clone();
        // A sync may have refilled the cache since CLEAR (a retried step 3):
        // it is emptied again before the account becomes Graph.
        if self.clear_cache(account).is_none() {
            return;
        }
        // The IMAP token goes first: a crash leaves step 3 saved, and the
        // next run removes what is left.
        let removed = self
            .store
            .tokens
            .borrow_mut()
            .remove(account, ProviderKind::Microsoft);
        if let Err(error) = removed {
            log::warn!("account {account}: removing the IMAP token: {error}");
            return;
        }
        settings.backend = BackendKind::Graph;
        settings.imap = None;
        settings.smtp = None;
        settings.migration = None;
        settings.dropped_note = u32::try_from(dropped).ok().filter(|n| *n > 0);
        if !self.store_account(account, settings) {
            return;
        }
        self.store.token_cells.forget(account);
        let fresh = self.store.accounts.get(account).map(Live::for_account);
        if let (Some(live), Some(fresh)) = (self.live.get_mut(account), fresh) {
            match &live.conn {
                Conn::Idling(stop) => stop.notify_one(),
                Conn::Opening(handle) => handle.abort(),
                _ => {}
            }
            *live = fresh;
            live.wants.full = true;
        }
        self.tell_done(account, dropped);
    }

    /// The Done page's state, with the dropped-actions notice when any were.
    fn tell_done(&mut self, account: &str, dropped: usize) {
        let (note_title, note_text) = if dropped > 0 {
            (
                text::migrate::dropped_title(dropped),
                text::migrate::dropped_text(),
            )
        } else {
            (String::new(), String::new())
        };
        self.tell_migration(
            account,
            MigrationState {
                stage: MigrationStage::Done,
                title: text::migrate::done_title(),
                text: text::migrate::done_text(),
                hint: text::migrate::done_hint(),
                note_title,
                note_text,
                can_switch: false,
            },
        );
    }

    /// After a restart: the Done page's notice for a move that dropped
    /// changes is told once more, then cleared. In the session that made the
    /// move it was told already and is kept for a restart.
    fn retell_dropped(&mut self) {
        let due: Vec<(String, u32)> = self
            .store
            .accounts
            .list()
            .iter()
            .filter(|a| {
                a.settings.migration.is_none() && !self.migration_shown.contains_key(&a.key)
            })
            .filter_map(|a| a.settings.dropped_note.map(|n| (a.key.clone(), n)))
            .collect();
        for (account, dropped) in due {
            self.tell_done(&account, dropped as usize);
            let Some(entry) = self.store.accounts.get(&account) else {
                continue;
            };
            let mut settings = entry.settings.clone();
            settings.dropped_note = None;
            self.store_account(&account, settings);
        }
    }

    fn show_migration(
        &mut self,
        (account, m): (&str, &Migration),
        step: MigrationStep,
        (pending, accepted): (usize, usize),
    ) {
        let signin = self.status_of(account) == AccountStatus::SignInFailed;
        let can_switch = plan::can_switch_anyway(m);
        let (stage, title, body, hint) = if step == MigrationStep::Stuck && signin {
            (
                MigrationStage::SignIn,
                text::migrate::sign_in_title(),
                text::migrate::sign_in_text(pending),
                text::migrate::switch_hint(pending),
            )
        } else if step == MigrationStep::Stuck && pending == 0 {
            // Only sent mail whose copy in Sent is refused: nothing to drop.
            (
                MigrationStage::Stuck,
                text::migrate::stuck_filing_title(accepted),
                text::migrate::stuck_filing_text(),
                text::migrate::switch_filing_hint(),
            )
        } else if step == MigrationStep::Stuck {
            (
                MigrationStage::Stuck,
                text::migrate::stuck_title(pending),
                text::migrate::stuck_text(),
                text::migrate::switch_hint(pending),
            )
        } else if signin {
            (
                MigrationStage::SignIn,
                text::migrate::sign_in_title(),
                text::migrate::sign_in_text(pending),
                String::new(),
            )
        } else {
            (
                MigrationStage::Moving,
                text::migrate::moving_title(),
                text::migrate::moving_text(pending, accepted),
                text::migrate::moving_hint(),
            )
        };
        self.tell_migration(
            account,
            MigrationState {
                stage,
                title,
                text: body,
                hint,
                note_title: String::new(),
                note_text: String::new(),
                can_switch,
            },
        );
    }

    /// Sends the state unless the page already has it.
    fn tell_migration(&mut self, account: &str, state: MigrationState) {
        if self.migration_shown.get(account) == Some(&state) {
            return;
        }
        self.migration_shown
            .insert(account.to_owned(), state.clone());
        self.emit(Event::Migration {
            account: account.to_owned(),
            state: Some(state),
        });
    }

    fn pending_of(&self, account: &str) -> usize {
        self.store
            .journal
            .pending()
            .filter(|a| a.target.account == account)
            .count()
    }

    fn accepted_of(&self, account: &str) -> usize {
        self.store
            .outbox
            .items()
            .unwrap_or_default()
            .iter()
            .filter(|i| i.meta.account == account && i.meta.state == SendState::Accepted)
            .count()
    }

    fn migration_step(&self, account: &str) -> Option<u8> {
        self.store
            .accounts
            .get(account)
            .and_then(|a| a.settings.migration)
            .map(|m| m.step)
    }

    fn save_step(&mut self, account: &str, step: u8) -> bool {
        self.set_migration(account, Some(Migration { step, failures: 0 }))
    }

    fn set_migration(&mut self, account: &str, migration: Option<Migration>) -> bool {
        let Some(entry) = self.store.accounts.get(account) else {
            return false;
        };
        let mut settings = entry.settings.clone();
        settings.migration = migration;
        self.store_account(account, settings)
    }

    fn store_account(&mut self, account: &str, settings: AccountSettings) -> bool {
        match self.store.accounts.update(account, settings) {
            Ok(()) => true,
            Err(error) => {
                log::warn!("account {account}: saving the move: {error}");
                false
            }
        }
    }
}
