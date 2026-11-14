//! The open message as the reader last got it, so the natural sizes of its
//! remote images (the reader reports them once loaded) can cap one with no
//! width that draws wider than the reader (#170). Qt's rich text has no
//! `max-width`, so the core renders again with the sizes known.

use std::collections::HashMap;

use envio_store::{BodyState, MessageRowId};

use super::Worker;
use crate::api::command::ImagesChoice;
use crate::api::event::Event;
use crate::api::view::MessageView;

/// Natural pixel size (width, height) by image URL.
pub(crate) type Sizes = HashMap<String, (u32, u32)>;

pub(crate) struct Shown {
    pub id: MessageRowId,
    /// What the reader asked for: images, width.
    pub choice: (ImagesChoice, u32),
    /// Every size reported for this message.
    pub sizes: Sizes,
    /// The HTML last sent, so a report that changes nothing sends nothing.
    pub html: String,
}

/// Adds `new` to `known`; whether any size was new or different.
pub(crate) fn merge(known: &mut Sizes, new: Sizes) -> bool {
    let mut changed = false;
    for (url, size) in new {
        if known.insert(url, size) != Some(size) {
            changed = true;
        }
    }
    changed
}

impl Worker<'_> {
    /// The sizes known for message `id` (none when another one is shown).
    pub(super) fn sizes_for(&self, id: MessageRowId) -> Sizes {
        self.shown
            .as_ref()
            .filter(|s| s.id == id)
            .map(|s| s.sizes.clone())
            .unwrap_or_default()
    }

    /// The reader opens message `id`: sizes reported for another are
    /// dropped.
    pub(super) fn forget_other_shown(&mut self, id: MessageRowId) {
        if self.shown.as_ref().is_some_and(|s| s.id != id) {
            self.shown = None;
        }
    }

    /// Sends the view of message `id`, remembering it while it is the open
    /// one.
    pub(super) fn show(
        &mut self,
        id: MessageRowId,
        choice: (ImagesChoice, u32),
        view: MessageView,
    ) {
        if self.store.open_message == Some(id) {
            let sizes = self.sizes_for(id);
            self.shown = Some(Shown {
                id,
                choice,
                sizes,
                html: view.html.clone(),
            });
        }
        self.emit(Event::MessageOpened { id, view });
    }

    /// The reader loaded remote images of message `id`: shows it again
    /// when their sizes change how it draws. It is not marked read again,
    /// and the size cap does not apply: the user already opened it.
    pub(super) fn image_sizes(&mut self, id: MessageRowId, sizes: Sizes) {
        if self.store.open_message != Some(id) {
            return;
        }
        let Some(shown) = self.shown.as_mut().filter(|s| s.id == id) else {
            return;
        };
        if !merge(&mut shown.sizes, sizes) {
            return;
        }
        let choice = shown.choice;
        let Ok(message) = self.store.db.message(id) else {
            return;
        };
        if message.body != BodyState::Downloaded {
            return;
        }
        let raw = match self.store.bodies.read(message.folder, message.uid) {
            Ok(raw) => raw,
            Err(e) => return log::warn!("reading a cached body: {e}"),
        };
        let view = self.view(&message, &raw, choice);
        if let Some(shown) = self.shown.as_mut()
            && shown.html != view.html
        {
            shown.html.clone_from(&view.html);
            self.emit(Event::MessageOpened { id, view });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sizes(entries: &[(&str, (u32, u32))]) -> Sizes {
        entries.iter().map(|(u, s)| ((*u).to_owned(), *s)).collect()
    }

    #[test]
    fn merge_says_whether_anything_changed() {
        let mut known = Sizes::new();
        assert!(merge(&mut known, sizes(&[("a", (10, 5))])));
        assert!(!merge(&mut known, sizes(&[("a", (10, 5))])));
        assert!(!merge(&mut known, Sizes::new()));
        assert!(merge(&mut known, sizes(&[("a", (20, 5))])));
        assert!(merge(&mut known, sizes(&[("a", (20, 5)), ("b", (1, 1))])));
        assert_eq!(known.len(), 2);
    }
}
