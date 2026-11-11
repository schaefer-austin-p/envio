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
        // A broken entry takes no password: say why, write nothing.
        if let AccountStatus::Broken(reason) = self.status_of(account) {
            self.emit(Event::Notice {
                text: text::status::broken(&reason),
            });
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
        // A broken entry is cleared only by repairing it or removing it.
        if matches!(before, AccountStatus::Broken(_)) {
            return;
        }
        if let Some(live) = self.live.get_mut(account) {
            live.health.resume();
            live.wants.full = true;
        }
        self.report(account, &before);
    }
}

/// `status` for the app: a certificate problem names the server it came
/// from (the status inside the worker does not know it).
pub(super) fn with_host(
    status: AccountStatus,
    settings: &envio_store::AccountSettings,
) -> AccountStatus {
    match status {
        AccountStatus::Certificate(Some(mut details)) => {
            details.host = match details.service {
                Service::Imap => settings.imap.host.clone(),
                Service::Smtp => settings.smtp.host.clone(),
            };
            AccountStatus::Certificate(Some(details))
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::problem::CertDetails;
    use envio_store::{AccountSettings, Security, Server};

    fn server(host: &str) -> Server {
        Server {
            host: host.into(),
            port: 993,
            security: Security::Tls,
            pin: None,
        }
    }

    #[test]
    fn a_certificate_status_names_the_server_of_its_service() {
        let settings = AccountSettings {
            imap: server("imap.example.org"),
            smtp: server("smtp.example.org"),
            name: "Ann".into(),
            email: "ann@example.org".into(),
            username: "ann".into(),
        };
        let details = |service| CertDetails {
            service,
            pinned: None,
            presented: Fingerprint::from_bytes([1; 32]),
            reason: None,
            host: String::new(),
        };
        let host_of = |service| match with_host(
            AccountStatus::Certificate(Some(details(service))),
            &settings,
        ) {
            AccountStatus::Certificate(Some(d)) => d.host,
            other => panic!("{other:?}"),
        };
        assert_eq!(host_of(Service::Imap), "imap.example.org");
        assert_eq!(host_of(Service::Smtp), "smtp.example.org");
        assert_eq!(
            with_host(AccountStatus::Ready, &settings),
            AccountStatus::Ready
        );
    }
}
