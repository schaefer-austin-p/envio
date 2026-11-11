//! What the worker does between commands: timers, which account runs next,
//! and when to wake.

use envio_store::FolderRole;

use crate::health::{AccountStatus, Health};

use super::Worker;
use super::conn::Conn;
use super::plan::{Next, POLL_MS, Step, Timing, Wants, next, steps};

impl Worker<'_> {
    /// Timers that are not accounts' own: a retry in the schedule is due.
    pub(super) fn on_timers(&mut self, now: i64) {
        // Undo expiry and draft saves join this with the commands that
        // create them (Tasks 9 and 12).
        let Some(due) = self.store.schedule.next_due_after(self.handled_due_ms) else {
            return;
        };
        if due <= now {
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
                    if !matches!(live.conn, Conn::Idling(_))
                        && let Err(problem) = self.begin_idle(account, until_ms).await
                    {
                        super::run::failed(self, account, &problem);
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
                    let todo = steps(&live.wants, &folders, disk_full)
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
        let mut wake = self.store.schedule.next_due_after(self.handled_due_ms);
        for live in self.live.values() {
            let at = account_wake(
                self.foreground && !matches!(live.conn, Conn::Idling(_)),
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

/// When an account needs the loop to wake. `needs_timer` is false in the
/// background and in IDLE: the latter wakes the loop with its own future,
/// so it needs none:
/// its IDLE future wakes the loop, and its own deadline is the next full run.
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
    use envio_sync::{Key, Schedule};

    #[test]
    fn an_idling_account_yields_no_wake_time() {
        let health = Health::default();
        // Its full run is overdue, which would spin the loop if it counted.
        assert_eq!(account_wake(false, &health, 0, 10_000), None);
        assert_eq!(account_wake(true, &health, 0, 10_000), Some(0));
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
