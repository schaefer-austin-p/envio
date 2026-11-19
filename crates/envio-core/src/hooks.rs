//! Test hooks (feature `test-hooks`): a pause at one point of the worker,
//! so a test can make something happen exactly there.

use std::sync::{Arc, Mutex};

type Hook = Arc<dyn Fn() + Send + Sync>;

/// Where the worker can be paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Point {
    /// The account's Inbox was synced; its SELECT for IDLE comes next.
    IdleSelect,
    /// A message body is about to be fetched for the reader.
    BodyFetch,
    /// The server accepted IDLE: mail that arrives now is reported.
    IdleEntered,
    /// The Inbox is selected and the IDLE command is about to be sent:
    /// mail that arrives now is in the gap the server does not report.
    IdleBegin,
}

static RENEWALS: Mutex<Vec<(String, i64)>> = Mutex::new(Vec::new());

/// From now on, `account`'s IDLE ends after at most `ms`, so a test sees
/// the quiet wake and re-SELECT without waiting for the poll interval.
pub fn cap_idle_renewal(account: &str, ms: i64) {
    if let Ok(mut caps) = RENEWALS.lock() {
        caps.retain(|(key, _)| key != account);
        caps.push((account.to_owned(), ms));
    }
}

pub(crate) fn idle_renewal_cap(account: &str) -> Option<i64> {
    RENEWALS.lock().ok().and_then(|caps| {
        caps.iter()
            .find(|(key, _)| key == account)
            .map(|(_, ms)| *ms)
    })
}

static HOOKS: Mutex<Vec<(Point, String, Hook)>> = Mutex::new(Vec::new());

/// Runs `hook` once, on the worker thread, the next time `account` reaches
/// `point`.
pub fn before_next(point: Point, account: &str, hook: impl Fn() + Send + Sync + 'static) {
    if let Ok(mut hooks) = HOOKS.lock() {
        hooks.push((point, account.to_owned(), Arc::new(hook)));
    }
}

type Counts = Vec<(Point, String, usize)>;
static REACHED: Mutex<Counts> = Mutex::new(Vec::new());

/// How many times `account` has reached `point` so far. A test that must
/// not miss a point the worker may pass before the test can register a hook
/// (the first IDLE after `Connect`) waits for this to grow instead.
pub fn reached_count(point: Point, account: &str) -> usize {
    REACHED.lock().map_or(0, |counts| {
        counts
            .iter()
            .find(|(p, key, _)| *p == point && key == account)
            .map_or(0, |(_, _, n)| *n)
    })
}

pub(crate) fn reached(point: Point, account: &str) {
    if let Ok(mut counts) = REACHED.lock() {
        match counts
            .iter_mut()
            .find(|(p, key, _)| *p == point && key == account)
        {
            Some(entry) => entry.2 += 1,
            None => counts.push((point, account.to_owned(), 1)),
        }
    }
    let hook = HOOKS.lock().ok().and_then(|mut hooks| {
        let at = hooks
            .iter()
            .position(|(p, key, _)| *p == point && key == account)?;
        Some(hooks.remove(at).2)
    });
    if let Some(hook) = hook {
        hook();
    }
}

/// What the setup thread does for one address instead of (or before) its
/// network work: a slow Internet, and a ready answer for `Detect`.
#[derive(Clone)]
pub struct SetupFake {
    /// The job waits this long before it runs.
    pub delay: std::time::Duration,
    /// The answer of `Detect`; `None` runs the real lookups after the wait.
    pub found: Option<envio_net::autoconfig::Found>,
}

type Fakes = Vec<(String, SetupFake, usize)>;
static SETUP: Mutex<Fakes> = Mutex::new(Vec::new());

/// From now on, setup jobs for `email` are made slow (and answered) as
/// `fake` says.
pub fn fake_setup(email: &str, fake: SetupFake) {
    if let Ok(mut fakes) = SETUP.lock() {
        fakes.retain(|(e, _, _)| e != email);
        fakes.push((email.to_owned(), fake, 0));
    }
}

/// How many setup jobs for `email` have started since `fake_setup`.
pub fn setup_runs(email: &str) -> usize {
    SETUP.lock().ok().map_or(0, |fakes| {
        fakes
            .iter()
            .find(|(e, _, _)| e == email)
            .map_or(0, |(_, _, runs)| *runs)
    })
}

static JOURNAL_FAILS: Mutex<Vec<std::path::PathBuf>> = Mutex::new(Vec::new());

/// The next journal write of the store whose journal is at `journal` fails
/// to sync after the line is written (`StoreError::MayStillApply`).
pub fn fail_next_journal_sync(journal: std::path::PathBuf) {
    if let Ok(mut fails) = JOURNAL_FAILS.lock() {
        fails.push(journal);
    }
}

static COMPACT_FAILS: Mutex<Vec<std::path::PathBuf>> = Mutex::new(Vec::new());

/// The next compact of the journal at `journal` fails (a replay then ends
/// with a store error after its commits).
pub fn fail_next_journal_compact(journal: std::path::PathBuf) {
    if let Ok(mut fails) = COMPACT_FAILS.lock() {
        fails.push(journal);
    }
}

pub(crate) fn take_compact_failure(journal: &std::path::Path) -> bool {
    COMPACT_FAILS.lock().is_ok_and(|mut fails| {
        let before = fails.len();
        fails.retain(|p| p != journal);
        fails.len() != before
    })
}

pub(crate) fn take_journal_failure(journal: &std::path::Path) -> bool {
    JOURNAL_FAILS.lock().is_ok_and(|mut fails| {
        let before = fails.len();
        fails.retain(|p| p != journal);
        fails.len() != before
    })
}

pub(crate) fn setup_started(email: &str) -> Option<SetupFake> {
    let mut fakes = SETUP.lock().ok()?;
    let entry = fakes.iter_mut().find(|(e, _, _)| e == email)?;
    entry.2 += 1;
    Some(entry.1.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_reached_before_anyone_looked_is_still_counted() {
        assert_eq!(reached_count(Point::IdleEntered, "counted"), 0);
        reached(Point::IdleEntered, "counted");
        assert_eq!(reached_count(Point::IdleEntered, "counted"), 1);
        reached(Point::IdleEntered, "counted");
        reached(Point::IdleSelect, "counted");
        assert_eq!(reached_count(Point::IdleEntered, "counted"), 2);
        assert_eq!(reached_count(Point::IdleEntered, "other"), 0);
    }
}
