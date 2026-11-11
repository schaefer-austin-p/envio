//! What the worker does between commands: timers, which account runs next,
//! and when to wake.

use envio_store::FolderRole;
use envio_sync::Schedule;

use crate::health::{AccountStatus, Health};

use super::Worker;
use super::conn::Conn;
use super::plan::{Next, POLL_MS, Step, Timing, Wants, next, steps};

impl Worker<'_> {
    /// Timers that are not accounts' own: an Undo window ended, or a retry in
    /// the schedule is due.
    pub(super) fn on_timers(&mut self, now: i64) {
        // Undo expiry and draft saves have their own clocks: they must not
        // share the retry watermark.
        self.expire_undos(now);
        self.save_due_drafts(now);
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
            let live = self.live.get_mut(account)?;
            let timing = Timing {
                foreground: self.foreground,
                can_run: live.health.can_run(now),
                paused: live.health.is_paused(),
                retry_at_ms: live.health.wake_at(),
                next_full_ms: live.next_full_ms,
                has_wants: !live.wants.is_empty(),
                idle_capable: live.idle_capable
                    && folders.iter().any(|f| f.role == FolderRole::Inbox),
            };
            match next(&timing, now) {
                Next::Sleep { .. } => return None,
                Next::Idle { until_ms } => {
                    if !matches!(live.conn, Conn::Idling(_)) {
                        if let Err(problem) = self.begin_idle(account, until_ms).await {
                            super::run::failed(self, account, &problem);
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
                    let todo = steps(&live.wants, &folders, disk_full, live.relist)
                        .into_iter()
                        .find(|step| !(live.listed && *step == Step::Folders));
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
        let mut wake = [retry, self.undo.next_expiry(), self.throttle.next_due()]
            .into_iter()
            .flatten()
            .min();
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
