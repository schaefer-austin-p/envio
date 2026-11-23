//! The open message as the reader last got it, so the natural sizes of its
//! remote images (the reader reports them once loaded) can cap one with no
//! width that draws wider than the reader (#170). Qt's rich text has no
//! `max-width`, so the core renders again with the sizes known.

use std::collections::{BTreeSet, HashMap};

use envio_mime::Unsized;
use envio_store::{BodyState, MessageRowId};

use super::Worker;
use crate::api::command::ImagesChoice;
use crate::api::event::Event;
use crate::api::view::{MessageHeader, MessageView};

/// Natural pixel size (width, height) by image URL.
pub(crate) type Sizes = HashMap<String, (u32, u32)>;

/// Most sizes taken from one report; the reader sends at most this many.
pub(crate) const MAX_BATCH: usize = 256;

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
    /// The header last sent (the sender verdict can change without the
    /// HTML changing).
    pub header: MessageHeader,
    /// Key names whose lookup came back `Unavailable` while this message is
    /// open; not looked up again until it is opened again. Carried over by
    /// `show()` for the same id, like `sizes`.
    pub failed: BTreeSet<String>,
    /// The sender was verified during this open: later renders keep it, with
    /// no new hashing or RSA check and no loss of images when the key's TTL
    /// ends. `open_message` starts it again.
    pub verified: bool,
}

/// Adds to `known` the sizes in `new` that are for an image in `open` (at
/// most [`MAX_BATCH`] of them; other keys are dropped whatever their
/// length). Whether one of them is new and caps an image, so a render
/// would change the HTML.
pub(crate) fn merge(known: &mut Sizes, new: Sizes, open: &[Unsized]) -> bool {
    let mut caps = false;
    let wanted = new
        .into_iter()
        .filter(|(url, _)| open.iter().any(|i| i.url == *url))
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

    /// The failed key names for message `id` (none when another one is
    /// shown).
    pub(super) fn failed_for(&self, id: MessageRowId) -> BTreeSet<String> {
        self.store
            .shown
            .as_ref()
            .filter(|s| s.id == id)
            .map(|s| s.failed.clone())
            .unwrap_or_default()
    }

    /// Whether message `id` was verified during this open.
    pub(super) fn verified_for(&self, id: MessageRowId) -> bool {
        self.store
            .shown
            .as_ref()
            .is_some_and(|s| s.id == id && s.verified)
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
        (view, open_images, lookups): (MessageView, Vec<Unsized>, Vec<String>),
    ) {
        if self.store.open_message == Some(id) {
            let sizes = self.sizes_for(id);
            let failed = self.failed_for(id);
            self.store.shown = Some(Shown {
                id,
                choice,
                sizes,
                html: view.html.clone(),
                open_images,
                header: view.header.clone(),
                failed,
                verified: view.header.sender_verified,
            });
        }
        self.emit(Event::MessageOpened { id, view });
        self.request_keys(id, lookups);
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
        self.show_again(id);
    }

    /// The reader's width changed (#192): renders the shown message `id`
    /// again for `width_px` and answers with it (the reader waits for the
    /// answer). It says nothing when `id` is not the open message, and while
    /// the body is still being fetched it only updates the width the
    /// pending open will render at. The images choice, the failed lookups and the verdict of
    /// this open stay, and the message is not marked read.
    pub(super) fn refit_message(&mut self, id: MessageRowId, width_px: u32) {
        if self.store.open_message != Some(id) {
            return;
        }
        let Some(shown) = self.store.shown.as_ref().filter(|s| s.id == id) else {
            // Still opening: the body arrives at the width asked for first.
            if let Some(choice) = self.opening.get_mut(&id) {
                choice.1 = width_px;
            }
            return;
        };
        let choice = (shown.choice.0, width_px);
        let Ok(message) = self.store.db.message(id) else {
            return self.unavailable(id);
        };
        if message.body == BodyState::Missing {
            return self.unavailable(id);
        }
        let raw = match self.store.bodies.read(message.folder, message.uid) {
            Ok(raw) => raw,
            Err(e) => {
                log::warn!("reading a cached body: {e}");
                return self.unavailable(id);
            }
        };
        let (view, open_images, lookups) = self.view(&message, &raw, choice);
        if let Some(shown) = self.store.shown.as_mut() {
            shown.choice = choice;
            shown.verified = view.header.sender_verified;
            shown.html.clone_from(&view.html);
            shown.header.clone_from(&view.header);
            shown.open_images = open_images;
        }
        self.emit(Event::MessageOpened { id, view });
        self.request_keys(id, lookups);
    }

    /// Renders message `id` again from its cached body and sends it when
    /// its HTML or its header changed (a size cap, a key that verified the
    /// sender). It is not marked read again.
    pub(super) fn show_again(&mut self, id: MessageRowId) {
        if self.store.open_message != Some(id) {
            return;
        }
        let Some(shown) = self.store.shown.as_ref().filter(|s| s.id == id) else {
            return;
        };
        let choice = shown.choice;
        let Ok(message) = self.store.db.message(id) else {
            return;
        };
        if message.body == BodyState::Missing {
            return;
        }
        let raw = match self.store.bodies.read(message.folder, message.uid) {
            Ok(raw) => raw,
            Err(e) => return log::warn!("reading a cached body: {e}"),
        };
        let (view, open_images, lookups) = self.view(&message, &raw, choice);
        if let Some(shown) = self.store.shown.as_mut() {
            shown.verified = view.header.sender_verified;
        }
        if let Some(shown) = self.store.shown.as_mut()
            && (shown.html != view.html || shown.header != view.header)
        {
            shown.html.clone_from(&view.html);
            shown.header.clone_from(&view.header);
            shown.open_images = open_images;
            self.emit(Event::MessageOpened { id, view });
        }
        self.request_keys(id, lookups);
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
        let long = format!("https://e.test/{}", "x".repeat(4096));
        assert!(!merge(
            &mut known,
            sizes(&[("elsewhere", (1000, 5))]),
            &open
        ));
        assert!(!merge(&mut known, sizes(&[(&long, (1000, 5))]), &open));
        assert!(known.is_empty());
    }

    #[test]
    fn merge_caps_an_own_image_with_a_long_url() {
        let long = format!("https://e.test/{}", "x".repeat(4096));
        let open = open(&[long.as_str()]);
        let mut known = Sizes::new();
        assert!(merge(&mut known, sizes(&[(&long, (1000, 5))]), &open));
        assert_eq!(known.get(&long), Some(&(1000, 5)));
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
