//! What the worker does between commands: timers, which account runs next,
//! and when to wake.

use envio_store::FolderRole;
use envio_sync::Schedule;

use crate::health::{AccountStatus, Health};

use super::Worker;
use envio_sync::plan::outbox::{next_release, next_resolve};

use super::conn::Conn;
use super::plan::{
    BACKGROUND_POLL_MS, Env, Next, POLL_MS, Step, Timing, Wants, background_steps, idle_problem,
    may_idle, next, prefetch_step, steps,
};

impl Worker<'_> {
    /// Timers that are not accounts' own: an Undo window ended, or a retry in
    /// the schedule is due.
    pub(super) fn on_timers(&mut self, now: i64) {
        // Undo expiry has its own clock: it must not share the retry
        // watermark.
        self.expire_undos(now);
        self.oauth_timer(now);
        if let Some(at) = self.next_release().filter(|at| *at <= now) {
            // A held message may go now.
            self.handled_release_ms = at;
            for live in self.live.values_mut() {
                live.wants.send = true;
            }
        }
        if let Some(at) = self.next_resolve().filter(|at| *at <= now) {
            // An unconfirmed send is looked for in Sent again.
            self.handled_resolve_ms = at;
            for live in self.live.values_mut() {
                live.wants.file = true;
            }
        }
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
        let mut backstop = Backstop::default();
        loop {
            let folders = self.store.db.folders(account).unwrap_or_default();
            let prefetch = self.prefetch_folder(account, &folders, now);
            let live = self.live.get_mut(account)?;
            // A message opened in the background is fetched now (and only
            // it), not when the app comes back.
            if !self.foreground
                && !disk_full
                && !live.health.is_paused()
                && live.health.can_run(now)
                && let Some(step) = live
                    .wants
                    .bodies
                    .first()
                    .map(|id| Step::Body(*id))
                    .or_else(|| live.wants.keywords.first().map(|id| Step::Keywords(*id)))
            {
                if let Conn::Idling(stop) = &live.conn {
                    stop.notify_one();
                    return None;
                }
                return Some(step);
            }
            // The one prefetch rule: `has_wants` and `steps` both ask it.
            let env = Env {
                disk_full,
                relist: live.relist,
                foreground: self.foreground,
                paused: live.health.is_paused(),
                prefetch,
            };
            let inbox_wanted = !disk_full && !background_steps(&live.wants, &folders).is_empty();
            let notifies = !self.store.settings.notify_off.contains(account);
            let timing = Timing {
                foreground: self.foreground,
                can_run: live.health.can_run(now),
                paused: live.health.is_paused(),
                retry_at_ms: live.health.wake_at(),
                next_full_ms: live.next_full_ms,
                has_wants: forced_wants()
                    || if self.foreground {
                        !live.wants.is_empty() || prefetch_step(&env).is_some()
                    } else {
                        inbox_wanted
                    },
                idle_capable: may_idle(
                    live.idle_capable,
                    folders.iter().any(|f| f.role == FolderRole::Inbox),
                ),
                notifies,
                poll_ms: (notifies && live.idle_capable == Some(false))
                    .then_some(BACKGROUND_POLL_MS),
            };
            match next(&timing, now) {
                Next::Sleep { .. } => return None,
                Next::Idle { until_ms } => {
                    #[cfg(feature = "test-hooks")]
                    let until_ms = crate::hooks::idle_renewal_cap(account)
                        .map_or(until_ms, |cap| until_ms.min(now + cap));
                    if !matches!(live.conn, Conn::Idling(_) | Conn::Opening(_))
                        && let Err(problem) = self.begin_open(account, until_ms)
                    {
                        let problem = idle_problem(problem, self.foreground);
                        super::run::failed(self, account, "connect", &problem);
                        self.open_failed(Some(account));
                    }
                    // The connect runs as a future of the turn loop; its
                    // end decides again (`open_done`).
                    return None;
                }
                Next::Run => {
                    if let Conn::Idling(stop) = &live.conn {
                        // Its session comes back through `idle_done`.
                        stop.notify_one();
                        return None;
                    }
                    if !env.foreground {
                        // A poll that is due: the clock moves on first,
                        // whatever the step does, so a failing one cannot
                        // spin. The Inbox is then wanted like after a wake.
                        if timing.poll_ms.is_some() && now >= live.next_full_ms {
                            live.next_full_ms = now + BACKGROUND_POLL_MS;
                            if !disk_full
                                && let Some(inbox) =
                                    folders.iter().find(|f| f.role == FolderRole::Inbox)
                            {
                                live.wants.folders.insert(inbox.id);
                            }
                        }
                        // Only the Inbox, after a wake: the rest waits for
                        // the app to come back.
                        let todo = background_steps(&live.wants, &folders).into_iter().next();
                        match backstop.plan(timing.has_wants, todo.is_none()) {
                            Pass::Run => return todo,
                            Pass::Stop => {
                                log::error!("planner wanted to run but had no step");
                                return None;
                            }
                            Pass::ResetAndRetry => {
                                live.wants.folders.clear();
                                continue;
                            }
                        }
                    }
                    if now >= live.next_full_ms {
                        live.wants.full = true;
                        live.next_full_ms = now + POLL_MS;
                    }
                    if live.listed {
                        live.wants.expand(&folders);
                    }
                    let mut todo = steps(&live.wants, &folders, &env)
                        .into_iter()
                        .find(|step| !(live.listed && *step == Step::Folders));
                    if let (Some(Step::Folder(_)), Some(id)) = (&todo, live.continuing) {
                        // The next part of the folder in progress goes first.
                        todo = Some(Step::Folder(id));
                    }
                    match backstop.plan(timing.has_wants, todo.is_none()) {
                        Pass::Run => return todo,
                        Pass::Stop => {
                            log::error!("planner wanted to run but had no step");
                            return None;
                        }
                        Pass::ResetAndRetry => {}
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

    /// The next time a held Outbox item may be sent, past the watermark.
    fn next_release(&self) -> Option<i64> {
        let items = self.store.outbox.items().ok()?;
        next_release(items.iter().map(|item| &item.meta), self.handled_release_ms)
    }

    /// The next time an unconfirmed send is looked for, past its watermark.
    fn next_resolve(&self) -> Option<i64> {
        let items = self.store.outbox.items().ok()?;
        next_resolve(items.iter().map(|item| &item.meta), self.handled_resolve_ms)
    }

    /// When to look again with nothing to do: the earliest timer.
    pub(super) fn wake_at(&self, now: i64) -> Option<i64> {
        let retry = self.store.schedule.next_due_after(self.handled_due_ms);
        // An Inbox that waits for the disk in the background.
        let disk = self.disk_full_until.filter(|_| {
            !self.foreground
                && self
                    .live
                    .values()
                    .any(|live| !live.wants.folders.is_empty())
        });
        let mut wake = [
            disk,
            retry,
            self.next_release(),
            self.next_resolve(),
            self.undo.next_expiry(),
            self.oauth_deadline(),
        ]
        .into_iter()
        .flatten()
        .min();
        for (account, live) in &self.live {
            let at = account_wake(
                needs_timer(self.foreground, self.notifies(account), &live.conn),
                &live.health,
                (self.foreground || (self.notifies(account) && live.idle_capable == Some(false)))
                    .then_some(live.next_full_ms),
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

/// What `pick_for` does with the plan `steps` made.
#[derive(Debug, PartialEq, Eq)]
enum Pass {
    /// Run the step.
    Run,
    /// Nothing to run though the planner said run: forget the wants, decide
    /// again.
    ResetAndRetry,
    /// An empty plan again after a reset: give up, or the loop never ends.
    Stop,
}

/// The loop decision of `pick_for`. A reset ends what is wanted, so wants
/// that still disagree with the plan afterwards are a planner bug (#116).
fn after_plan(has_wants: bool, steps_empty: bool, resets: u8) -> Pass {
    if !steps_empty {
        Pass::Run
    } else if has_wants && resets >= 1 {
        Pass::Stop
    } else {
        Pass::ResetAndRetry
    }
}

/// Counts the consecutive empty plans of one `pick_for` call.
#[derive(Default)]
struct Backstop {
    resets: u8,
}

impl Backstop {
    fn plan(&mut self, has_wants: bool, steps_empty: bool) -> Pass {
        let pass = after_plan(has_wants, steps_empty, self.resets);
        if pass == Pass::ResetAndRetry {
            self.resets += 1;
        }
        pass
    }
}

#[cfg(test)]
thread_local! {
    /// Test hook: `Timing.has_wants` is true whatever the planner says.
    static FORCE_WANTS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn forced_wants() -> bool {
    FORCE_WANTS.with(std::cell::Cell::get)
}

#[cfg(not(test))]
fn forced_wants() -> bool {
    false
}

/// Whether an account needs the loop to wake for it: in the foreground, or
/// in the background when it notifies (its retries), and not in IDLE, whose
/// own future wakes the loop.
fn needs_timer(foreground: bool, notifies: bool, conn: &Conn) -> bool {
    (foreground || notifies) && !matches!(conn, Conn::Idling(_) | Conn::Opening(_))
}

/// The retry timer to handle now: the earliest one after the watermark, if
/// it is due.
fn due_retry(schedule: &Schedule, handled_ms: i64, now: i64) -> Option<i64> {
    schedule
        .next_due_after(handled_ms)
        .filter(|due| *due <= now)
}

/// When an account needs the loop to wake, if it needs a timer at all.
fn account_wake(
    needs_timer: bool,
    health: &Health,
    next_full_ms: Option<i64>,
    now: i64,
) -> Option<i64> {
    if !needs_timer || health.is_paused() {
        None
    } else if health.can_run(now) {
        next_full_ms
    } else {
        health.wake_at()
    }
}

#[cfg(test)]
mod tests;
