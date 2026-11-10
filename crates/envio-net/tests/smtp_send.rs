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

/// Sends `eml` and returns what the server received after DATA, up to
/// and including the end-of-data line.
async fn wire_after_data(eml: &[u8]) -> String {
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
    session
        .send("ada@example.org", &["bob@example.org".to_owned()], eml)
        .await
        .unwrap();
    let lines = transcript.lines();
    let at = lines
        .iter()
        .position(|(_, l)| l.starts_with("DATA"))
        .unwrap();
    // lettre ends the data with its own CRLF before the dot, hence the extra
    // empty line. The transcript trims line ends; lines are joined with "\n".
    lines[at + 1..]
        .iter()
        .map(|(_, l)| l.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn a_body_line_starting_with_a_dot_is_stuffed() {
    let wire = wire_after_data(b"Subject: x\r\n\r\n.hidden\r\nend\r\n").await;
    assert_eq!(wire, "Subject: x\n\n..hidden\nend\n\n.");
}

#[tokio::test]
async fn a_bare_lf_dot_in_the_body_does_not_end_the_data() {
    let wire = wire_after_data(b"Subject: x\r\n\r\nbefore\n.\r\nQUIT\r\nafter\r\n").await;
    assert_eq!(wire, "Subject: x\n\nbefore\n..\nQUIT\nafter\n\n.");
}

#[tokio::test]
async fn a_body_over_one_chunk_arrives_intact() {
    let mut eml = b"Subject: big\r\n\r\n".to_vec();
    let line = format!("{}\r\n", "x".repeat(78));
    while eml.len() < 200 * 1024 {
        eml.extend_from_slice(line.as_bytes());
    }
    let wire = wire_after_data(&eml).await;
    let expected = format!(
        "{}\n.",
        String::from_utf8(eml).unwrap().replace("\r\n", "\n")
    );
    assert!(
        wire == expected,
        "lengths {} vs {}",
        wire.len(),
        expected.len()
    );
}

#[tokio::test]
async fn an_address_with_a_line_break_is_refused_before_anything_is_sent() {
    for (from, to) in [
        ("ada@example.org\r\nRCPT TO:<x@y.z>", "bob@example.org"),
        ("ada@example.org", "bob@example.org\nRCPT TO:<x@y.z>"),
        ("ada@example.org", "bob@example.org\r"),
    ] {
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
        let err = session
            .send(from, &[to.to_owned()], MAIL)
            .await
            .unwrap_err();
        assert!(!err.unconfirmed, "{err:?}");
        let lines = transcript.lines();
        assert!(
            !lines
                .iter()
                .any(|(_, l)| l.starts_with("MAIL") || l.starts_with("RCPT")),
            "{lines:?}"
        );
    }
}
