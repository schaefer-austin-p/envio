//! What autoconfig asks, in which order, and which answer wins. Pure: the
//! lookups themselves are in `http` and `srv`.

use crate::server::{Endpoint, Security};

/// Where a server setting came from (the UI says "found by ...").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Thunderbird's ISPDB.
    Ispdb,
    /// autoconfig.<domain>, run by the provider.
    Provider,
    /// <domain>/.well-known/autoconfig.
    WellKnown,
    /// DNS SRV records (RFC 6186, RFC 8314).
    Srv,
    /// imap.<domain> and smtp.<domain>; the user should check them.
    Guess,
}

/// One server and the name to sign in with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setting {
    pub endpoint: Endpoint,
    pub username: String,
}

/// Every usable server one source offered, in its order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Candidates {
    pub imap: Vec<Setting>,
    pub smtp: Vec<Setting>,
}

/// The settings autoconfig settled on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub imap: Setting,
    pub smtp: Setting,
    pub source: Source,
}

/// One SRV answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrvRecord {
    pub priority: u16,
    pub weight: u16,
    pub port: u16,
    pub target: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    Imap,
    Smtp,
}

/// The lowercased domain of an address; None if it isn't one.
pub fn domain_of(email: &str) -> Option<String> {
    let (local, domain) = email.trim().rsplit_once('@')?;
    let domain = domain.to_ascii_lowercase();
    (!local.is_empty() && domain.contains('.') && is_hostname(&domain)).then_some(domain)
}

/// Letters, digits, '-' and '.', not starting or ending with a dot.
pub fn is_hostname(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && !host.starts_with('.')
        && !host.ends_with('.')
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
}

/// The config files to try, in order. https only: a plain-http answer
/// could come from anyone on the path.
pub fn config_urls(domain: &str) -> [(Source, String); 3] {
    [
        (
            Source::Ispdb,
            format!("https://autoconfig.thunderbird.net/v1.1/{domain}"),
        ),
        (
            Source::Provider,
            format!("https://autoconfig.{domain}/mail/config-v1.1.xml"),
        ),
        (
            Source::WellKnown,
            format!("https://{domain}/.well-known/autoconfig/mail/config-v1.1.xml"),
        ),
    ]
}

/// The SRV names to ask, implicit TLS first (RFC 8314).
pub fn srv_queries(domain: &str) -> [(String, Service, Security); 4] {
    [
        (
            format!("_imaps._tcp.{domain}"),
            Service::Imap,
            Security::Tls,
        ),
        (
            format!("_imap._tcp.{domain}"),
            Service::Imap,
            Security::StartTls,
        ),
        (
            format!("_submissions._tcp.{domain}"),
            Service::Smtp,
            Security::Tls,
        ),
        (
            format!("_submission._tcp.{domain}"),
            Service::Smtp,
            Security::StartTls,
        ),
    ]
}

/// SRV answers as settings: target "." means "not offered here" and is
/// dropped; lowest priority first, then highest weight. The username is
/// the address (RFC 6186 3.2).
pub fn srv_candidates(
    mut records: Vec<SrvRecord>,
    security: Security,
    email: &str,
) -> Vec<Setting> {
    records.sort_by_key(|r| (r.priority, std::cmp::Reverse(r.weight)));
    records
        .into_iter()
        .filter_map(|r| {
            let host = r.target.trim_end_matches('.').to_ascii_lowercase();
            (r.port != 0 && is_hostname(&host)).then(|| Setting {
                endpoint: Endpoint::new(&host, r.port, security),
                username: email.trim().to_owned(),
            })
        })
        .collect()
}

/// The first implicit-TLS server of each kind, else the first STARTTLS
/// one. None unless there is both an IMAP and an SMTP server.
pub fn best(candidates: &Candidates, source: Source) -> Option<Found> {
    fn pick(list: &[Setting]) -> Option<&Setting> {
        list.iter()
            .find(|s| s.endpoint.security == Security::Tls)
            .or_else(|| list.first())
    }
    Some(Found {
        imap: pick(&candidates.imap)?.clone(),
        smtp: pick(&candidates.smtp)?.clone(),
        source,
    })
}

/// The last resort, for the manual form to start from.
pub fn guess(domain: &str, email: &str) -> Found {
    let setting = |host: String, port| Setting {
        endpoint: Endpoint::new(&host, port, Security::Tls),
        username: email.trim().to_owned(),
    };
    Found {
        imap: setting(format!("imap.{domain}"), 993),
        smtp: setting(format!("smtp.{domain}"), 465),
        source: Source::Guess,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setting(host: &str, port: u16, security: Security) -> Setting {
        Setting {
            endpoint: Endpoint::new(host, port, security),
            username: "me@example.org".into(),
        }
    }

    #[test]
    fn only_real_addresses_have_a_domain() {
        assert_eq!(
            domain_of(" Me@Example.ORG ").as_deref(),
            Some("example.org")
        );
        assert_eq!(domain_of("a@b@example.org").as_deref(), Some("example.org"));
        for bad in [
            "example.org",
            "@example.org",
            "me@localhost",
            "me@exa mple.org",
            "me@x/y.org",
            "me@.org.",
        ] {
            assert_eq!(domain_of(bad), None, "{bad}");
        }
    }

    #[test]
    fn config_files_are_tried_ispdb_first_and_over_https() {
        let urls = config_urls("example.org");
        assert_eq!(
            urls[0],
            (
                Source::Ispdb,
                "https://autoconfig.thunderbird.net/v1.1/example.org".into()
            )
        );
        assert_eq!(
            urls[1].1,
            "https://autoconfig.example.org/mail/config-v1.1.xml"
        );
        assert_eq!(
            urls[2].1,
            "https://example.org/.well-known/autoconfig/mail/config-v1.1.xml"
        );
        assert_eq!(srv_queries("example.org")[0].0, "_imaps._tcp.example.org");
    }

    #[test]
    fn srv_answers_sort_and_drop_the_not_offered_marker() {
        let records = vec![
            SrvRecord {
                priority: 10,
                weight: 0,
                port: 993,
                target: "backup.example.org.".into(),
            },
            SrvRecord {
                priority: 0,
                weight: 5,
                port: 993,
                target: "Mail.Example.org.".into(),
            },
            SrvRecord {
                priority: 0,
                weight: 50,
                port: 993,
                target: "heavy.example.org.".into(),
            },
            SrvRecord {
                priority: 0,
                weight: 0,
                port: 0,
                target: ".".into(),
            },
        ];
        let hosts: Vec<_> = srv_candidates(records, Security::Tls, "me@example.org")
            .into_iter()
            .map(|s| s.endpoint.host)
            .collect();
        assert_eq!(
            hosts,
            [
                "heavy.example.org",
                "mail.example.org",
                "backup.example.org"
            ]
        );
    }

    #[test]
    fn implicit_tls_wins_and_both_servers_are_needed() {
        let candidates = Candidates {
            imap: vec![
                setting("a", 143, Security::StartTls),
                setting("b", 993, Security::Tls),
            ],
            smtp: vec![setting("c", 587, Security::StartTls)],
        };
        let found = best(&candidates, Source::Provider).unwrap();
        assert_eq!(found.imap.endpoint.host, "b");
        assert_eq!(found.smtp.endpoint.host, "c");
        let no_smtp = Candidates {
            smtp: vec![],
            ..candidates
        };
        assert_eq!(best(&no_smtp, Source::Provider), None);
    }

    #[test]
    fn the_guess_uses_the_usual_names_and_implicit_tls() {
        let found = guess("example.org", "me@example.org");
        assert_eq!(
            found.imap.endpoint,
            Endpoint::new("imap.example.org", 993, Security::Tls)
        );
        assert_eq!(
            found.smtp.endpoint,
            Endpoint::new("smtp.example.org", 465, Security::Tls)
        );
        assert_eq!(found.source, Source::Guess);
    }
}
