//! New-mail alerts: what to show (`plan`, pure) and who shows it (`Poster`,
//! the platform). Not `notice`: `Event::Notice` is an in-app message.

mod plan;
mod postal;
mod tally;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

pub use plan::plan;
pub(crate) use tally::Tally;

/// A card for the platform to show. The text is final and translated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    /// Cards with the same tag replace each other (one per account).
    pub tag: String,
    pub summary: String,
    pub body: String,
    /// The account key, so a tap can open its Inbox.
    pub account: String,
}

/// What the platform does with an alert and the launcher badge. It decides
/// nothing: the core already chose what to show. Called on the worker
/// thread, so it must not block.
pub trait Poster: Send {
    fn post(&self, alert: &Alert);
    /// The Inbox unread count over all accounts; 0 clears the badge.
    fn badge(&self, total_unread: u32);
}

/// Where the worker finds the poster; set after the core starts, and kept
/// across worker restarts. Until one is set, alerts and badges go nowhere.
/// The poster is called outside the slot's lock, so `set` never waits for a
/// call, and one that panicked does not end alerts for good.
#[derive(Default)]
pub(crate) struct PosterSlot {
    poster: Mutex<Option<Shared>>,
    /// The last `AppActive` the worker saw: a worker the supervisor restarts
    /// starts from it, not from the `Config` (the app may have moved to the
    /// background since).
    foreground: Mutex<Option<bool>>,
    /// New mail alerted since the app was last on screen, per account. Here
    /// so a restarted worker continues the count: the user saw nothing.
    unseen: Mutex<Tally>,
}

type Shared = Arc<Mutex<Box<dyn Poster>>>;

fn unpoisoned<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(PoisonError::into_inner)
}

impl PosterSlot {
    pub(crate) fn set(&self, poster: Box<dyn Poster>) {
        *unpoisoned(&self.poster) = Some(Arc::new(Mutex::new(poster)));
    }

    pub(crate) fn set_foreground(&self, foreground: bool) {
        *unpoisoned(&self.foreground) = Some(foreground);
    }

    pub(crate) fn foreground(&self) -> Option<bool> {
        *unpoisoned(&self.foreground)
    }

    /// Adds alerted mail; returns the account's total unseen.
    pub(crate) fn add_unseen(
        &self,
        mail: &crate::api::event::NewMail,
    ) -> crate::api::event::NewMail {
        unpoisoned(&self.unseen).add(mail)
    }

    /// The app is on screen: nothing is unseen.
    pub(crate) fn reset_unseen_all(&self) {
        unpoisoned(&self.unseen).reset_all();
    }

    /// The account's switch went off.
    pub(crate) fn reset_unseen(&self, account: &str) {
        unpoisoned(&self.unseen).reset(account);
    }

    fn current(&self) -> Option<Shared> {
        unpoisoned(&self.poster).clone()
    }

    pub(crate) fn post(&self, alert: &Alert) {
        if let Some(poster) = self.current() {
            unpoisoned(&poster).post(alert);
        }
    }

    pub(crate) fn badge(&self, total_unread: u32) {
        if let Some(poster) = self.current() {
            unpoisoned(&poster).badge(total_unread);
        }
    }
}
