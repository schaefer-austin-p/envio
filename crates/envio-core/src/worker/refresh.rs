//! Telling the app a refresh is over (`Event::Refreshed`).
//!
//! There is no step queue to watch (`Worker::pick` derives one step at a time
//! from each account's wants), so a refresh is over when none of the
//! accounts it asked for is still pending: it has no sync wanted, or cannot
//! run now, or is gone.

use std::collections::BTreeSet;

use crate::api::event::Event;
use crate::clock::now_ms;
use crate::health::Health;

use super::Worker;
use super::plan::Wants;

/// Whether an account still has a refresh's sync ahead of it: it wants the
/// full pass, some folder or the Drafts upload, and is neither paused nor waiting out a retry.
/// The connection is not an input: an account in IDLE with `full` set leaves
/// IDLE and syncs.
pub(super) fn refresh_pending(wants: &Wants, health: &Health, now_ms: i64) -> bool {
    (wants.full || !wants.folders.is_empty() || wants.drafts)
        && !health.is_paused()
        && health.can_run(now_ms)
}

impl Worker<'_> {
    /// A refresh of `accounts` was asked for: opens its ack, holding those
    /// of them that are pending now.
    pub(super) fn open_refresh(&mut self, accounts: &[String], ticket: u64) {
        let now = now_ms();
        let waiting = accounts
            .iter()
            .filter(|key| {
                self.live
                    .get(*key)
                    .is_some_and(|live| refresh_pending(&live.wants, &live.health, now))
            })
            .cloned()
            .collect();
        self.refreshes.push((ticket, waiting));
    }

    /// Closes the open refreshes that nothing waits for, one `Refreshed`
    /// each. It only reads state (an account that left `live`, or whose
    /// wants a failure, a reset or a disk-full cleared, is no longer
    /// pending), so one call per turn of the loop covers every way a
    /// refresh can end, and calling it again does nothing.
    pub(super) fn settle_refreshes(&mut self) {
        if self.refreshes.is_empty() {
            return;
        }
        let now = now_ms();
        let live = &self.live;
        let still = |key: &String| {
            live.get(key)
                .is_some_and(|l| refresh_pending(&l.wants, &l.health, now))
        };
        let mut done = Vec::new();
        let open: Vec<(u64, BTreeSet<String>)> = std::mem::take(&mut self.refreshes)
            .into_iter()
            .filter_map(|(ticket, mut set)| {
                set.retain(&still);
                if set.is_empty() {
                    done.push(ticket);
                    None
                } else {
                    Some((ticket, set))
                }
            })
            .collect();
        self.refreshes = open;
        for ticket in done {
            self.emit(Event::Refreshed { ticket });
        }
    }
}

#[cfg(test)]
mod tests;
