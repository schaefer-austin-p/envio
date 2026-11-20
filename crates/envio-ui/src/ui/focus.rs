//! The keyboard-focus row of the message list (Up/Down): a cue apart from
//! the open row and the selection. Focus lives in the list shown, not in
//! Search.

use super::{Output, Ui, change};
use crate::models::Model;

impl Ui {
    /// The row Up/Down last focused, if it is still shown.
    pub fn focus_message(&self) -> Option<i64> {
        self.focus
    }

    /// The message Enter opens: the focused row, else the open message.
    pub fn open_focused(&self) -> Option<i64> {
        self.focus.or(self.open)
    }

    /// The local draft behind a shown row of the message list; `None` for
    /// a server message.
    pub fn draft_id_of(&self, id: i64) -> Option<String> {
        self.current.as_ref()?.shown.draft_of(id)
    }

    /// Move the focus `delta` rows (stopping at the ends); from no focus
    /// either direction lands on the first row. Returns the row updates.
    pub fn step_focus(&mut self, delta: i32) -> Vec<Output> {
        let Some(cur) = self.current.as_ref() else {
            return Vec::new();
        };
        let ids = cur.shown.ids();
        let Some(last) = ids.len().checked_sub(1) else {
            return Vec::new();
        };
        let at = self.focus.and_then(|f| ids.iter().position(|&i| i == f));
        let to = match at {
            None => 0,
            Some(a) => (a as i64 + i64::from(delta)).clamp(0, last as i64) as usize,
        };
        let outs = self.move_focus(Some(ids[to]));
        self.mirror.track(&outs);
        outs
    }

    pub(super) fn clear_focus(&mut self) -> Vec<Output> {
        self.move_focus(None)
    }

    /// The focused row left the list: the focus goes with it.
    pub(super) fn prune_focus(&mut self) -> Vec<Output> {
        let held = self
            .focus
            .is_none_or(|f| self.current.as_ref().is_some_and(|c| c.shown.contains(f)));
        if held {
            return Vec::new();
        }
        self.focus = None;
        Vec::new()
    }

    fn move_focus(&mut self, to: Option<i64>) -> Vec<Output> {
        let prev = std::mem::replace(&mut self.focus, to);
        let Some(cur) = self.current.as_mut() else {
            return Vec::new();
        };
        let mut outs = Vec::new();
        if let Some(p) = prev.filter(|&p| Some(p) != to) {
            outs.extend(
                cur.shown
                    .set_focus(p, false)
                    .map(|op| change(Model::Messages, op)),
            );
        }
        if let Some(t) = to {
            outs.extend(
                cur.shown
                    .set_focus(t, true)
                    .map(|op| change(Model::Messages, op)),
            );
        }
        outs
    }
}
