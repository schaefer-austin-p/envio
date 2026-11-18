//! The message the reader shows, marked on its row in the message and
//! search models. Not the selection: the two are independent.

use super::{Output, Ui, change};
use crate::models::Model;

impl Ui {
    /// The message the reader shows, if its row is still shown.
    pub fn open_message(&self) -> Option<i64> {
        self.open
    }

    /// Marks `id` open and unmarks the previous one in Messages and
    /// Search. `None` clears; so does an id no model shows. Returns the
    /// row updates.
    pub fn set_open(&mut self, id: Option<i64>) -> Vec<Output> {
        let id = id.filter(|&i| self.shows(i));
        let mut outs = self.unmark_open(|prev| Some(prev) != id);
        if let Some(id) = id {
            self.open = Some(id);
            outs.extend(self.mark_open(id, true));
        }
        self.mirror.track(&outs);
        outs
    }

    /// `set_open(None)` for callers that record the changes themselves.
    pub(super) fn clear_open(&mut self) -> Vec<Output> {
        self.unmark_open(|_| true)
    }

    /// Unmark the open row when `when` agrees; the open id clears with it.
    fn unmark_open(&mut self, when: impl Fn(i64) -> bool) -> Vec<Output> {
        let Some(prev) = self.open.filter(|&p| when(p)) else {
            return Vec::new();
        };
        self.open = None;
        self.mark_open(prev, false)
    }

    fn mark_open(&mut self, id: i64, on: bool) -> Vec<Output> {
        let cur = self.current.as_mut().map(|c| &mut c.shown);
        [
            (Model::Messages, cur.and_then(|s| s.set_open(id, on))),
            (Model::Search, self.found.set_open(id, on)),
        ]
        .into_iter()
        .filter_map(|(m, op)| op.map(|op| change(m, op)))
        .collect()
    }

    /// A row that left its models cannot be open.
    pub(super) fn prune_open(&mut self) {
        if self.open.is_some_and(|i| !self.shows(i)) {
            self.open = None;
        }
    }
}
