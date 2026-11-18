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
}

static HOOKS: Mutex<Vec<(Point, String, Hook)>> = Mutex::new(Vec::new());

/// Runs `hook` once, on the worker thread, the next time `account` reaches
/// `point`.
pub fn before_next(point: Point, account: &str, hook: impl Fn() + Send + Sync + 'static) {
    if let Ok(mut hooks) = HOOKS.lock() {
        hooks.push((point, account.to_owned(), Arc::new(hook)));
    }
}

pub(crate) fn reached(point: Point, account: &str) {
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

static UNFOCUSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// While `on`, the worker treats the app as not focused even in the
/// foreground, so a test can get alerts without the background (where IDLE
/// stops until Plan 9a Task 4). Process-wide: reset it when done.
pub fn set_unfocused(on: bool) {
    UNFOCUSED.store(on, std::sync::atomic::Ordering::SeqCst);
}

pub(crate) fn unfocused() -> bool {
    UNFOCUSED.load(std::sync::atomic::Ordering::SeqCst)
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
