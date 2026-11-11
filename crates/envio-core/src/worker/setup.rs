//! Adding an account: finding servers, checking them, saving.

use envio_net::autoconfig::{self, Found, Internet, Setting};
use envio_net::imap::ImapSession;
use envio_net::smtp::SmtpSession;
use envio_net::{Endpoint, Fingerprint, NetError, Untrusted};
use envio_store::{Account, AccountSettings};
use secrecy::SecretString;

use super::Worker;
use super::conn::Live;
use crate::api::command::{AccountSetup, ConnectRequest};
use crate::api::event::Event;
use crate::api::view::AccountInfo;
use crate::clock::now_ms;
use crate::health::AccountStatus;
use crate::mask;
use crate::problem::{self, Problem};
use crate::servers::{ServerAddr, Service, credentials, store_server};
use crate::text;

fn addr_of(setting: &Setting) -> ServerAddr {
    ServerAddr {
        host: setting.endpoint.host.clone(),
        port: setting.endpoint.port,
        security: setting.endpoint.security,
    }
}

/// What autoconfig found, as the wizard's form.
pub(crate) fn setup_from(found: &Found, email: &str) -> AccountSetup {
    let name = email.split('@').next().unwrap_or_default();
    AccountSetup {
        name: name.to_owned(),
        email: email.to_owned(),
        username: found.imap.username.clone(),
        imap: addr_of(&found.imap),
        smtp: addr_of(&found.smtp),
        source: Some(found.source),
        needs_confirm: found.confirm,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// Servers from DNS the user has not agreed to.
    Unconfirmed,
    BadFingerprint,
    Duplicate,
}

/// What must hold before any connection is made; returns the fingerprints
/// the user accepted.
pub(crate) fn check_request(
    request: &ConnectRequest,
    existing: &[Account],
) -> Result<Vec<(Service, Fingerprint)>, Refusal> {
    if request.setup.needs_confirm && !request.servers_confirmed {
        return Err(Refusal::Unconfirmed);
    }
    let accepted = request
        .accepted
        .iter()
        .map(|(service, hex)| {
            Fingerprint::from_hex(hex)
                .map(|fingerprint| (*service, fingerprint))
                .ok_or(Refusal::BadFingerprint)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let email = &request.setup.email;
    if existing
        .iter()
        .any(|a| a.settings.email.eq_ignore_ascii_case(email))
    {
        return Err(Refusal::Duplicate);
    }
    Ok(accepted)
}

fn refusal_text(refusal: &Refusal) -> String {
    match refusal {
        Refusal::Unconfirmed => text::setup::unconfirmed(),
        Refusal::BadFingerprint => text::setup::bad_fingerprint(),
        Refusal::Duplicate => text::setup::duplicate(),
    }
}

/// The accepted pin for `service`, if any.
fn pin_for(accepted: &[(Service, Fingerprint)], service: Service) -> Option<Fingerprint> {
    accepted
        .iter()
        .find(|(s, _)| *s == service)
        .map(|(_, fingerprint)| *fingerprint)
}

fn endpoint_of(addr: &ServerAddr, pin: Option<Fingerprint>) -> Endpoint {
    Endpoint {
        host: addr.host.clone(),
        port: addr.port,
        security: addr.security,
        pin,
    }
}

/// The event that tells the app why a server did not work out.
fn setup_event(setup: &AccountSetup, service: Service, error: &NetError) -> Event {
    let email = setup.email.clone();
    let host = match service {
        Service::Imap => &setup.imap.host,
        Service::Smtp => &setup.smtp.host,
    };
    let (presented, reason) = match error {
        NetError::Untrusted { presented, reason } => (*presented, *reason),
        // A pin the user accepted is no longer what the server shows.
        NetError::PinChanged { presented, .. } => (*presented, Untrusted::Other),
        _ => {
            let text = match problem::from_net(error, service) {
                Problem::SignIn => text::setup::sign_in_failed(),
                Problem::Certificate(_) => text::status::status(&AccountStatus::Certificate(None)),
                Problem::Refused(reason) => text::notice::refused(&reason),
                _ => text::setup::unreachable(),
            };
            return Event::SetupProblem { email, text };
        }
    };
    Event::TrustPrompt {
        email,
        service,
        host: host.clone(),
        fingerprint: presented.display(),
        hex: presented.to_hex(),
        text: text::setup::trust(host, reason),
    }
}

impl Worker<'_> {
    /// Looks the servers up (only the domain goes out) and tells the app.
    pub(crate) async fn detect(&mut self, email: String) {
        log::info!("detecting servers for {}", mask::address(&email));
        let internet = Internet::new(self.roots.clone());
        match autoconfig::detect(&email, &internet).await {
            Ok(found) => {
                let text = if found.confirm {
                    let (imap, smtp) = (&found.imap.endpoint.host, &found.smtp.endpoint.host);
                    text::setup::confirm_servers(imap, smtp)
                } else {
                    text::setup::found_by(found.source)
                };
                let setup = setup_from(&found, &email);
                self.emit(Event::Detected { email, setup, text });
            }
            Err(error) => {
                log::warn!("detect failed: {error}");
                self.emit(Event::SetupProblem {
                    email,
                    text: text::setup::bad_address(),
                });
            }
        }
    }

    /// Checks both servers with the password, then adds the account.
    pub(crate) async fn connect(&mut self, request: ConnectRequest) {
        let email = request.setup.email.clone();
        let accepted = match check_request(&request, self.store.accounts.list()) {
            Ok(accepted) => accepted,
            Err(refusal) => {
                let text = refusal_text(&refusal);
                self.emit(Event::SetupProblem { email, text });
                return;
            }
        };
        log::info!("checking a new account {}", mask::address(&email));
        if let Err(event) = self.try_servers(&request, &accepted).await {
            self.emit(*event);
            return;
        }
        // A pin is stored only if the user accepted it in this request.
        let setup = &request.setup;
        let settings = AccountSettings {
            name: setup.name.clone(),
            email: setup.email.clone(),
            username: setup.username.clone(),
            imap: store_server(&setup.imap, pin_for(&accepted, Service::Imap)),
            smtp: store_server(&setup.smtp, pin_for(&accepted, Service::Smtp)),
        };
        match self.save_account(settings, request.password) {
            Ok(account) => {
                let mut live = Live::new();
                live.wants.full = true;
                self.live.insert(account.key.clone(), live);
                self.emit(Event::AccountAdded { account });
            }
            Err(text) => self.emit(Event::SetupProblem { email, text }),
        }
    }

    /// Settings, then password; if the password cannot be written the
    /// account is taken back out.
    fn save_account(
        &mut self,
        settings: AccountSettings,
        password: SecretString,
    ) -> Result<AccountInfo, String> {
        let (name, email) = (settings.name.clone(), settings.email.clone());
        let key = self
            .store
            .accounts
            .add(settings, now_ms())
            .map_err(|error| {
                log::error!("saving the account: {error}");
                text::setup::not_saved()
            })?;
        if let Err(error) = self.store.secrets.set(&key, password) {
            log::error!("saving the password: {error}");
            if let Err(error) = self.store.accounts.remove(&key) {
                log::error!("taking the account back out: {error}");
            }
            return Err(text::setup::not_saved());
        }
        Ok(AccountInfo {
            key,
            name,
            email,
            status: AccountStatus::Ready,
        })
    }

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

    /// Opens IMAP, then SMTP, with the request's pins. The error is the
    /// event to send: the trust prompt or the problem.
    async fn try_servers(
        &self,
        request: &ConnectRequest,
        accepted: &[(Service, Fingerprint)],
    ) -> Result<(), Box<Event>> {
        let setup = &request.setup;
        let login = credentials(&setup.username, &request.password);
        let imap = endpoint_of(&setup.imap, pin_for(accepted, Service::Imap));
        let session = ImapSession::open(&imap, &self.roots, &login)
            .await
            .map_err(|e| Box::new(setup_event(setup, Service::Imap, &e)))?;
        // Logging out is a courtesy; the check has passed.
        let _ = session.logout().await;
        let on_smtp = |e: NetError| Box::new(setup_event(setup, Service::Smtp, &e));
        let smtp = endpoint_of(&setup.smtp, pin_for(accepted, Service::Smtp));
        let mut session = SmtpSession::connect(&smtp, &self.roots)
            .await
            .map_err(on_smtp)?;
        session.login(&login).await.map_err(on_smtp)?;
        let _ = session.quit().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use envio_net::autoconfig::{Setting, Source};
    use envio_net::{Endpoint, Security};
    use envio_store::{AccountSettings, Server};
    use secrecy::SecretString;

    fn found(source: Source, confirm: bool) -> Found {
        let setting = |host: &str, port| Setting {
            endpoint: Endpoint::new(host, port, Security::Tls),
            username: "ada@example.org".into(),
        };
        Found {
            imap: setting("imap.example.org", 993),
            smtp: setting("smtp.example.org", 465),
            source,
            confirm,
        }
    }

    fn request(setup: AccountSetup, confirmed: bool, accepted: &[&str]) -> ConnectRequest {
        ConnectRequest {
            setup,
            password: SecretString::from("pw".to_owned()),
            servers_confirmed: confirmed,
            accepted: accepted
                .iter()
                .map(|hex| (Service::Imap, (*hex).to_owned()))
                .collect(),
        }
    }

    fn existing(email: &str) -> Account {
        let server = || Server {
            host: "h".into(),
            port: 1,
            security: envio_store::Security::Tls,
            pin: None,
        };
        Account {
            key: "a1".into(),
            settings: AccountSettings {
                name: "N".into(),
                email: email.into(),
                username: email.into(),
                imap: server(),
                smtp: server(),
            },
        }
    }

    #[test]
    fn srv_results_need_confirmation() {
        let setup = setup_from(&found(Source::Srv, true), "ada@example.org");
        assert!(setup.needs_confirm);
        assert_eq!(setup.source, Some(Source::Srv));
        let refused = check_request(&request(setup.clone(), false, &[]), &[]);
        assert_eq!(refused, Err(Refusal::Unconfirmed));
        assert_eq!(check_request(&request(setup, true, &[]), &[]), Ok(vec![]));
    }

    #[test]
    fn an_ispdb_result_needs_none() {
        let setup = setup_from(&found(Source::Ispdb, false), "ada@example.org");
        assert!(!setup.needs_confirm);
        assert_eq!(setup.email, "ada@example.org");
        assert_eq!(setup.username, "ada@example.org");
        assert_eq!(setup.imap.host, "imap.example.org");
        assert_eq!(setup.smtp.port, 465);
        assert_eq!(check_request(&request(setup, false, &[]), &[]), Ok(vec![]));
    }

    #[test]
    fn a_bad_fingerprint_is_refused() {
        let setup = setup_from(&found(Source::Ispdb, false), "ada@example.org");
        for bad in ["", "abc", &"g".repeat(64), &"a".repeat(63)] {
            let refused = check_request(&request(setup.clone(), false, &[bad]), &[]);
            assert_eq!(refused, Err(Refusal::BadFingerprint), "{bad}");
        }
        let good = "ab".repeat(32);
        let accepted = check_request(&request(setup, false, &[&good]), &[]).unwrap();
        assert_eq!(accepted.len(), 1);
        assert_eq!(accepted[0].0, Service::Imap);
        assert_eq!(accepted[0].1.to_hex(), good);
    }

    #[test]
    fn a_second_account_with_the_same_email_is_refused() {
        let setup = setup_from(&found(Source::Ispdb, false), "Ada@Example.org");
        let refused = check_request(&request(setup, false, &[]), &[existing("ada@example.org")]);
        assert_eq!(refused, Err(Refusal::Duplicate));
    }
}
