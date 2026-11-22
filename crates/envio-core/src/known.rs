//! The mail providers that need an app password (#221): their servers are
//! known, so nothing is looked up and the typed address goes straight to
//! the sign-in. Pure.

use envio_net::Security;
use envio_net::autoconfig::Source;

use crate::api::command::AccountSetup;
use crate::servers::ServerAddr;

/// The servers of `provider` ("icloud", "gmail" or "yahoo") for `email`;
/// `None` for any other provider id. The name is the part before the `@`.
pub fn app_password_setup(provider: &str, email: &str) -> Option<AccountSetup> {
    let addr = |host: &str, port, security| ServerAddr {
        host: host.to_owned(),
        port,
        security,
    };
    let (imap, smtp) = match provider {
        "icloud" => (
            addr("imap.mail.me.com", 993, Security::Tls),
            addr("smtp.mail.me.com", 587, Security::StartTls),
        ),
        "gmail" => (
            addr("imap.gmail.com", 993, Security::Tls),
            addr("smtp.gmail.com", 465, Security::Tls),
        ),
        "yahoo" => (
            addr("imap.mail.yahoo.com", 993, Security::Tls),
            addr("smtp.mail.yahoo.com", 465, Security::Tls),
        ),
        _ => return None,
    };
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
    fn any_other_provider_id_has_none() {
        assert_eq!(app_password_setup("other", "ann@example.org"), None);
        assert_eq!(app_password_setup("", "ann@example.org"), None);
    }
}
