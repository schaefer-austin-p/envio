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
    /// At least one host is outside the email domain, whatever the source
    /// (DNS is unauthenticated, other files may point anywhere); the setup
    /// screen must show the hosts and ask before sending the password
    /// (RFC 6186 §6).
    pub confirm: bool,
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

/// Letters, digits and '-' in labels separated by single dots. A label
/// may not be empty or start or end with '-'.
pub fn is_hostname(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// Whether `host` is `domain` or under it. Case-insensitive, one trailing
/// dot ignored, and label-aware: `mail.example.org` is in `example.org`,
/// `badexample.org` is not.
fn within(host: &str, domain: &str) -> bool {
    let host = host.strip_suffix('.').unwrap_or(host).to_ascii_lowercase();
    let domain = domain
        .strip_suffix('.')
        .unwrap_or(domain)
        .to_ascii_lowercase();
    host == domain || host.ends_with(&format!(".{domain}"))
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
/// one. None unless there is both an IMAP and an SMTP server. `confirm`
/// is set when either host is outside `domain`, whatever the source: the
/// password would go there.
pub fn best(candidates: &Candidates, source: Source, domain: &str) -> Option<Found> {
    fn pick(list: &[Setting]) -> Option<&Setting> {
        list.iter()
            .find(|s| s.endpoint.security == Security::Tls)
            .or_else(|| list.first())
    }
    let imap = pick(&candidates.imap)?.clone();
    let smtp = pick(&candidates.smtp)?.clone();
    let outside = |s: &Setting| !within(&s.endpoint.host, domain);
    let confirm = outside(&imap) || outside(&smtp);
    Some(Found {
        imap,
        smtp,
        source,
        confirm,
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
        confirm: false,
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
                port: 993,
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
        let found = best(&candidates, Source::Provider, "example.org").unwrap();
        assert_eq!(found.imap.endpoint.host, "b");
        assert_eq!(found.smtp.endpoint.host, "c");
        let no_smtp = Candidates {
            smtp: vec![],
            ..candidates
        };
        assert_eq!(best(&no_smtp, Source::Provider, "example.org"), None);
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
        assert!(!found.confirm);
    }

    #[test]
    fn hostnames_need_non_empty_labels_without_dash_edges() {
        for good in ["mail.example.org", "a-b.example.org", "x"] {
            assert!(is_hostname(good), "{good}");
        }
        for bad in ["", "a..b", ".a", "a.", "-a.com", "a-.com", "a.b-", "a.-b.c"] {
            assert!(!is_hostname(bad), "{bad}");
        }
    }

    #[test]
    fn servers_outside_the_domain_need_confirming_from_any_source() {
        let pair = |imap: &str, smtp: &str| Candidates {
            imap: vec![setting(imap, 993, Security::Tls)],
            smtp: vec![setting(smtp, 465, Security::Tls)],
        };
        let confirm =
            |c: &Candidates, source: Source| best(c, source, "example.org").unwrap().confirm;
        assert!(!confirm(
            &pair("imap.example.org", "smtp.example.org"),
            Source::Srv
        ));
        assert!(!confirm(
            &pair("mail.example.org.", "EXAMPLE.org"),
            Source::Srv
        ));
        assert!(confirm(
            &pair("evil.attacker.net.", "smtp.example.org"),
            Source::Srv
        ));
        assert!(confirm(
            &pair("imap.example.org", "smtp.attacker.net"),
            Source::Srv
        ));
        assert!(confirm(
            &pair("badexample.org", "smtp.example.org"),
            Source::Srv
        ));
        for source in [Source::Ispdb, Source::Provider, Source::WellKnown] {
            let off = pair("evil.attacker.net", "smtp.attacker.net");
            assert!(confirm(&off, source), "{source:?}");
            let on = pair("imap.example.org", "smtp.example.org");
            assert!(!confirm(&on, source), "{source:?}");
        }
    }
}
