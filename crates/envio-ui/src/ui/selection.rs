//! What the user has marked, across the message and search models.

use std::collections::BTreeSet;

use envio_core::{Command, MessageRowId, UserAction};

use super::{Output, Ui, change};
use crate::models::Model;

impl Ui {
    pub fn select(&mut self, id: i64, on: bool) -> Vec<Output> {
        let cur = self.current.as_mut().map(|c| &mut c.shown);
        let ops = [
            (Model::Messages, cur.and_then(|s| s.set_selected(id, on))),
            (Model::Search, self.found.set_selected(id, on)),
        ];
        let shown = ops.iter().any(|(_, op)| op.is_some()) || self.shows(id);
        if on && shown {
            self.selection.insert(id);
        } else {
            self.selection.remove(&id);
        }
        let outs: Vec<Output> = ops
            .into_iter()
            .filter_map(|(m, op)| op.map(|op| change(m, op)))
            .collect();
        self.mirror.track(&outs);
        outs
    }

    pub fn clear_selection(&mut self) -> Vec<Output> {
        let outs = self.clear_marks();
        self.mirror.track(&outs);
        outs
    }

    /// `clear_selection` for callers that record the changes themselves.
    pub(super) fn clear_marks(&mut self) -> Vec<Output> {
        let ids: Vec<i64> = std::mem::take(&mut self.selection).into_iter().collect();
        let mut outs = Vec::new();
        for id in ids {
            if let Some(op) = self
                .current
                .as_mut()
                .and_then(|c| c.shown.set_selected(id, false))
            {
                outs.push(change(Model::Messages, op));
            }
            if let Some(op) = self.found.set_selected(id, false) {
                outs.push(change(Model::Search, op));
            }
        }
        outs
    }

    /// `Act` on the selected messages. The selection clears at once; the
    /// outputs are the row updates that show it.
    pub fn act_on_selection(&mut self, action: UserAction) -> Option<(Command, Vec<Output>)> {
        if self.selection.is_empty() {
            return None;
        }
        let ids = self.selection.iter().map(|&id| MessageRowId(id)).collect();
        let outs = self.clear_marks();
        self.mirror.track(&outs);
        Some((Command::Act { ids, action }, outs))
    }

    fn shows(&self, id: i64) -> bool {
        self.found.contains(id) || self.current.as_ref().is_some_and(|c| c.shown.contains(id))
    }

    /// Rows that left their model leave the selection.
    pub(super) fn prune_selection(&mut self) {
        let keep: BTreeSet<i64> = self
            .selection
            .iter()
            .copied()
            .filter(|&i| self.shows(i))
            .collect();
        self.selection = keep;
    }
}
