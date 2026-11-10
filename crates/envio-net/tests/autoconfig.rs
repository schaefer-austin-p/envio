//! Autoconfig's order end to end with scripted lookups. The HTTPS fetch
//! is in `autoconfig_fetch.rs`.

use std::collections::HashMap;
use std::time::Duration;

use envio_net::autoconfig::{self, Lookups, Source, SrvRecord};
use envio_net::{Endpoint, NetError, Security};

#[derive(Default)]
struct Scripted {
    files: HashMap<String, String>,
    srv: HashMap<String, Vec<SrvRecord>>,
}

impl Lookups for Scripted {
    async fn fetch(&self, url: &str) -> Option<String> {
        self.files.get(url).cloned()
    }

    async fn srv(&self, name: &str) -> Vec<SrvRecord> {
        self.srv.get(name).cloned().unwrap_or_default()
    }
}

fn config(imap_host: &str) -> String {
    format!(
        "<clientConfig version=\"1.1\"><emailProvider id=\"x\">\
         <incomingServer type=\"imap\"><hostname>{imap_host}</hostname><port>993</port>\
         <socketType>SSL</socketType><username>%EMAILADDRESS%</username></incomingServer>\
         <outgoingServer type=\"smtp\"><hostname>smtp.example.org</hostname><port>465</port>\
         <socketType>SSL</socketType><username>%EMAILADDRESS%</username></outgoingServer>\
         </emailProvider></clientConfig>"
    )
}

#[tokio::test]
async fn the_ispdb_answers_first() {
    let mut lookups = Scripted::default();
    lookups.files.insert(
        "https://autoconfig.thunderbird.net/v1.1/example.org".into(),
        config("ispdb.example.org"),
    );
    lookups.files.insert(
        "https://autoconfig.example.org/mail/config-v1.1.xml".into(),
        config("provider.example.org"),
    );
    let found = autoconfig::detect("me@example.org", &lookups)
        .await
        .unwrap();
    assert_eq!(found.source, Source::Ispdb);
    assert_eq!(
        found.imap.endpoint,
        Endpoint::new("ispdb.example.org", 993, Security::Tls)
    );
    assert_eq!(found.imap.username, "me@example.org");
}

#[tokio::test]
async fn a_useless_file_falls_through_to_the_next_source() {
    let mut lookups = Scripted::default();
    lookups.files.insert(
        "https://autoconfig.thunderbird.net/v1.1/example.org".into(),
        "<html>404</html>".into(),
    );
    lookups.files.insert(
        "https://example.org/.well-known/autoconfig/mail/config-v1.1.xml".into(),
        config("wellknown.example.org"),
    );
    let found = autoconfig::detect("me@example.org", &lookups)
        .await
        .unwrap();
    assert_eq!(found.source, Source::WellKnown);
}

#[tokio::test]
async fn srv_records_come_after_the_files() {
    let mut lookups = Scripted::default();
    let record = |target: &str, port| SrvRecord {
        priority: 0,
        weight: 0,
        port,
        target: target.into(),
    };
    lookups.srv.insert(
        "_imap._tcp.example.org".into(),
        vec![record("mail.example.org.", 143)],
    );
    lookups
        .srv
        .insert("_imaps._tcp.example.org".into(), vec![record(".", 0)]);
    lookups.srv.insert(
        "_submission._tcp.example.org".into(),
        vec![record("mail.example.org.", 587)],
    );
    let found = autoconfig::detect("me@example.org", &lookups)
        .await
        .unwrap();
    assert_eq!(found.source, Source::Srv);
    assert_eq!(
        found.imap.endpoint,
        Endpoint::new("mail.example.org", 143, Security::StartTls)
    );
    assert_eq!(
        found.smtp.endpoint,
        Endpoint::new("mail.example.org", 587, Security::StartTls)
    );
}

#[tokio::test]
async fn with_nothing_found_the_guess_comes_back() {
    let found = autoconfig::detect("me@example.org", &Scripted::default())
        .await
        .unwrap();
    assert_eq!(found.source, Source::Guess);
    assert_eq!(found.imap.endpoint.host, "imap.example.org");
}

#[tokio::test]
async fn an_address_without_a_domain_is_refused() {
    let err = autoconfig::detect("not-an-address", &Scripted::default())
        .await
        .unwrap_err();
    assert!(matches!(err, NetError::Refused(_)));
}

/// SRV answers for IMAP (implicit TLS) and SMTP (submission over TLS)
/// pointing at the given hosts.
fn srv_pair(imap_host: &str, smtp_host: &str) -> Scripted {
    let record = |target: &str, port| SrvRecord {
        priority: 0,
        weight: 0,
        port,
        target: target.into(),
    };
    let mut lookups = Scripted::default();
    lookups.srv.insert(
        "_imaps._tcp.example.org".into(),
        vec![record(imap_host, 993)],
    );
    lookups.srv.insert(
        "_submissions._tcp.example.org".into(),
        vec![record(smtp_host, 465)],
    );
    lookups
}

#[tokio::test]
async fn srv_servers_outside_the_domain_ask_for_confirmation() {
    let outside = srv_pair("imap.attacker.net.", "smtp.example.org.");
    let found = autoconfig::detect("me@example.org", &outside)
        .await
        .unwrap();
    assert_eq!(found.source, Source::Srv);
    assert!(found.confirm);

    let inside = srv_pair("imap.example.org.", "smtp.example.org.");
    let found = autoconfig::detect("me@example.org", &inside).await.unwrap();
    assert_eq!(found.source, Source::Srv);
    assert!(!found.confirm);
}

/// Hangs on every lookup: only the budget can end the search.
struct Hangs;

impl Lookups for Hangs {
    async fn fetch(&self, _url: &str) -> Option<String> {
        std::future::pending().await
    }

    async fn srv(&self, _name: &str) -> Vec<SrvRecord> {
        std::future::pending().await
    }
}

#[tokio::test]
async fn a_search_that_runs_out_of_time_gives_the_guess() {
    let found = tokio::time::timeout(
        Duration::from_secs(10),
        autoconfig::detect_within("me@example.org", &Hangs, Duration::from_millis(100)),
    )
    .await
    .expect("detect ignored its budget")
    .unwrap();
    assert_eq!(found.source, Source::Guess);
}
