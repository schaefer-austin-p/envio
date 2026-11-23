//! "Open this account's Inbox", from outside QML: one entry point that the
//! platform glue calls (the desktop notification click today; the phone's
//! URL dispatch, #230, later) and that the app turns into the EnvioCore
//! `openInboxRequested` signal. Which page that opens is the shell's call.

use std::sync::{Mutex, PoisonError};

type Handler = Box<dyn Fn(String) + Send>;

static HANDLER: Mutex<Option<Handler>> = Mutex::new(None);

/// The app's side, set once the core object exists (any thread).
pub(crate) fn set_handler(handler: impl Fn(String) + Send + 'static) {
    *HANDLER.lock().unwrap_or_else(PoisonError::into_inner) = Some(Box::new(handler));
}

/// Asks the app to show `account`'s Inbox. False when nobody listens yet.
pub(crate) fn open_inbox(account: &str) -> bool {
    let guard = HANDLER.lock().unwrap_or_else(PoisonError::into_inner);
    guard.as_ref().is_some_and(|handler| {
        handler(account.to_owned());
        true
    })
}
