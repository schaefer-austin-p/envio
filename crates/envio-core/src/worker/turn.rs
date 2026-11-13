//! What the worker does between commands: timers, which account runs next,
//! and when to wake.

use envio_store::FolderRole;
use envio_sync::Schedule;

use crate::health::{AccountStatus, Health};

use super::Worker;
use super::conn::Conn;
use super::plan::{Env, Next, POLL_MS, Step, Timing, Wants, next, steps};

impl Worker<'_> {
    /// Timers that are not accounts' own: an Undo window ended, or a retry in
    /// the schedule is due.
    pub(super) fn on_timers(&mut self, now: i64) {
        // Undo expiry has its own clock: it must not share the retry
        // watermark.
        self.expire_undos(now);
        if let Some(due) = due_retry(&self.store.schedule, self.handled_due_ms, now) {
            self.handled_due_ms = due;
            for live in self.live.values_mut() {
                live.wants.replay = true;
                live.wants.send = true;
            }
        }
    }

    pub(super) fn disk_full(&mut self, now: i64) -> bool {
        match self.disk_full_until {
            Some(until) if now < until => true,
            Some(_) => {
                self.disk_full_until = None;
                false
            }
            None => false,
        }
    }

    /// The next step to run, round robin over the accounts that have one.
    /// Accounts that should idle are taken into IDLE on the way.
    pub(super) async fn pick(&mut self, now: i64) -> Option<(String, Step)> {
        let disk_full = self.disk_full(now);
        let mut accounts: Vec<String> = self.live.keys().cloned().collect();
        let after = self.last_ran.as_ref().map_or(0, |last| {
            accounts.iter().position(|a| a > last).unwrap_or(0)
        });
        accounts.rotate_left(after);
        for account in accounts {
            let before = self.status_of(&account);
            let picked = self.pick_for(&account, now, disk_full).await;
            self.report(&account, &before);
            if let Some(step) = picked {
                return Some((account, step));
            }
        }
        None
    }

    async fn pick_for(&mut self, account: &str, now: i64, disk_full: bool) -> Option<Step> {
        loop {
            let folders = self.store.db.folders(account).unwrap_or_default();
            let prefetch = self.prefetch_due(account, &folders, (now, disk_full));
            let live = self.live.get_mut(account)?;
            // A message opened in the background is fetched now (and only
            // it), not when the app comes back.
            if !self.foreground
                && !disk_full
                && !live.health.is_paused()
                && live.health.can_run(now)
                && let Some(id) = live.wants.bodies.first().copied()
            {
                if let Conn::Idling(stop) = &live.conn {
                    stop.notify_one();
                    return None;
                }
                return Some(Step::Body(id));
            }
            let timing = Timing {
                foreground: self.foreground,
                can_run: live.health.can_run(now),
                paused: live.health.is_paused(),
                retry_at_ms: live.health.wake_at(),
                next_full_ms: live.next_full_ms,
                has_wants: !live.wants.is_empty() || prefetch.is_some(),
                idle_capable: live.idle_capable
                    && folders.iter().any(|f| f.role == FolderRole::Inbox),
            };
            match next(&timing, now) {
                Next::Sleep { .. } => return None,
                Next::Idle { until_ms } => {
                    if !matches!(live.conn, Conn::Idling(_)) {
                        if let Err(problem) = self.begin_idle(account, until_ms).await {
                            super::run::failed(self, account, "connect", &problem);
                            self.open_failed(Some(account));
                            return None;
                        }
                        // Recovery may have queued the Inbox: decide again.
                        continue;
                    }
                    return None;
                }
                Next::Run => {
                    if let Conn::Idling(stop) = &live.conn {
                        // Its session comes back through `idle_done`.
                        stop.notify_one();
                        return None;
                    }
                    if now >= live.next_full_ms {
                        live.wants.full = true;
                        live.next_full_ms = now + POLL_MS;
                    }
                    if live.listed {
                        live.wants.expand(&folders);
                    }
                    let env = Env {
                        disk_full,
                        relist: live.relist,
                        foreground: self.foreground,
                        paused: live.health.is_paused(),
                        prefetch,
                    };
                    let mut todo = steps(&live.wants, &folders, &env)
                        .into_iter()
                        .find(|step| !(live.listed && *step == Step::Folders));
                    if let (Some(Step::Folder(_)), Some(id)) = (&todo, live.continuing) {
                        // The next part of the folder in progress goes first.
                        todo = Some(Step::Folder(id));
                    }
                    if todo.is_some() {
                        return todo;
                    }
                    // Only downloads were wanted and the disk is full.
                    live.wants = Wants::default();
                    live.listed = false;
                    if live.health.status == AccountStatus::Syncing {
                        live.health.status = AccountStatus::Ready;
                    }
                }
            }
        }
    }

    /// When to look again with nothing to do: the earliest timer.
    pub(super) fn wake_at(&self, now: i64) -> Option<i64> {
        let retry = self.store.schedule.next_due_after(self.handled_due_ms);
        let mut wake = [retry, self.undo.next_expiry()].into_iter().flatten().min();
        for live in self.live.values() {
            let at = account_wake(
                needs_timer(self.foreground, &live.conn),
                &live.health,
                live.next_full_ms,
                now,
            );
            wake = match (wake, at) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
        }
        wake.map(|at| at.max(now))
    }
}

/// Whether an account needs the loop to wake for it: not in the background,
/// and not in IDLE, whose own future wakes the loop.
fn needs_timer(foreground: bool, conn: &Conn) -> bool {
    foreground && !matches!(conn, Conn::Idling(_))
}

/// The retry timer to handle now: the earliest one after the watermark, if
/// it is due.
fn due_retry(schedule: &Schedule, handled_ms: i64, now: i64) -> Option<i64> {
    schedule
        .next_due_after(handled_ms)
        .filter(|due| *due <= now)
}

/// When an account needs the loop to wake, if it needs a timer at all.
fn account_wake(needs_timer: bool, health: &Health, next_full_ms: i64, now: i64) -> Option<i64> {
    if !needs_timer || health.is_paused() {
        None
    } else if health.can_run(now) {
        Some(next_full_ms)
    } else {
        health.wake_at()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    use envio_sync::Key;
    use tokio::sync::Notify;

    use crate::clock::now_ms;
    use crate::worker::testing::{Sink, open, seed};

    #[test]
    fn an_open_in_the_background_is_served() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        assert!(!config.foreground);
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let account = seeded.account.clone();
        // Nothing is wanted: the background stays quiet.
        let idle = runtime.block_on(worker.pick_for(&account, now_ms(), false));
        assert_eq!(idle, None);
        // The reader opens a message that was never downloaded.
        worker.open_message(
            seeded.message,
            crate::api::command::ImagesChoice::Default,
            300,
        );
        let step = runtime.block_on(worker.pick_for(&account, now_ms(), false));
        assert_eq!(step, Some(Step::Body(seeded.message)));
        // Not with a full disk, which has no room for it.
        let full = runtime.block_on(worker.pick_for(&account, now_ms(), true));
        assert_eq!(full, None);
    }

    #[test]
    fn recent_inbox_bodies_are_prefetched_until_skipped() {
        use envio_store::{Envelope, Flags, NewMessage};

        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let account = seeded.account.clone();
        // The seeded message arrived just now.
        let recent = NewMessage {
            uid: 1,
            size: 10,
            envelope: Envelope {
                subject: "hi".into(),
                date: now_ms() / 1_000,
                ..Envelope::default()
            },
            flags: Flags::default(),
            preview: String::new(),
        };
        worker
            .store
            .db
            .upsert_message(seeded.folder, &recent)
            .unwrap();
        worker.live.get_mut(&account).unwrap().next_full_ms = i64::MAX;
        // In the background nothing is downloaded ahead.
        let background = runtime.block_on(worker.pick_for(&account, now_ms(), false));
        assert_eq!(background, None);
        worker.foreground = true;
        let full = runtime.block_on(worker.pick_for(&account, now_ms(), true));
        assert_eq!(full, None, "not with a full disk");
        let step = runtime.block_on(worker.pick_for(&account, now_ms(), false));
        assert_eq!(step, Some(Step::Prefetch(seeded.folder)));
        // A message whose body could not be had is not tried again.
        let live = worker.live.get_mut(&account).unwrap();
        live.prefetch_skip.insert(seeded.message);
        let step = runtime.block_on(worker.pick_for(&account, now_ms(), false));
        assert_eq!(step, None);
    }

    #[test]
    fn a_folder_in_parts_goes_first_and_commands_run_between_parts() {
        use crate::api::command::Command;
        use crate::api::event::Event;
        use crate::worker::Input;
        use envio_store::FolderRole;

        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let account = seeded.account.clone();
        worker.foreground = true;
        // The Inbox sorts first; the Archive is the one with parts left.
        let archive = worker
            .store
            .db
            .upsert_folder(&account, "Archive", FolderRole::Archive)
            .unwrap();
        let live = worker.live.get_mut(&account).unwrap();
        live.listed = true;
        live.next_full_ms = i64::MAX;
        live.wants.folders.insert(seeded.folder);
        live.wants.folders.insert(archive);
        let plain = runtime.block_on(worker.pick_for(&account, now_ms(), false));
        assert_eq!(plain, Some(Step::Folder(seeded.folder)));
        worker.live.get_mut(&account).unwrap().continuing = Some(archive);
        let first = runtime.block_on(worker.pick_for(&account, now_ms(), false));
        assert_eq!(first, Some(Step::Folder(archive)));
        // Between two parts the app asks for the folder list: answered at once.
        let command = Command::ListFolders {
            account: account.clone(),
        };
        runtime.block_on(worker.input(Input::Command((command, now_ms()))));
        assert!(
            sink.events()
                .iter()
                .any(|e| matches!(e, Event::Folders { .. }))
        );
        // And the folder in progress is still the next step.
        let next = runtime.block_on(worker.pick_for(&account, now_ms(), false));
        assert_eq!(next, Some(Step::Folder(archive)));
    }

    #[test]
    fn an_idling_account_yields_no_wake_time() {
        let health = Health::default();
        // Its full run is overdue, which would spin the loop if it counted.
        assert_eq!(account_wake(false, &health, 0, 10_000), None);
        assert_eq!(account_wake(true, &health, 0, 10_000), Some(0));
    }

    #[test]
    fn only_a_foreground_account_outside_idle_needs_a_timer() {
        let idling = Conn::Idling(Rc::new(Notify::new()));
        assert!(!needs_timer(true, &idling));
        assert!(!needs_timer(false, &Conn::Closed));
        assert!(needs_timer(true, &Conn::Closed));
    }

    #[test]
    fn a_retry_is_handled_once_and_only_when_due() {
        let mut schedule = Schedule::default();
        let first = schedule.failed(Key::Action(1), 0);
        let second = schedule.failed(Key::Action(2), 5_000);
        assert_eq!(due_retry(&schedule, i64::MIN, first - 1), None);
        assert_eq!(due_retry(&schedule, i64::MIN, first), Some(first));
        // With the first handled, the second is next, not the first again.
        assert_eq!(due_retry(&schedule, first, second - 1), None);
        assert_eq!(due_retry(&schedule, first, second), Some(second));
    }

    #[test]
    fn a_stale_retry_does_not_hide_a_later_one() {
        // Account 1's entry is never retried (it is paused); account 2's is.
        let mut schedule = Schedule::default();
        let stale = schedule.failed(Key::Action(1), 0);
        let later = schedule.failed(Key::Action(2), 2_000);
        let mut handled = i64::MIN;
        assert_eq!(schedule.next_due_after(handled), Some(stale));
        handled = stale;
        assert_eq!(schedule.next_due_after(handled), Some(later));
        handled = later;
        assert_eq!(schedule.next_due_after(handled), None);
    }
}
