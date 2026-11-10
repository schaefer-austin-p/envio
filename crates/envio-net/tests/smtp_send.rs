//! Whether a failed send may still have delivered the message, against a
//! scripted in-process server. The Outbox resends only what is confirmed
//! unsent.

mod support;

use envio_net::smtp::{SendError, SmtpSession};
use envio_net::{Credentials, Endpoint, Fingerprint, NetError, Roots, Security};
use support::certs::Cert;
use support::fake_smtp::{Script, serve};

const MAIL: &[u8] =
    b"From: ada@example.org\r\nTo: bob@example.org\r\nSubject: hi\r\n\r\nHello.\r\n";

async fn send_with(script: Script) -> Result<(), SendError> {
    let cert = Cert::self_signed("localhost");
    let script = Script {
        implicit: true,
        ..script
    };
    let (port, _) = serve(&cert, script).await;
    let endpoint = Endpoint {
        pin: Some(Fingerprint::of(&cert.der)),
        ..Endpoint::new("localhost", port, Security::Tls)
    };
    let mut session = SmtpSession::connect(&endpoint, &Roots::empty())
        .await
        .unwrap();
    session
        .login(&Credentials::new("ada", "secret-pw"))
        .await
        .unwrap();
    session
        .send("ada@example.org", &["bob@example.org".to_owned()], MAIL)
        .await
}

#[tokio::test]
async fn a_cut_after_the_final_dot_may_have_sent_the_message() {
    let err = send_with(Script {
        data_reply: None,
        ..Script::default()
    })
    .await
    .unwrap_err();
    assert!(err.unconfirmed, "{err:?}");
    assert!(err.error.is_temporary(), "{err:?}");
}

#[tokio::test]
async fn a_cut_before_the_message_sent_nothing() {
    let err = send_with(Script {
        mail_reply: None,
        ..Script::default()
    })
    .await
    .unwrap_err();
    assert!(!err.unconfirmed, "{err:?}");
    assert!(matches!(err.error, NetError::Io(_)), "{err:?}");
}

#[tokio::test]
async fn a_refusal_after_the_message_is_confirmed_unsent() {
    let err = send_with(Script {
        data_reply: Some("554 5.7.1 spam"),
        ..Script::default()
    })
    .await
    .unwrap_err();
    assert!(!err.unconfirmed, "{err:?}");
    assert!(matches!(err.error, NetError::Refused(_)), "{err:?}");
}

#[tokio::test]
async fn a_busy_recipient_is_confirmed_unsent_and_temporary() {
    let err = send_with(Script {
        rcpt_reply: "451 4.3.0 try later",
        ..Script::default()
    })
    .await
    .unwrap_err();
    assert!(!err.unconfirmed, "{err:?}");
    assert!(err.error.is_temporary(), "{err:?}");
}

#[tokio::test]
async fn non_ascii_mail_goes_out_with_8bitmime() {
    let cert = Cert::self_signed("localhost");
    let script = Script {
        implicit: true,
        ..Script::default()
    };
    let (port, transcript) = serve(&cert, script).await;
    let endpoint = Endpoint {
        pin: Some(Fingerprint::of(&cert.der)),
        ..Endpoint::new("localhost", port, Security::Tls)
    };
    let mut session = SmtpSession::connect(&endpoint, &Roots::empty())
        .await
        .unwrap();
    session
        .login(&Credentials::new("ada", "secret-pw"))
        .await
        .unwrap();
    let eml = "Subject: Grüße\r\n\r\nHallo.\r\n".as_bytes();
    session
        .send("ada@example.org", &["bob@example.org".to_owned()], eml)
        .await
        .unwrap();
    let lines = transcript.lines();
    assert!(
        lines
            .iter()
            .any(|(_, l)| l == "MAIL FROM:<ada@example.org> BODY=8BITMIME"),
        "{lines:?}"
    );
}
