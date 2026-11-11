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
