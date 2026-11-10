//! SMTP against a scripted in-process server: what goes over the wire in
//! the clear, the generic EHLO name, and the STARTTLS refusals.

mod support;

use envio_net::smtp::{EHLO_NAME, SmtpSession};
use envio_net::{Credentials, Endpoint, Fingerprint, NetError, Roots, Security};
use support::certs::Cert;
use support::fake_smtp::{Script, serve};

const MAIL: &[u8] =
    b"From: ada@example.org\r\nTo: bob@example.org\r\nSubject: hi\r\n\r\nHello.\r\n";

/// Standard base64 with padding, the form AUTH PLAIN carries.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = |i: usize| u32::from(*chunk.get(i).unwrap_or(&0));
        let n = (b(0) << 16) | (b(1) << 8) | b(2);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn endpoint(port: u16, security: Security, cert: &Cert) -> Endpoint {
    Endpoint {
        pin: Some(Fingerprint::of(&cert.der)),
        ..Endpoint::new("localhost", port, security)
    }
}

async fn send_one(endpoint: &Endpoint) -> Result<(), NetError> {
    let mut session = SmtpSession::connect(endpoint, &Roots::empty()).await?;
    session.login(&Credentials::new("ada", "secret-pw")).await?;
    session
        .send("ada@example.org", &["bob@example.org".to_owned()], MAIL)
        .await?;
    session.quit().await
}

#[tokio::test]
async fn starttls_sends_only_ehlo_and_starttls_in_the_clear() {
    let cert = Cert::self_signed("localhost");
    let (port, transcript) = serve(&cert, Script::default()).await;
    send_one(&endpoint(port, Security::StartTls, &cert))
        .await
        .unwrap();
    let lines = transcript.lines();
    let clear: Vec<_> = lines
        .iter()
        .filter(|(tls, _)| !tls)
        .map(|(_, l)| l.as_str())
        .collect();
    assert_eq!(clear, [format!("EHLO {EHLO_NAME}").as_str(), "STARTTLS"]);
    let secure: Vec<_> = lines
        .iter()
        .filter(|(tls, _)| *tls)
        .map(|(_, l)| l.clone())
        .collect();
    assert_eq!(secure[0], "EHLO [127.0.0.1]");
    assert!(
        secure.iter().any(|l| l.starts_with("AUTH PLAIN")),
        "{secure:?}"
    );
    assert!(secure.iter().any(|l| l == "Subject: hi"), "{secure:?}");
    // AUTH PLAIN is base64, so look for the encoded credentials: they
    // must appear, and only on lines that went over TLS.
    let token = base64(b"\0ada\0secret-pw");
    let carrying: Vec<_> = lines.iter().filter(|(_, l)| l.contains(&token)).collect();
    assert!(!carrying.is_empty(), "{lines:?}");
    assert!(carrying.iter().all(|(tls, _)| *tls), "{lines:?}");
}

#[tokio::test]
async fn implicit_tls_greets_with_the_generic_name() {
    let cert = Cert::self_signed("localhost");
    let script = Script {
        implicit: true,
        ..Script::default()
    };
    let (port, transcript) = serve(&cert, script).await;
    send_one(&endpoint(port, Security::Tls, &cert))
        .await
        .unwrap();
    let lines = transcript.lines();
    assert!(lines.iter().all(|(tls, _)| *tls));
    assert_eq!(lines[0].1, "EHLO [127.0.0.1]");
}

#[tokio::test]
async fn a_server_without_starttls_is_refused_before_any_secret_is_sent() {
    let cert = Cert::self_signed("localhost");
    let script = Script {
        offer_starttls: false,
        ..Script::default()
    };
    let (port, transcript) = serve(&cert, script).await;
    let err = send_one(&endpoint(port, Security::StartTls, &cert))
        .await
        .unwrap_err();
    assert!(matches!(err, NetError::NoStartTls), "{err:?}");
    let lines = transcript.lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0].1, "EHLO [127.0.0.1]");
}

#[tokio::test]
async fn bytes_injected_after_the_starttls_reply_are_refused() {
    let cert = Cert::self_signed("localhost");
    let script = Script {
        inject: true,
        ..Script::default()
    };
    let (port, _) = serve(&cert, script).await;
    let err = send_one(&endpoint(port, Security::StartTls, &cert))
        .await
        .unwrap_err();
    assert!(
        matches!(&err, NetError::Protocol(m) if m == "data after the STARTTLS reply"),
        "{err:?}"
    );
}

#[tokio::test]
async fn a_greeting_cut_short_is_a_temporary_io_error() {
    let cert = Cert::self_signed("localhost");
    let script = Script {
        cut_greeting: true,
        ..Script::default()
    };
    let (port, _) = serve(&cert, script).await;
    let err = send_one(&endpoint(port, Security::StartTls, &cert))
        .await
        .unwrap_err();
    assert!(matches!(err, NetError::Io(_)), "{err:?}");
    assert!(err.is_temporary());
}

#[tokio::test]
async fn a_rejected_password_is_a_sign_in_error() {
    let cert = Cert::self_signed("localhost");
    let script = Script {
        auth_reply: "535 5.7.8 bad credentials",
        ..Script::default()
    };
    let (port, _) = serve(&cert, script).await;
    let err = send_one(&endpoint(port, Security::StartTls, &cert))
        .await
        .unwrap_err();
    assert!(matches!(err, NetError::SignIn(_)), "{err:?}");
    assert!(!err.is_temporary());
}

#[tokio::test]
async fn an_unpinned_self_signed_smtp_server_asks_the_user() {
    let cert = Cert::self_signed("localhost");
    let (port, transcript) = serve(&cert, Script::default()).await;
    let unpinned = Endpoint::new("localhost", port, Security::StartTls);
    let err = send_one(&unpinned).await.unwrap_err();
    assert!(matches!(err, NetError::Untrusted { .. }), "{err:?}");
    assert!(transcript.lines().iter().all(|(tls, _)| !tls));
}
