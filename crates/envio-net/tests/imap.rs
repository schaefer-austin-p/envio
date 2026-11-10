//! Opening an IMAP session against a scripted server: what goes over the
//! wire in the clear, and what each refusal becomes.

mod support;

use envio_net::imap::ImapSession;
use envio_net::{Credentials, Endpoint, Fingerprint, NetError, Roots, Security};
use support::certs::Cert;
use support::fake_imap::{Script, serve};

async fn open(
    cert: &Cert,
    script: Script,
) -> (Result<ImapSession, NetError>, Vec<(bool, String)>, u16) {
    let (port, transcript) = serve(cert, script).await;
    let security = if script.implicit {
        Security::Tls
    } else {
        Security::StartTls
    };
    let endpoint = Endpoint {
        pin: Some(Fingerprint::of(&cert.der)),
        ..Endpoint::new("localhost", port, security)
    };
    let credentials = Credentials::new("alice", "s3cret");
    let result = ImapSession::open(&endpoint, &Roots::empty(), &credentials).await;
    // Let the server record whatever came after the error.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    (result, transcript.lines(), port)
}

fn clear_text(lines: &[(bool, String)]) -> Vec<String> {
    lines
        .iter()
        .filter(|(tls, _)| !tls)
        .map(|(_, line)| line.split(' ').skip(1).collect::<Vec<_>>().join(" "))
        .collect()
}

#[tokio::test]
async fn starttls_then_login_and_only_starttls_goes_in_the_clear() {
    let cert = Cert::self_signed("localhost");
    let (session, lines, _) = open(&cert, Script::default()).await;
    let session = session.unwrap();
    assert!(session.caps().idle && session.caps().uidplus);
    assert_eq!(clear_text(&lines), ["STARTTLS"]);
    assert!(
        lines
            .iter()
            .any(|(tls, line)| *tls && line.contains("LOGIN"))
    );
    session.logout().await.unwrap();
}

#[tokio::test]
async fn a_refused_starttls_never_sends_the_password() {
    for refusal in ["NO", "BAD"] {
        let cert = Cert::self_signed("localhost");
        let script = Script {
            starttls: refusal,
            ..Script::default()
        };
        let (session, lines, _) = open(&cert, script).await;
        assert!(matches!(session, Err(NetError::NoStartTls)), "{refusal}");
        assert!(
            lines.iter().all(|(_, line)| !line.contains("LOGIN")),
            "{lines:?}"
        );
    }
}

#[tokio::test]
async fn a_preauth_greeting_on_the_plain_port_is_refused() {
    let cert = Cert::self_signed("localhost");
    let script = Script {
        greeting: "* PREAUTH welcome back",
        ..Script::default()
    };
    let (session, lines, _) = open(&cert, script).await;
    assert!(matches!(session, Err(NetError::NoStartTls)));
    assert!(lines.is_empty(), "{lines:?}");
}

#[tokio::test]
async fn bytes_injected_after_the_starttls_ok_are_never_read() {
    let cert = Cert::self_signed("localhost");
    let script = Script {
        inject: "* CAPABILITY IMAP4rev1 MOVE\r\n",
        ..Script::default()
    };
    let (session, _, _) = open(&cert, script).await;
    // Dropped with the plaintext buffer, or the handshake fails on them;
    // never a MOVE capability taken from the clear.
    match session {
        Ok(session) => assert!(!session.caps().r#move),
        Err(err) => assert!(matches!(err, NetError::Tls(_) | NetError::Io(_)), "{err:?}"),
    }
}

#[tokio::test]
async fn a_wrong_password_is_a_sign_in_error() {
    let cert = Cert::self_signed("localhost");
    let script = Script {
        implicit: true,
        login: "NO [AUTHENTICATIONFAILED] invalid credentials",
        ..Script::default()
    };
    let (session, _, _) = open(&cert, script).await;
    let err = session.err().unwrap();
    assert!(matches!(err, NetError::SignIn(_)), "{err:?}");
    assert!(!err.is_temporary());
}

#[tokio::test]
async fn a_bye_greeting_is_busy() {
    let cert = Cert::self_signed("localhost");
    let script = Script {
        implicit: true,
        greeting: "* BYE too many connections",
        ..Script::default()
    };
    let (session, _, _) = open(&cert, script).await;
    let err = session.err().unwrap();
    assert!(matches!(err, NetError::Busy(_)), "{err:?}");
    assert!(err.is_temporary());
}

#[tokio::test]
async fn an_unpinned_self_signed_server_goes_to_the_prompt() {
    let cert = Cert::self_signed("localhost");
    let (port, transcript) = serve(
        &cert,
        Script {
            implicit: true,
            ..Script::default()
        },
    )
    .await;
    let endpoint = Endpoint::new("localhost", port, Security::Tls);
    let credentials = Credentials::new("alice", "s3cret");
    let err = ImapSession::open(&endpoint, &Roots::empty(), &credentials)
        .await
        .err()
        .unwrap();
    assert!(matches!(err, NetError::Untrusted { .. }), "{err:?}");
    assert!(transcript.lines().is_empty());
}
