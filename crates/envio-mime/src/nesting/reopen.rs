//! Formatting elements html5ever reopens after a table mode pops them.
//!
//! When html5ever clears the stack back to a table (or closes a table) it
//! pops formatting elements without taking them off its list of active
//! formatting elements, unless a marker above them (a cell, caption,
//! template, applet, marquee or object) closes too. It reopens them at
//! the next insertion outside the table. The model keeps them counted as
//! a debt on the nearest marker below, or on the document, until that
//! marker closes.

use super::tags::FORMATTING;
use super::{Model, Ns, Open};

const MARKERS: &[&str] = &[
    "td", "th", "caption", "template", "applet", "marquee", "object",
];

impl Model {
    /// Drops `stack[len..]`, and the debt its markers carried.
    pub(super) fn truncate(&mut self, len: usize) {
        let len = len.min(self.stack.len());
        let freed: usize = self.stack[len..].iter().map(|open| open.debt).sum();
        self.debt -= freed;
        self.stack.truncate(len);
    }

    pub(super) fn pop(&mut self) {
        if let Some(len) = self.stack.len().checked_sub(1) {
            self.truncate(len);
        }
    }

    /// A table mode's cut back to `stack[..len]`.
    pub(super) fn cut(&mut self, len: usize) {
        let len = len.min(self.stack.len());
        let kept = self.stack[len..]
            .iter()
            .take_while(|open| !is_marker(open))
            .filter(|open| open.ns == Ns::Html && FORMATTING.contains(&&*open.name))
            .count();
        self.truncate(len);
        if kept == 0 {
            return;
        }
        if let Some(marker) = self.stack.iter().rposition(is_marker) {
            self.stack[marker].debt += kept;
        }
        self.debt += kept;
        self.check_depth();
    }
}

fn is_marker(open: &Open) -> bool {
    open.ns == Ns::Html && !open.inert && MARKERS.contains(&&*open.name)
}
