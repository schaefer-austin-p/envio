//! The OAuth token client against a fake token endpoint over TLS: what it
//! sends, how every reply maps to `Tokens` or a `TokenError`, and that no
//! secret reaches a log or a `Debug` text.

mod support;

use std::sync::Mutex;
use std::time::{Duration, Instant};

use envio_net::Roots;
use envio_net::oauth::{Provider, TokenError, Tokens, exchange_code, refresh, refresh_within};
use secrecy::{ExposeSecret, SecretString};
use support::certs::Ca;
use support::fake_token::{Step, Transcript, serve};

/// The body cap in `http` (MAX_BODY). That module is private, so the value
/// is repeated here.
const CAP: usize = 256 * 1024;

fn secret(s: &str) -> SecretString {
    SecretString::from(s.to_owned())
}

/// A provider whose token endpoint is the fake at `port`, and roots that
/// trust it.
fn setup(ca: &Ca, port: u16) -> (Provider, Roots) {
    let mut p = Provider::microsoft_template().with_client_id("test-client-id");
    p.token_url = format!("https://127.0.0.1:{port}/token");
    (p, Roots::from_der([ca.der()]))
}

async fn fake(script: Vec<Step>) -> (Provider, Roots, Transcript) {
    let ca = Ca::new();
    let (port, transcript) = serve(&ca.sign("127.0.0.1"), script).await;
    let (p, roots) = setup(&ca, port);
    (p, roots, transcript)
}

fn ok_body(access: &str, refresh: &str) -> String {
    format!(r#"{{"access_token":"{access}","refresh_token":"{refresh}","expires_in":3599}}"#)
}

async fn exchange(p: &Provider, roots: &Roots) -> Result<Tokens, TokenError> {
    exchange_code(p, roots, &secret("the-code"), &secret("the-verifier")).await
}

async fn refreshed(p: &Provider, roots: &Roots) -> Result<Tokens, TokenError> {
    refresh(p, roots, &secret("old-refresh")).await
}

#[tokio::test]
async fn the_code_grant_posts_its_fields_and_no_client_secret() {
    let (p, roots, seen) = fake(vec![Step::Reply(200, ok_body("AT-1", "RT-1"))]).await;
    let tokens = exchange(&p, &roots).await.unwrap();
    assert_eq!(tokens.access.expose_secret(), "AT-1");
    assert_eq!(tokens.refresh.unwrap().expose_secret(), "RT-1");
    assert_eq!(tokens.expires_in, 3599);

    let requests = seen.requests();
    assert_eq!(requests.len(), 1);
    let r = &requests[0];
    assert_eq!(r.line, "POST /token");
    assert_eq!(r.content_type, "application/x-www-form-urlencoded");
    let mut keys: Vec<_> = r.form.iter().map(|(k, _)| k.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "client_id",
            "code",
            "code_verifier",
            "grant_type",
            "redirect_uri"
        ]
    );
    assert_eq!(r.get("grant_type"), Some("authorization_code"));
    assert_eq!(r.get("client_id"), Some("test-client-id"));
    assert_eq!(r.get("code"), Some("the-code"));
    assert_eq!(r.get("code_verifier"), Some("the-verifier"));
    assert_eq!(r.get("redirect_uri"), Some(p.redirect_uri.as_str()));
    assert_eq!(r.get("client_secret"), None);
}

#[tokio::test]
async fn the_refresh_grant_posts_its_fields_and_no_client_secret() {
    let (p, roots, seen) = fake(vec![Step::Reply(200, ok_body("AT-2", "RT-2"))]).await;
    let tokens = refreshed(&p, &roots).await.unwrap();
    assert_eq!(tokens.access.expose_secret(), "AT-2");

    let r = &seen.requests()[0];
    let mut keys: Vec<_> = r.form.iter().map(|(k, _)| k.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["client_id", "grant_type", "refresh_token", "scope"]);
    assert_eq!(r.get("grant_type"), Some("refresh_token"));
    assert_eq!(r.get("refresh_token"), Some("old-refresh"));
    assert_eq!(r.get("scope"), Some(p.scopes));
    assert_eq!(r.get("client_secret"), None);
}

#[tokio::test]
async fn a_reply_without_a_refresh_token_still_parses() {
    let body = r#"{"access_token":"AT-3","expires_in":60}"#.to_owned();
    let (p, roots, _) = fake(vec![Step::Reply(200, body)]).await;
    let tokens = refreshed(&p, &roots).await.unwrap();
    assert!(tokens.refresh.is_none());
}

#[tokio::test]
async fn invalid_grant_is_revoked() {
    let body = r#"{"error":"invalid_grant","error_description":"AADSTS70008 expired"}"#;
    let (p, roots, _) = fake(vec![Step::Reply(400, body.into())]).await;
    assert_eq!(
        refreshed(&p, &roots).await.unwrap_err(),
        TokenError::Revoked
    );
}

#[tokio::test]
async fn a_503_a_cut_connection_and_a_stall_are_temporary() {
    let script = vec![Step::Reply(503, "{}".into()), Step::Cut, Step::Stall];
    let (p, roots, seen) = fake(script).await;
    assert_eq!(
        refreshed(&p, &roots).await.unwrap_err(),
        TokenError::Temporary
    );
    assert_eq!(
        refreshed(&p, &roots).await.unwrap_err(),
        TokenError::Temporary
    );
    let started = Instant::now();
    let stalled = tokio::time::timeout(
        Duration::from_secs(10),
        refresh_within(&p, &roots, &secret("r"), Duration::from_millis(300)),
    )
    .await
    .expect("the call must give up by itself");
    assert_eq!(stalled.unwrap_err(), TokenError::Temporary);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(seen.requests().len(), 3);
}

#[tokio::test]
async fn an_html_200_is_not_a_token() {
    let html = "<html><body>Sign in to your network</body></html>";
    let (p, roots, _) = fake(vec![Step::Reply(200, html.into())]).await;
    assert!(matches!(
        refreshed(&p, &roots).await,
        Err(TokenError::Refused(_))
    ));
}

#[tokio::test]
async fn a_redirect_is_not_followed() {
    // The target would answer with tokens; the provider names the first
    // endpoint, which redirects to it. One CA signs both.
    let ca = Ca::new();
    let ok = ok_body("AT-R", "RT-R");
    let (target_port, target_seen) = serve(&ca.sign("127.0.0.1"), vec![Step::Reply(200, ok)]).await;
    let location = format!("https://127.0.0.1:{target_port}/token");
    let redirect = format!("HTTP/1.0 302 Found\r\nLocation: {location}\r\n\r\n");
    let script = vec![Step::Raw(redirect.into_bytes())];
    let (port, first) = serve(&ca.sign("127.0.0.1"), script).await;
    let (p, roots) = setup(&ca, port);

    let err = refreshed(&p, &roots).await.unwrap_err();
    assert!(matches!(err, TokenError::Refused(_)), "{err:?}");
    assert_eq!(first.requests().len(), 1);
    assert!(
        target_seen.requests().is_empty(),
        "the redirect was followed"
    );
}

#[tokio::test]
async fn a_body_over_the_cap_is_refused() {
    let big = ok_body(&"A".repeat(CAP + 1024), "RT");
    let (p, roots, _) = fake(vec![Step::Reply(200, big)]).await;
    assert!(refreshed(&p, &roots).await.is_err());
}

#[tokio::test]
async fn an_untrusted_certificate_is_never_a_token() {
    let ca = Ca::new();
    let (port, seen) = serve(
        &ca.sign("127.0.0.1"),
        vec![Step::Reply(200, ok_body("AT", "RT"))],
    )
    .await;
    let (p, _) = setup(&ca, port);
    let err = refreshed(&p, &Roots::empty()).await.unwrap_err();
    assert!(matches!(err, TokenError::Refused(_)), "{err:?}");
    assert!(
        seen.requests().is_empty(),
        "the secrets went to an untrusted host"
    );
}

/// Collects every log record, at every level, as text.
struct Capture;
static LOGS: Mutex<String> = Mutex::new(String::new());
impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, r: &log::Record) {
        let mut logs = LOGS.lock().unwrap();
        logs.push_str(&format!("{} {}\n", r.target(), r.args()));
    }
    fn flush(&self) {}
}

#[tokio::test]
async fn no_token_code_or_verifier_reaches_a_log_or_a_debug_text() {
    log::set_logger(&Capture).unwrap();
    log::set_max_level(log::LevelFilter::Trace);

    let reply = |s: &str| format!(r#"{{"error":"invalid_grant","error_description":"{s}"}}"#);
    let script = vec![
        Step::Reply(200, ok_body("SECRET-ACCESS", "SECRET-REFRESH")),
        Step::Reply(400, reply("SECRET-ACCESS SECRET-CODE")),
        Step::Reply(503, "SECRET-ACCESS".into()),
        Step::Reply(200, "SECRET-ACCESS not json".into()),
        Step::Cut,
    ];
    let (p, roots, _) = fake(script).await;
    let (code, verifier, old) = (
        secret("SECRET-CODE"),
        secret("SECRET-VERIFIER"),
        secret("SECRET-OLD-REFRESH"),
    );
    let mut texts = Vec::new();
    texts.push(format!(
        "{:?}",
        exchange_code(&p, &roots, &code, &verifier).await
    ));
    for _ in 0..4 {
        texts.push(format!("{:?}", refresh(&p, &roots, &old).await));
    }
    texts.push(format!("{p:?}"));
    let all = format!("{}{}", texts.join("\n"), LOGS.lock().unwrap());
    assert!(all.len() > 100, "the capture saw nothing: {all}");
    for needle in [
        "SECRET-ACCESS",
        "SECRET-REFRESH",
        "SECRET-CODE",
        "SECRET-VERIFIER",
        "SECRET-OLD-REFRESH",
    ] {
        assert!(!all.contains(needle), "{needle} leaked");
    }
}
