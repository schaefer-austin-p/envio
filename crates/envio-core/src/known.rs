//! The mail providers that need an app password (#221): their servers are
//! known, so nothing is looked up and the typed address goes straight to
//! the sign-in. Pure.

use envio_net::autoconfig::{Found, Hosting, Setting, Source};
use envio_net::{Endpoint, Security};

use crate::api::command::AccountSetup;
use crate::app_password::AppPasswordProvider;
use crate::servers::ServerAddr;

type Host = (&'static str, u16, Security);

/// The (IMAP, SMTP) servers of a provider that has known ones; `None` for
/// AOL, which the provider step does not offer.
fn servers_of(provider: AppPasswordProvider) -> Option<(Host, Host)> {
    match provider {
        AppPasswordProvider::Icloud => Some((
            ("imap.mail.me.com", 993, Security::Tls),
            ("smtp.mail.me.com", 587, Security::StartTls),
        )),
        AppPasswordProvider::Google => Some((
            ("imap.gmail.com", 993, Security::Tls),
            ("smtp.gmail.com", 465, Security::Tls),
        )),
        AppPasswordProvider::Yahoo => Some((
            ("imap.mail.yahoo.com", 993, Security::Tls),
            ("smtp.mail.yahoo.com", 465, Security::Tls),
        )),
        AppPasswordProvider::Aol => None,
    }
}

/// The servers of `provider` ("icloud", "gmail" or "yahoo") for `email`;
/// `None` for any other provider id. The name is the part before the `@`.
pub fn app_password_setup(provider: &str, email: &str) -> Option<AccountSetup> {
    let (imap, smtp) = servers_of(AppPasswordProvider::from_id(provider)?)?;
    let addr = |(host, port, security): Host| ServerAddr {
        host: host.to_owned(),
        port,
        security,
    };
    let (imap, smtp) = (addr(imap), addr(smtp));
    let email = email.trim().to_owned();
    let name = email.rsplit_once('@').map_or("", |(local, _)| local);
    Some(AccountSetup {
        name: name.to_owned(),
        username: email.clone(),
        email,
        imap,
        smtp,
        source: Some(Source::Provider),
        needs_confirm: false,
    })
}

/// Gmail's servers for a Google Workspace domain found by its MX records:
/// autoconfig found nothing there, and `imap.<domain>` would not answer.
pub fn google_found(email: &str) -> Found {
    let ((imap, ip, is), (smtp, sp, ss)) =
        servers_of(AppPasswordProvider::Google).expect("Google has servers");
    let setting = |host, port, security| Setting {
        endpoint: Endpoint::new(host, port, security),
        username: email.trim().to_owned(),
    };
    Found {
        imap: setting(imap, ip, is),
        smtp: setting(smtp, sp, ss),
        source: Source::Provider,
        // No confirm step: the hosts are fixed Google hosts, so a spoofed MX
        // record can say "Google" but cannot send the password anywhere else.
        confirm: false,
    }
}

/// What a Google Workspace MX verdict means for a detect result: a guess
/// (autoconfig found nothing, `imap.<domain>` would not answer) becomes
/// Gmail's servers. The returned verdict is `Google` only while the servers
/// are Google's, so a hint never sits next to another provider's servers.
pub fn settle_google(found: Found, hosting: Hosting, email: &str) -> (Found, Hosting) {
    if hosting != Hosting::Google {
        return (found, hosting);
    }
    let found = if found.source == Source::Guess {
        google_found(email)
    } else {
        found
    };
    let google = found.imap.endpoint.host == "imap.gmail.com"
        && found.smtp.endpoint.host == "smtp.gmail.com";
    (
        found,
        if google {
            Hosting::Google
        } else {
            Hosting::Unknown
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_app_password_provider_has_its_known_servers() {
        let hosts = |p: &str| {
            let s = app_password_setup(p, " ann@example.org ").unwrap();
            (
                s.imap.host,
                s.imap.port,
                s.smtp.host,
                s.smtp.port,
                s.smtp.security,
            )
        };
        assert_eq!(
            hosts("icloud"),
            (
                "imap.mail.me.com".into(),
                993,
                "smtp.mail.me.com".into(),
                587,
                Security::StartTls
            )
        );
        assert_eq!(
            hosts("gmail"),
            (
                "imap.gmail.com".into(),
                993,
                "smtp.gmail.com".into(),
                465,
                Security::Tls
            )
        );
        assert_eq!(
            hosts("yahoo"),
            (
                "imap.mail.yahoo.com".into(),
                993,
                "smtp.mail.yahoo.com".into(),
                465,
                Security::Tls
            )
        );
    }

    #[test]
    fn the_setup_names_the_account_after_the_address_and_asks_no_confirmation() {
        let s = app_password_setup("icloud", " Ann@example.org ").unwrap();
        assert_eq!(
            (s.name.as_str(), s.email.as_str(), s.username.as_str()),
            ("Ann", "Ann@example.org", "Ann@example.org")
        );
        assert_eq!(s.source, Some(Source::Provider));
        assert!(!s.needs_confirm);
    }

    #[test]
    fn a_workspace_domain_gets_the_gmail_servers_as_a_provider_find() {
        let f = google_found(" ann@studio.example ");
        assert_eq!(
            (f.imap.endpoint.host.as_str(), f.imap.endpoint.port),
            ("imap.gmail.com", 993)
        );
        assert_eq!(
            (f.smtp.endpoint.host.as_str(), f.smtp.endpoint.port),
            ("smtp.gmail.com", 465)
        );
        assert_eq!(f.imap.username, "ann@studio.example");
        assert_eq!(f.source, Source::Provider);
        assert!(!f.confirm);
    }

    fn guess() -> Found {
        let mut f = google_found("ann@studio.example");
        f.source = Source::Guess;
        f.imap.endpoint = Endpoint::new("imap.studio.example", 993, Security::Tls);
        f
    }

    #[test]
    fn a_google_mx_turns_a_guess_into_gmails_servers() {
        let (f, h) = settle_google(guess(), Hosting::Google, "ann@studio.example");
        assert_eq!(f.imap.endpoint.host, "imap.gmail.com");
        assert_eq!(h, Hosting::Google);
    }

    #[test]
    fn a_google_mx_beside_other_servers_is_no_google_verdict() {
        let mut other = guess();
        other.source = Source::Srv;
        let (f, h) = settle_google(other, Hosting::Google, "ann@studio.example");
        assert_eq!(f.imap.endpoint.host, "imap.studio.example");
        assert_eq!(h, Hosting::Unknown);
    }

    #[test]
    fn other_verdicts_pass_through() {
        let (f, h) = settle_google(guess(), Hosting::Unknown, "ann@studio.example");
        assert_eq!((f.source, h), (Source::Guess, Hosting::Unknown));
    }

    #[test]
    fn any_other_provider_id_has_none() {
        assert_eq!(app_password_setup("aol", "ann@example.org"), None);
        assert_eq!(app_password_setup("other", "ann@example.org"), None);
        assert_eq!(app_password_setup("", "ann@example.org"), None);
    }
}
