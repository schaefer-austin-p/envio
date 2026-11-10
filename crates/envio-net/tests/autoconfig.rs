//! Autoconfig's order end to end with scripted lookups, and the HTTPS
//! fetch against an in-process server.

mod support;

use std::collections::HashMap;

use envio_net::autoconfig::{self, Internet, Lookups, Source, SrvRecord};
use envio_net::{Endpoint, NetError, Roots, Security};
use support::certs::Ca;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

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

/// An HTTPS server on localhost: /moved redirects to /config, which
/// answers `body`; anything else is 404.
async fn https_server(ca: &Ca, body: String) -> u16 {
    let acceptor = ca.sign("localhost").acceptor();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            let body = body.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let mut buf = vec![0; 4096];
                let n = tls.read(&mut buf).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).into_owned();
                let reply = if request.starts_with("GET /moved ") {
                    format!(
                        "HTTP/1.1 301 Moved\r\nLocation: https://localhost:{port}/config\r\n\r\n"
                    )
                } else if request.starts_with("GET /config ") {
                    format!("HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\n\r\n{body}")
                } else {
                    "HTTP/1.1 404 Not Found\r\n\r\n".to_owned()
                };
                let _ = tls.write_all(reply.as_bytes()).await;
                let _ = tls.shutdown().await;
            });
        }
    });
    port
}

#[tokio::test]
async fn https_fetch_follows_a_redirect_and_needs_a_trusted_certificate() {
    let ca = Ca::new();
    let port = https_server(&ca, config("x.example.org")).await;
    let trusted = Internet::new(Roots::from_der([ca.der()]));
    let body = trusted
        .fetch(&format!("https://localhost:{port}/moved"))
        .await
        .unwrap();
    assert!(body.contains("x.example.org"));
    assert_eq!(
        trusted
            .fetch(&format!("https://localhost:{port}/missing"))
            .await,
        None
    );

    // Autoconfig has no trust prompt: an unknown certificate is no answer.
    let untrusted = Internet::new(Roots::empty());
    assert_eq!(
        untrusted
            .fetch(&format!("https://localhost:{port}/config"))
            .await,
        None
    );
}
