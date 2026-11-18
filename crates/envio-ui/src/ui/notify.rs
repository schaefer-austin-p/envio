//! Settings → Notifications: which accounts raise alerts, as the switches
//! of the account rows, and the settings a switch writes.

use std::collections::BTreeSet;

use envio_core::Settings;

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

/// The settings with one account's notification switch set.
pub fn settings_with_notify(current: &Settings, account: &str, on: bool) -> Settings {
    let mut settings = current.clone();
    if on {
        settings.notify_off.remove(account);
    } else {
        settings.notify_off.insert(account.to_owned());
    }
    settings
}

/// The settings with "Show sender and subject" set.
pub fn settings_with_show_text(current: &Settings, on: bool) -> Settings {
    Settings {
        notify_show_text: on,
        ..current.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggling_an_account_writes_its_setting() {
        let off = settings_with_notify(&Settings::default(), "a", false);
        assert_eq!(off.notify_off, BTreeSet::from(["a".to_owned()]));
        let other = settings_with_notify(&off, "b", false);
        assert_eq!(other.notify_off.len(), 2);
        let on = settings_with_notify(&other, "a", true);
        assert_eq!(on.notify_off, BTreeSet::from(["b".to_owned()]));
    }

    #[test]
    fn the_text_switch_defaults_off() {
        let settings = Settings::default();
        assert!(!settings.notify_show_text);
        assert!(settings_with_show_text(&settings, true).notify_show_text);
        let back = settings_with_show_text(&settings_with_show_text(&settings, true), false);
        assert!(!back.notify_show_text);
    }
}
