//! Autoconfig's order end to end with scripted lookups. The HTTPS fetch
//! is in `autoconfig_fetch.rs`.

use std::collections::HashMap;
use std::time::Duration;

use envio_net::autoconfig::{self, Hosting, Lookups, MxRecord, Source, SrvRecord};
use envio_net::{Endpoint, NetError, Security};

#[derive(Default)]
struct Scripted {
    files: HashMap<String, String>,
    srv: HashMap<String, Vec<SrvRecord>>,
    mx: Vec<MxRecord>,
    mx_asked: std::sync::Mutex<Vec<String>>,
}

impl Lookups for Scripted {
    async fn fetch(&self, url: &str) -> Option<String> {
        self.files.get(url).cloned()
    }

    async fn srv(&self, name: &str) -> Vec<SrvRecord> {
        self.srv.get(name).cloned().unwrap_or_default()
    }

    async fn mx(&self, name: &str) -> Vec<MxRecord> {
        self.mx_asked.lock().unwrap().push(name.to_owned());
        self.mx.clone()
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

    async fn mx(&self, _name: &str) -> Vec<MxRecord> {
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

/// Answers like `Scripted`, each lookup after `delay` and the ISPDB
/// after `ispdb_delay`.
struct Slow {
    script: Scripted,
    delay: Duration,
    ispdb_delay: Duration,
}

impl Lookups for Slow {
    async fn fetch(&self, url: &str) -> Option<String> {
        let wait = if url.contains("thunderbird.net") {
            self.ispdb_delay
        } else {
            self.delay
        };
        tokio::time::sleep(wait).await;
        self.script.fetch(url).await
    }

    async fn srv(&self, name: &str) -> Vec<SrvRecord> {
        tokio::time::sleep(self.delay).await;
        self.script.srv(name).await
    }

    async fn mx(&self, name: &str) -> Vec<MxRecord> {
        tokio::time::sleep(self.delay).await;
        self.script.mx(name).await
    }
}

#[tokio::test]
async fn a_slower_ispdb_still_wins_over_the_provider() {
    let mut script = Scripted::default();
    script.files.insert(
        "https://autoconfig.thunderbird.net/v1.1/example.org".into(),
        config("ispdb.example.org"),
    );
    script.files.insert(
        "https://autoconfig.example.org/mail/config-v1.1.xml".into(),
        config("provider.example.org"),
    );
    let lookups = Slow {
        script,
        delay: Duration::ZERO,
        ispdb_delay: Duration::from_millis(200),
    };
    let found = autoconfig::detect("me@example.org", &lookups)
        .await
        .unwrap();
    assert_eq!(found.source, Source::Ispdb);
}

#[tokio::test]
async fn slow_lookups_wait_side_by_side_not_one_after_another() {
    // Three file fetches and four SRV queries of 400 ms each: 2.8 s one
    // after another, 0.8 s when each step's lookups run at once.
    let delay = Duration::from_millis(400);
    let lookups = Slow {
        script: srv_pair("imap.example.org.", "smtp.example.org."),
        delay,
        ispdb_delay: delay,
    };
    let started = std::time::Instant::now();
    let found = autoconfig::detect("me@example.org", &lookups)
        .await
        .unwrap();
    assert_eq!(found.source, Source::Srv);
    assert!(
        started.elapsed() < Duration::from_millis(2000),
        "took {:?}",
        started.elapsed()
    );
}

fn m365() -> Vec<MxRecord> {
    vec![MxRecord {
        preference: 0,
        host: "example-org.mail.protection.outlook.com.".into(),
    }]
}

fn ispdb_script() -> Scripted {
    let mut script = Scripted::default();
    script.files.insert(
        "https://autoconfig.thunderbird.net/v1.1/example.org".into(),
        config("ispdb.example.org"),
    );
    script
}

const SHORT: Duration = Duration::from_millis(100);
const BUDGET: Duration = Duration::from_secs(2);

#[tokio::test]
async fn the_verdict_and_the_settings_come_back_together() {
    let script = Scripted {
        mx: m365(),
        ..ispdb_script()
    };
    let d = autoconfig::detect_with_limits("me@example.org", &script, BUDGET, SHORT)
        .await
        .unwrap();
    assert_eq!(d.hosting, Hosting::Microsoft365);
    assert_eq!(d.found.source, Source::Ispdb);
    assert_eq!(*script.mx_asked.lock().unwrap(), ["example.org"]);
}

#[tokio::test]
async fn no_mx_records_is_unknown() {
    let d = autoconfig::detect_with_hosting("me@example.org", &ispdb_script(), BUDGET)
        .await
        .unwrap();
    assert_eq!(d.hosting, Hosting::Unknown);
    assert_eq!(d.found.source, Source::Ispdb);
}

/// Hangs on `mx` only, the rest as scripted.
struct MxHangs(Scripted);

impl Lookups for MxHangs {
    async fn fetch(&self, url: &str) -> Option<String> {
        self.0.fetch(url).await
    }

    async fn srv(&self, name: &str) -> Vec<SrvRecord> {
        self.0.srv(name).await
    }

    async fn mx(&self, _name: &str) -> Vec<MxRecord> {
        std::future::pending().await
    }
}

#[tokio::test]
async fn a_hanging_mx_lookup_ends_at_its_own_limit_and_keeps_the_found_settings() {
    let started = std::time::Instant::now();
    let d =
        autoconfig::detect_with_limits("me@example.org", &MxHangs(ispdb_script()), BUDGET, SHORT)
            .await
            .unwrap();
    assert_eq!(d.hosting, Hosting::Unknown);
    assert_eq!(d.found.source, Source::Ispdb);
    assert!(
        started.elapsed() < Duration::from_millis(1000),
        "{:?}",
        started.elapsed()
    );
}

/// Hangs on everything but `mx`.
struct FindHangs(Scripted);

impl Lookups for FindHangs {
    async fn fetch(&self, _url: &str) -> Option<String> {
        std::future::pending().await
    }

    async fn srv(&self, _name: &str) -> Vec<SrvRecord> {
        std::future::pending().await
    }

    async fn mx(&self, name: &str) -> Vec<MxRecord> {
        self.0.mx(name).await
    }
}

#[tokio::test]
async fn a_hanging_find_does_not_cost_the_verdict() {
    let script = Scripted {
        mx: m365(),
        ..Default::default()
    };
    let d = autoconfig::detect_with_limits(
        "me@example.org",
        &FindHangs(script),
        Duration::from_millis(300),
        Duration::from_secs(5),
    )
    .await
    .unwrap();
    assert_eq!(d.hosting, Hosting::Microsoft365);
    assert_eq!(d.found.source, Source::Guess);
}

#[tokio::test]
async fn when_everything_hangs_the_budget_gives_the_guess_and_unknown() {
    let started = std::time::Instant::now();
    let d = autoconfig::detect_with_limits(
        "me@example.org",
        &Hangs,
        Duration::from_millis(200),
        Duration::from_secs(5),
    )
    .await
    .unwrap();
    assert_eq!(d.hosting, Hosting::Unknown);
    assert_eq!(d.found.source, Source::Guess);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
}
