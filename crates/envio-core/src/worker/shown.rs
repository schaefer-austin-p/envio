//! The open message as the reader last got it, so the natural sizes of its
//! remote images (the reader reports them once loaded) can cap one with no
//! width that draws wider than the reader (#170). Qt's rich text has no
//! `max-width`, so the core renders again with the sizes known.

use std::collections::HashMap;

use envio_mime::Unsized;
use envio_store::{BodyState, MessageRowId};

use super::Worker;
use crate::api::command::ImagesChoice;
use crate::api::event::Event;
use crate::api::view::MessageView;

/// Natural pixel size (width, height) by image URL.
pub(crate) type Sizes = HashMap<String, (u32, u32)>;

/// Most sizes taken from one report; the reader sends at most this many.
pub(crate) const MAX_BATCH: usize = 256;
/// Longest URL a size is kept for, in bytes.
pub(crate) const MAX_URL_BYTES: usize = 2048;

pub(crate) struct Shown {
    pub id: MessageRowId,
    /// What the reader asked for: images, width.
    pub choice: (ImagesChoice, u32),
    /// The sizes reported for this message's images.
    pub sizes: Sizes,
    /// The HTML last sent, so a report that changes nothing sends nothing.
    pub html: String,
    /// The remote images that HTML has with no width.
    pub open_images: Vec<Unsized>,
}

/// Adds to `known` the sizes in `new` that are for an image in `open` (at
/// most [`MAX_BATCH`] of them, URLs up to [`MAX_URL_BYTES`]). Whether one
/// of them is new and caps an image, so a render would change the HTML.
pub(crate) fn merge(known: &mut Sizes, new: Sizes, open: &[Unsized]) -> bool {
    let mut caps = false;
    let wanted = new
        .into_iter()
        .filter(|(url, _)| url.len() <= MAX_URL_BYTES && open.iter().any(|i| i.url == *url))
        .take(MAX_BATCH);
    for (url, size) in wanted {
        let capping = open.iter().any(|i| i.url == url && i.caps(size));
        if known.insert(url, size) != Some(size) && capping {
            caps = true;
        }
    }
    caps
}

impl Worker<'_> {
    /// The sizes known for message `id` (none when another one is shown).
    pub(super) fn sizes_for(&self, id: MessageRowId) -> Sizes {
        self.store
            .shown
            .as_ref()
            .filter(|s| s.id == id)
            .map(|s| s.sizes.clone())
            .unwrap_or_default()
    }

    /// The reader opens message `id`: sizes reported for another are
    /// dropped.
    pub(super) fn forget_other_shown(&mut self, id: MessageRowId) {
        if self.store.shown.as_ref().is_some_and(|s| s.id != id) {
            self.store.shown = None;
        }
    }

    /// Sends the view of message `id`, remembering it while it is the open
    /// one.
    pub(super) fn show(
        &mut self,
        id: MessageRowId,
        choice: (ImagesChoice, u32),
        (view, open_images): (MessageView, Vec<Unsized>),
    ) {
        if self.store.open_message == Some(id) {
            let sizes = self.sizes_for(id);
            self.store.shown = Some(Shown {
                id,
                choice,
                sizes,
                html: view.html.clone(),
                open_images,
            });
        }
        self.emit(Event::MessageOpened { id, view });
    }

    /// The reader loaded remote images of message `id`: shows it again
    /// when their sizes cap one. It is not marked read again, and the size
    /// cap does not apply: the user already opened it.
    pub(super) fn image_sizes(&mut self, id: MessageRowId, sizes: Sizes) {
        if self.store.open_message != Some(id) {
            return;
        }
        let Some(shown) = self.store.shown.as_mut().filter(|s| s.id == id) else {
            return;
        };
        if !merge(&mut shown.sizes, sizes, &shown.open_images) {
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
        let (view, open_images) = self.view(&message, &raw, choice);
        if let Some(shown) = self.store.shown.as_mut()
            && shown.html != view.html
        {
            shown.html.clone_from(&view.html);
            shown.open_images = open_images;
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

    fn open(urls: &[&str]) -> Vec<Unsized> {
        urls.iter()
            .map(|u| Unsized {
                url: (*u).to_owned(),
                room: 300,
                height: None,
            })
            .collect()
    }

    #[test]
    fn merge_says_whether_a_new_size_caps_an_image() {
        let open = open(&["a", "b"]);
        let mut known = Sizes::new();
        assert!(merge(&mut known, sizes(&[("a", (1000, 5))]), &open));
        assert!(!merge(&mut known, sizes(&[("a", (1000, 5))]), &open));
        assert!(!merge(&mut known, Sizes::new(), &open));
        assert!(merge(&mut known, sizes(&[("a", (2000, 5))]), &open));
        // Small enough to fit: kept, but nothing to render.
        assert!(!merge(&mut known, sizes(&[("b", (300, 5))]), &open));
        assert_eq!(known.len(), 2);
    }

    #[test]
    fn merge_keeps_only_the_messages_open_images() {
        let open = open(&["a"]);
        let mut known = Sizes::new();
        let long = format!("{}a", "x".repeat(MAX_URL_BYTES));
        assert!(!merge(
            &mut known,
            sizes(&[("elsewhere", (1000, 5))]),
            &open
        ));
        assert!(!merge(&mut known, sizes(&[(&long, (1000, 5))]), &open));
        assert!(known.is_empty());
    }

    #[test]
    fn merge_takes_a_bounded_batch() {
        let urls: Vec<String> = (0..10_000).map(|i| format!("https://e.test/{i}")).collect();
        let refs: Vec<&str> = urls.iter().map(String::as_str).collect();
        let open = open(&refs);
        let many: Sizes = urls.iter().map(|u| (u.clone(), (1000, 5))).collect();
        let mut known = Sizes::new();
        assert!(merge(&mut known, many, &open));
        assert_eq!(known.len(), MAX_BATCH);
        // Junk keys don't use up the batch.
        let mut junk: Sizes = (0..10_000)
            .map(|i| (format!("junk{i}"), (1000, 5)))
            .collect();
        junk.insert(urls[0].clone(), (1000, 5));
        let mut known = Sizes::new();
        assert!(merge(&mut known, junk, &open));
        assert_eq!(known.len(), 1);
    }
}
