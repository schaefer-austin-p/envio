//! Which providers want an app password, and where the user makes one
//! (#221). Pure: the wizard asks it for an address's domain and the MX
//! verdict (`Hosting`), and for a provider id of the provider step.

use envio_net::autoconfig::Hosting;

/// A provider that takes an app password, not the account password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppPasswordProvider {
    Google,
    Icloud,
    Yahoo,
    Aol,
}

const GOOGLE: &[&str] = &["gmail.com", "googlemail.com"];
const ICLOUD: &[&str] = &["icloud.com", "me.com", "mac.com"];
/// An explicit list: Yahoo Japan (yahoo.co.jp) is a separate service.
/// sbcglobal.net and att.net are left out on purpose: AT&T mail uses its
/// own "secure mail key", not a Yahoo app password.
const YAHOO: &[&str] = &[
    "yahoo.com",
    "ymail.com",
    "rocketmail.com",
    "yahoo.co.uk",
    "yahoo.fr",
    "yahoo.de",
    "yahoo.it",
    "yahoo.es",
    "yahoo.ca",
    "yahoo.ie",
    "yahoo.in",
    "yahoo.co.in",
    "yahoo.com.au",
    "yahoo.com.br",
    "yahoo.com.mx",
    "yahoo.com.ar",
    "yahoo.com.sg",
    "yahoo.co.nz",
    "yahoo.co.id",
];
const AOL: &[&str] = &["aol.com"];

impl AppPasswordProvider {
    /// The id the provider step uses ("gmail", "icloud", "yahoo", "aol").
    pub fn id(self) -> &'static str {
        match self {
            Self::Google => "gmail",
            Self::Icloud => "icloud",
            Self::Yahoo => "yahoo",
            Self::Aol => "aol",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        [Self::Google, Self::Icloud, Self::Yahoo, Self::Aol]
            .into_iter()
            .find(|p| p.id() == id)
    }

    /// The brand name, as the hint says it. Not translated.
    pub fn name(self) -> &'static str {
        match self {
            Self::Google => "Google",
            Self::Icloud => "iCloud",
            Self::Yahoo => "Yahoo",
            Self::Aol => "AOL",
        }
    }

    /// Where the provider's app-password page is.
    pub fn help_url(self) -> &'static str {
        match self {
            Self::Google => "https://myaccount.google.com/apppasswords",
            Self::Icloud => "https://account.apple.com/account/manage",
            Self::Yahoo => "https://login.yahoo.com/account/security",
            Self::Aol => "https://login.aol.com/account/security",
        }
    }

    fn domains(self) -> &'static [&'static str] {
        match self {
            Self::Google => GOOGLE,
            Self::Icloud => ICLOUD,
            Self::Yahoo => YAHOO,
            Self::Aol => AOL,
        }
    }
}

/// The provider of `domain`, or of the MX records' verdict (a Google
/// Workspace domain). The domain list wins; `None` for anything else.
pub fn app_password_provider(domain: &str, hosting: Hosting) -> Option<AppPasswordProvider> {
    let domain = domain.trim().trim_end_matches('.').to_ascii_lowercase();
    [
        AppPasswordProvider::Google,
        AppPasswordProvider::Icloud,
        AppPasswordProvider::Yahoo,
        AppPasswordProvider::Aol,
    ]
    .into_iter()
    .find(|p| p.domains().contains(&domain.as_str()))
    .or((hosting == Hosting::Google).then_some(AppPasswordProvider::Google))
}

/// The help page of the provider step's id; empty for any other id.
pub fn app_password_help_url(id: &str) -> String {
    AppPasswordProvider::from_id(id).map_or_else(String::new, |p| p.help_url().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use AppPasswordProvider::*;

    fn of(domain: &str) -> Option<AppPasswordProvider> {
        app_password_provider(domain, Hosting::Unknown)
    }

    #[test]
    fn each_listed_domain_has_its_provider() {
        assert_eq!(of("gmail.com"), Some(Google));
        assert_eq!(of("googlemail.com"), Some(Google));
        for d in ["icloud.com", "me.com", "mac.com"] {
            assert_eq!(of(d), Some(Icloud), "{d}");
        }
        for d in ["yahoo.com", "ymail.com", "rocketmail.com", "yahoo.co.uk"] {
            assert_eq!(of(d), Some(Yahoo), "{d}");
        }
        assert_eq!(of("aol.com"), Some(Aol));
    }

    #[test]
    fn case_and_the_trailing_dot_do_not_matter() {
        assert_eq!(of("GMail.COM."), Some(Google));
        assert_eq!(of(" iCloud.com "), Some(Icloud));
    }

    #[test]
    fn unknown_and_lookalike_domains_have_none() {
        for d in [
            "example.org",
            "",
            "gmail.com.evil.test",
            "notgmail.com",
            "mail.gmail.com",
            "yahoo.co.jp",
            "yahoo.example",
            "att.net",
            "sbcglobal.net",
        ] {
            assert_eq!(of(d), None, "{d}");
        }
    }

    #[test]
    fn a_google_workspace_domain_is_google_by_its_mx() {
        assert_eq!(
            app_password_provider("studio.example", Hosting::Google),
            Some(Google)
        );
        assert_eq!(
            app_password_provider("studio.example", Hosting::Unknown),
            None
        );
        assert_eq!(
            app_password_provider("studio.example", Hosting::Microsoft365),
            None
        );
    }

    #[test]
    fn the_domain_list_wins_over_the_mx() {
        assert_eq!(
            app_password_provider("icloud.com", Hosting::Google),
            Some(Icloud)
        );
    }

    #[test]
    fn ids_round_trip_and_only_known_ids_have_a_page() {
        for p in [Google, Icloud, Yahoo, Aol] {
            assert_eq!(AppPasswordProvider::from_id(p.id()), Some(p));
            assert!(p.help_url().starts_with("https://"));
        }
        assert_eq!(AppPasswordProvider::from_id("other"), None);
        assert_eq!(
            app_password_help_url("gmail"),
            "https://myaccount.google.com/apppasswords"
        );
        assert_eq!(app_password_help_url("other"), "");
    }
}
