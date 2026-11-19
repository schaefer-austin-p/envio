//! The OAuth token client against a fake token endpoint over TLS: what it
//! sends, how every reply maps to `Tokens` or a `TokenError`, and that no
//! secret reaches a log or a `Debug` text.

mod support;

use std::sync::Mutex;
use std::time::Duration;

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
async fn a_503_and_a_cut_connection_are_temporary() {
    let (p, roots, seen) = fake(vec![Step::Reply(503, "{}".into()), Step::Cut]).await;
    for _ in 0..2 {
        assert_eq!(
            refreshed(&p, &roots).await.unwrap_err(),
            TokenError::Temporary
        );
    }
    assert_eq!(seen.requests().len(), 2);
}

#[tokio::test]
async fn a_stall_ends_as_temporary_at_the_limit() {
    let (p, roots, seen) = fake(vec![Step::Stall]).await;
    // The limit is generous so a slow runner still gets through the TLS
    // handshake; the outer guard only keeps a missing timeout from hanging.
    let stalled = tokio::time::timeout(
        Duration::from_secs(60),
        refresh_within(&p, &roots, &secret("r"), Duration::from_secs(3)),
    )
    .await
    .expect("the call must give up by itself");
    assert_eq!(stalled.unwrap_err(), TokenError::Temporary);
    // The server records the request after the client has sent it; wait
    // for that rather than racing it.
    for _ in 0..100 {
        if !seen.requests().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(seen.requests().len(), 1);
}

#[tokio::test]
async fn a_reply_that_is_not_http_is_temporary() {
    let (p, roots, _) = fake(vec![Step::Raw(b"\x00\x01 not http".to_vec())]).await;
    assert_eq!(
        refreshed(&p, &roots).await.unwrap_err(),
        TokenError::Temporary
    );
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
async fn a_body_over_the_cap_is_refused_for_good() {
    let big = ok_body(&"A".repeat(CAP + 1024), "RT");
    let (p, roots, _) = fake(vec![Step::Reply(200, big)]).await;
    assert_eq!(
        refreshed(&p, &roots).await.unwrap_err(),
        TokenError::Refused("reply_too_large".to_owned())
    );
}

#[tokio::test]
async fn a_body_over_the_cap_without_a_length_is_refused_for_good() {
    // HTTP/1.0 with no Content-Length: the body runs to the close.
    let mut raw = b"HTTP/1.0 200 OK\r\n\r\n".to_vec();
    raw.extend(ok_body(&"A".repeat(CAP + 1024), "RT").into_bytes());
    let (p, roots, _) = fake(vec![Step::Raw(raw)]).await;
    assert_eq!(
        refreshed(&p, &roots).await.unwrap_err(),
        TokenError::Refused("reply_too_large".to_owned())
    );
}

#[tokio::test]
async fn a_certificate_fault_is_temporary_and_sends_nothing() {
    let ca = Ca::new();
    let ok = || vec![Step::Reply(200, ok_body("AT", "RT"))];
    // An unknown issuer (roots that trust nothing), and an expired
    // certificate (a wrong clock, a captive portal): both clear by
    // themselves, so neither may read as "sign in again".
    let (port, unknown) = serve(&ca.sign("127.0.0.1"), ok()).await;
    let (p, _) = setup(&ca, port);
    let err = refreshed(&p, &Roots::empty()).await.unwrap_err();
    assert_eq!(err, TokenError::Temporary);

    let (port, expired) = serve(&ca.sign_expired("127.0.0.1"), ok()).await;
    let (p, roots) = setup(&ca, port);
    assert_eq!(
        refreshed(&p, &roots).await.unwrap_err(),
        TokenError::Temporary
    );
    for seen in [unknown, expired] {
        assert!(
            seen.requests().is_empty(),
            "the secrets went to an untrusted host"
        );
    }
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

// Chunked replies (#308): a token endpoint may answer with
// `Transfer-Encoding: chunked`.

fn chunked(status: &str, body: &str) -> Vec<u8> {
    format!("HTTP/1.1 {status}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{body}")
        .into_bytes()
}

/// `text` as chunks of `size` bytes.
fn in_chunks(text: &str, size: usize) -> String {
    let mut out = String::new();
    for part in text.as_bytes().chunks(size) {
        out += &format!(
            "{:x}\r\n{}\r\n",
            part.len(),
            std::str::from_utf8(part).unwrap()
        );
    }
    out + "0\r\n\r\n"
}

#[tokio::test]
async fn a_chunked_token_reply_is_read() {
    let body = in_chunks(&ok_body("AT-CHUNKED", "RT-CHUNKED"), 7);
    let (p, roots, _) = fake(vec![Step::Raw(chunked("200 OK", &body))]).await;
    let tokens = refreshed(&p, &roots).await.unwrap();
    assert_eq!(tokens.access.expose_secret(), "AT-CHUNKED");
    assert_eq!(tokens.refresh.unwrap().expose_secret(), "RT-CHUNKED");
}

#[tokio::test]
async fn a_chunked_error_reply_keeps_its_meaning() {
    let body = in_chunks(r#"{"error":"invalid_grant"}"#, 5);
    let (p, roots, _) = fake(vec![Step::Raw(chunked("400 Bad Request", &body))]).await;
    assert_eq!(
        refreshed(&p, &roots).await.unwrap_err(),
        TokenError::Revoked
    );
}

#[tokio::test]
async fn chunk_extensions_and_trailers_do_not_matter() {
    let json = ok_body("AT-EXT", "RT-EXT");
    let (a, b) = json.split_at(10);
    let body = format!(
        "{:x};ext=1\r\n{a}\r\n{:X};a;b=\"c\"\r\n{b}\r\n0;done\r\nExpires: never\r\nX-A: b\r\n\r\n",
        a.len(),
        b.len()
    );
    let (p, roots, _) = fake(vec![Step::Raw(chunked("200 OK", &body))]).await;
    let tokens = exchange(&p, &roots).await.unwrap();
    assert_eq!(tokens.access.expose_secret(), "AT-EXT");
}

#[tokio::test]
async fn a_truncated_chunk_is_refused_for_good_not_retried() {
    // The size promises 100 bytes; the server sends 10 and closes cleanly.
    let body = format!("64\r\n{}", &ok_body("A", "R")[..10]);
    let (p, roots, seen) = fake(vec![Step::Raw(chunked("200 OK", &body))]).await;
    assert_eq!(
        refreshed(&p, &roots).await.unwrap_err(),
        TokenError::Refused("reply_malformed".to_owned())
    );
    assert_eq!(seen.requests().len(), 1);
}

#[tokio::test]
async fn a_missing_last_chunk_is_refused_for_good() {
    let (p, roots, _) = fake(vec![Step::Raw(chunked("200 OK", "5\r\nhello\r\n"))]).await;
    assert_eq!(
        refreshed(&p, &roots).await.unwrap_err(),
        TokenError::Refused("reply_malformed".to_owned())
    );
}

#[tokio::test]
async fn a_bad_chunk_size_is_refused_for_good() {
    let raw = chunked("200 OK", "xyz\r\n{}\r\n0\r\n\r\n");
    let (p, roots, _) = fake(vec![Step::Raw(raw)]).await;
    assert_eq!(
        refreshed(&p, &roots).await.unwrap_err(),
        TokenError::Refused("reply_malformed".to_owned())
    );
}

#[tokio::test]
async fn an_oversized_chunk_is_refused_for_good_with_a_bounded_body() {
    // One chunk over the cap, and far more data behind it than the cap.
    let big = "A".repeat(CAP * 2);
    let body = format!("{:x}\r\n{big}\r\n0\r\n\r\n", big.len());
    let (p, roots, _) = fake(vec![Step::Raw(chunked("200 OK", &body))]).await;
    assert_eq!(
        refreshed(&p, &roots).await.unwrap_err(),
        TokenError::Refused("reply_too_large".to_owned())
    );
}

#[tokio::test]
async fn many_small_chunks_over_the_cap_are_refused_for_good() {
    let body = in_chunks(&"A".repeat(CAP + 4096), 1000);
    let (p, roots, _) = fake(vec![Step::Raw(chunked("200 OK", &body))]).await;
    assert_eq!(
        refreshed(&p, &roots).await.unwrap_err(),
        TokenError::Refused("reply_too_large".to_owned())
    );
}

#[tokio::test]
async fn an_unsupported_transfer_encoding_is_refused_for_good() {
    let raw = "HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\n\r\n2\r\n{}\r\n0\r\n\r\n";
    let (p, roots, _) = fake(vec![Step::Raw(raw.as_bytes().to_vec())]).await;
    assert_eq!(
        refreshed(&p, &roots).await.unwrap_err(),
        TokenError::Refused("reply_malformed".to_owned())
    );
}

#[tokio::test]
async fn a_content_length_reply_still_works() {
    let json = ok_body("AT-LEN", "RT-LEN");
    let raw = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{json}",
        json.len()
    );
    let (p, roots, _) = fake(vec![Step::Raw(raw.into_bytes())]).await;
    let tokens = refreshed(&p, &roots).await.unwrap();
    assert_eq!(tokens.access.expose_secret(), "AT-LEN");
}

#[tokio::test]
async fn bad_framing_in_a_try_later_reply_is_temporary() {
    let gzip = "HTTP/1.1 503 Unavailable\r\nTransfer-Encoding: gzip\r\n\r\n2\r\n{}\r\n0\r\n\r\n";
    let cut = chunked("503 Unavailable", "5\r\nhel");
    let broken = chunked("429 Too Many Requests", "zz\r\n{}\r\n0\r\n\r\n");
    for raw in [gzip.as_bytes().to_vec(), cut, broken] {
        let (p, roots, _) = fake(vec![Step::Raw(raw)]).await;
        assert_eq!(
            refreshed(&p, &roots).await.unwrap_err(),
            TokenError::Temporary
        );
    }
}

#[tokio::test]
async fn chunk_framing_that_fills_the_read_cap_is_refused_as_too_big() {
    // One-byte chunks and no last chunk: the framing alone passes the cap.
    let body = "1\r\nx\r\n".repeat(CAP / 3);
    let (p, roots, _) = fake(vec![Step::Raw(chunked("200 OK", &body))]).await;
    assert_eq!(
        refreshed(&p, &roots).await.unwrap_err(),
        TokenError::Refused("reply_too_large".to_owned())
    );
}
