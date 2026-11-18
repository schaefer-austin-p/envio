//! Naming an account for the screens.

use super::Worker;
use crate::api::event::Event;
use crate::text;

impl Worker<'_> {
    pub(crate) fn set_account_label(&mut self, account: &str, label: &str) {
        let Some(entry) = self.store.accounts.get(account) else {
            log::warn!("label for unknown account {account}");
            return;
        };
        if let Some(reason) = entry.broken.clone() {
            self.emit(Event::Notice {
                text: text::status::broken(&reason),
            });
            return;
        }
        if let Err(error) = self.store.accounts.set_label(account, Some(label)) {
            log::error!("saving the label: {error}");
            self.emit(Event::Notice {
                text: text::setup::not_saved(),
            });
            return;
        }
        let saved = self
            .store
            .accounts
            .get(account)
            .and_then(|a| a.label.clone())
            .unwrap_or_default();
        self.emit(Event::AccountLabel {
            account: account.to_owned(),
            label: saved,
        });
    }
}
