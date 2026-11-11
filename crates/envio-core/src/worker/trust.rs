//! Fixing a paused account: a new password, or trusting its certificate.

use envio_net::Fingerprint;
use secrecy::SecretString;

use super::Worker;
use crate::api::event::Event;
use crate::health::AccountStatus;
use crate::servers::Service;
use crate::text;

impl Worker<'_> {
    /// A new password for a paused account: save it, run again.
    pub(crate) fn update_password(&mut self, account: &str, password: SecretString) {
        if !self.live.contains_key(account) {
            log::warn!("password for unknown account {account}");
            return;
        }
        if let Err(error) = self.store.secrets.set(account, password) {
            log::error!("saving the password: {error}");
            self.emit(Event::Notice {
                text: text::setup::not_saved(),
            });
            return;
        }
        self.resume(account);
    }

    /// Pins the certificate the account's server presented, if it is the
    /// one the user was shown.
    pub(crate) fn accept_certificate(&mut self, account: &str, service: Service, hex: &str) {
        let Some(pin) = self.presented_certificate(account, service, hex) else {
            self.emit(Event::Notice {
                text: text::setup::nothing_to_accept(),
            });
            return;
        };
        let Some(current) = self.store.accounts.get(account) else {
            return;
        };
        let mut settings = current.settings.clone();
        match service {
            Service::Imap => settings.imap.pin = Some(pin.to_hex()),
            Service::Smtp => settings.smtp.pin = Some(pin.to_hex()),
        }
        if let Err(error) = self.store.accounts.update(account, settings) {
            log::error!("saving the pin: {error}");
            self.emit(Event::Notice {
                text: text::setup::not_saved(),
            });
            return;
        }
        self.resume(account);
    }

    /// The certificate `hex` names, if the account is paused on exactly it.
    fn presented_certificate(
        &self,
        account: &str,
        service: Service,
        hex: &str,
    ) -> Option<Fingerprint> {
        match &self.live.get(account)?.health.status {
            AccountStatus::Certificate(Some(details))
                if details.service == service && details.presented.to_hex() == hex =>
            {
                Some(details.presented)
            }
            _ => None,
        }
    }

    /// The user fixed what paused the account: sync again now.
    fn resume(&mut self, account: &str) {
        let before = self.status_of(account);
        if let Some(live) = self.live.get_mut(account) {
            live.health.resume();
            live.wants.full = true;
        }
        self.report(account, &before);
    }
}
