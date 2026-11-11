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

pub(crate) fn setup_started(email: &str) -> Option<SetupFake> {
    let mut fakes = SETUP.lock().ok()?;
    let entry = fakes.iter_mut().find(|(e, _, _)| e == email)?;
    entry.2 += 1;
    Some(entry.1.clone())
}
