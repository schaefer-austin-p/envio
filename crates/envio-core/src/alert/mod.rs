//! New-mail alerts: what to show (`plan`, pure) and who shows it (`Poster`,
//! the platform). Not `notice`: `Event::Notice` is an in-app message.

mod plan;

use std::sync::Mutex;

pub use plan::plan;

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
#[derive(Default)]
pub(crate) struct PosterSlot(Mutex<Option<Box<dyn Poster>>>);

impl PosterSlot {
    pub(crate) fn set(&self, poster: Box<dyn Poster>) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = Some(poster);
        }
    }

    pub(crate) fn post(&self, alert: &Alert) {
        if let Ok(slot) = self.0.lock()
            && let Some(poster) = slot.as_ref()
        {
            poster.post(alert);
        }
    }

    pub(crate) fn badge(&self, total_unread: u32) {
        if let Ok(slot) = self.0.lock()
            && let Some(poster) = slot.as_ref()
        {
            poster.badge(total_unread);
        }
    }
}
