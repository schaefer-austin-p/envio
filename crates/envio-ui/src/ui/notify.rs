//! Settings → Notifications: which accounts raise alerts, as the switches
//! of the account rows, and the settings a switch writes.

use std::collections::BTreeSet;

use super::{Output, Ui, change};
use crate::models::{Model, Op};
use crate::rows::Row;

impl Ui {
    /// The settings arrived: rows whose switch changed update in place.
    pub(super) fn notify_off_is(&mut self, off: &BTreeSet<String>) -> Vec<Output> {
        self.notify_off.clone_from(off);
        let mut outs = Vec::new();
        for (at, row) in self.accounts.iter_mut().enumerate() {
            let notify = !off.contains(&row.key);
            if row.notify != notify {
                row.notify = notify;
                let row = Row::Account(row.clone());
                outs.push(change(Model::Accounts, Op::Update { at, row }));
            }
        }
        outs
    }
}
