//! Acting on given messages, apart from the selection.

use envio_core::{Command, MessageRowId, UserAction};

use super::{Output, Ui};

impl Ui {
    /// Act on exactly these messages; the selection set is untouched.
    /// Ids no model shows are dropped; `None` when none are left. Acting
    /// on the open message clears the open id; the outputs are that row
    /// update.
    pub fn act_on(&mut self, ids: Vec<i64>, action: UserAction) -> Option<(Command, Vec<Output>)> {
        let ids: Vec<i64> = ids.into_iter().filter(|&i| self.shows(i)).collect();
        if ids.is_empty() {
            return None;
        }
        let mut outs = Vec::new();
        if self.open.is_some_and(|o| ids.contains(&o)) {
            outs = self.clear_open();
            self.mirror.track(&outs);
        }
        let ids = ids.into_iter().map(MessageRowId).collect();
        Some((Command::Act { ids, action }, outs))
    }
}
