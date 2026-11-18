//! XOAUTH2 against the scripted IMAP and SMTP servers: a bearer credential
//! signs in with XOAUTH2 only, every Microsoft failure shape is a sign-in
//! error, and the token never goes to a server that cannot take it.

mod support;

use envio_net::imap::ImapSession;
use envio_net::smtp::SmtpSession;
use envio_net::{Credentials, Endpoint, Fingerprint, NetError, Roots, Security};
use support::certs::Cert;
use support::fake_imap::{self, base64_encode};
use support::fake_smtp;

const GOOD: &str = "user=ada@outlook.example\x01auth=Bearer good-token\x01\x01";

fn pinned(port: u16, cert: &Cert) -> Endpoint {
    Endpoint {
        pin: Some(Fingerprint::of(&cert.der)),
        ..Endpoint::new("localhost", port, Security::Tls)
    }
}

fn sends_password(lines: &[String]) -> bool {
    lines.iter().any(|l| {
        l.contains("LOGIN")
            || l.contains("PLAIN")
            || l.contains(&base64_encode(b"\0ada\0good-token"))
    })
}

async fn imap_open(
    script: fake_imap::Script,
    token: &str,
) -> (Result<ImapSession, NetError>, Vec<String>) {
    let cert = Cert::self_signed("localhost");
    let (port, transcript) = fake_imap::serve(&cert, script).await;
    let credentials = Credentials::bearer("ada@outlook.example", token);
    let result = ImapSession::open(&pinned(port, &cert), &Roots::empty(), &credentials).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let lines = transcript.lines().into_iter().map(|(_, l)| l).collect();
    (result, lines)
}

fn imap_script() -> fake_imap::Script {
    fake_imap::Script {
        implicit: true,
        xoauth2_accepts: GOOD,
        ..fake_imap::Script::default()
    }
}

async fn smtp_login(script: fake_smtp::Script, token: &str) -> (Result<(), NetError>, Vec<String>) {
    let cert = Cert::self_signed("localhost");
    let (port, transcript) = fake_smtp::serve(&cert, script).await;
    let result = async {
        let mut session = SmtpSession::connect(&pinned(port, &cert), &Roots::empty()).await?;
        session
            .login(&Credentials::bearer("ada@outlook.example", token))
            .await?;
        session.quit().await
    }
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let lines = transcript.lines().into_iter().map(|(_, l)| l).collect();
    (result, lines)
}

fn smtp_script() -> fake_smtp::Script {
    fake_smtp::Script {
        implicit: true,
        xoauth2_accepts: GOOD,
        ..fake_smtp::Script::default()
    }
}

#[tokio::test]
async fn imap_signs_in_with_the_token() {
    let (session, lines) = imap_open(imap_script(), "good-token").await;
    session.unwrap().logout().await.unwrap();
    assert!(
        lines.iter().any(|l| l.contains("AUTHENTICATE XOAUTH2")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|l| *l == base64_encode(GOOD.as_bytes())),
        "{lines:?}"
    );
    assert!(!sends_password(&lines), "{lines:?}");
}

#[tokio::test]
async fn imap_wrong_token_is_a_sign_in_error_in_both_failure_shapes() {
    for challenge in [false, true] {
        let script = fake_imap::Script {
            xoauth2_challenge: challenge,
            ..imap_script()
        };
        let (session, lines) = imap_open(script, "bad-token").await;
        let err = session.err().unwrap();
        assert!(matches!(err, NetError::SignIn(_)), "{challenge}: {err:?}");
        assert!(!sends_password(&lines), "{lines:?}");
    }
}

#[tokio::test]
async fn imap_without_xoauth2_is_refused_and_sends_no_token() {
    let script = fake_imap::Script {
        xoauth2_accepts: "",
        signed_out_capabilities: "AUTH=PLAIN",
        ..imap_script()
    };
    let (session, lines) = imap_open(script, "good-token").await;
    let err = session.err().unwrap();
    // Not SignIn: that would read as an expired token.
    assert!(matches!(err, NetError::Refused(_)), "{err:?}");
    assert!(
        !lines
            .iter()
            .any(|l| l.contains("good-token") || l.contains("AUTHENTICATE") || l.contains("LOGIN")),
        "{lines:?}"
    );
}

#[tokio::test]
async fn smtp_signs_in_with_the_token() {
    let (result, lines) = smtp_login(smtp_script(), "good-token").await;
    result.unwrap();
    let token = base64_encode(GOOD.as_bytes());
    assert!(
        lines.iter().any(|l| *l == format!("AUTH XOAUTH2 {token}")),
        "{lines:?}"
    );
    assert!(!sends_password(&lines), "{lines:?}");
}

#[tokio::test]
async fn smtp_wrong_token_is_a_sign_in_error_in_both_failure_shapes() {
    for challenge in [false, true] {
        let script = fake_smtp::Script {
            xoauth2_challenge: challenge,
            ..smtp_script()
        };
        let (result, lines) = smtp_login(script, "bad-token").await;
        let err = result.unwrap_err();
        assert!(matches!(err, NetError::SignIn(_)), "{challenge}: {err:?}");
        assert!(!sends_password(&lines), "{lines:?}");
        if challenge {
            // lettre never answers a 334 error challenge for XOAUTH2: it
            // fails on the client side ("does not expect a challenge",
            // mapped by `client_error`), so the 535 is never read.
            assert!(
                lines.last().unwrap().starts_with("AUTH XOAUTH2 "),
                "{lines:?}"
            );
        }
    }
}

#[tokio::test]
async fn smtp_without_xoauth2_is_refused_and_sends_no_token() {
    let script = fake_smtp::Script {
        xoauth2_accepts: "",
        ..smtp_script()
    };
    let (result, lines) = smtp_login(script, "good-token").await;
    let err = result.unwrap_err();
    assert!(matches!(err, NetError::Refused(_)), "{err:?}");
    assert!(!lines.iter().any(|l| l.starts_with("AUTH ")), "{lines:?}");
}
