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
    open_as(cert, script, Credentials::new("alice", "s3cret")).await
}

async fn open_as(
    cert: &Cert,
    script: Script,
    credentials: Credentials,
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
    // The server writes the OK and the injected line in one write, which
    // reaches the client as one segment on loopback. The client drops the
    // plaintext buffer, so the session opens and its capabilities come
    // from the TLS side only: no MOVE from the clear.
    let session = session.unwrap();
    let caps = session.caps();
    assert!(caps.idle && caps.uidplus, "{caps:?}");
    assert!(!caps.r#move, "{caps:?}");
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
async fn a_temporary_login_refusal_is_busy_not_a_wrong_password() {
    let cert = Cert::self_signed("localhost");
    let script = Script {
        implicit: true,
        login: "NO [UNAVAILABLE] Temporary authentication failure",
        ..Script::default()
    };
    let (session, _, _) = open(&cert, script).await;
    let err = session.err().unwrap();
    assert!(matches!(err, NetError::Busy(_)), "{err:?}");
    assert!(err.is_temporary());
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

/// Moves UID 5 to Trash on a server with `capabilities` and returns the
/// commands the client sent after sign-in, without tags.
async fn move_commands(capabilities: &'static str) -> Vec<String> {
    let cert = Cert::self_signed("localhost");
    let script = Script {
        implicit: true,
        capabilities,
        ..Script::default()
    };
    let (port, transcript) = serve(&cert, script).await;
    let endpoint = Endpoint {
        pin: Some(Fingerprint::of(&cert.der)),
        ..Endpoint::new("localhost", port, Security::Tls)
    };
    let credentials = Credentials::new("alice", "s3cret");
    let mut session = ImapSession::open(&endpoint, &Roots::empty(), &credentials)
        .await
        .unwrap();
    assert!(!session.caps().r#move, "{capabilities}");
    session.move_to(&[5], "Trash").await.unwrap();
    session.logout().await.unwrap();
    // Let the server record the LOGOUT.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    transcript
        .lines()
        .into_iter()
        .filter(|(tls, _)| *tls)
        .filter_map(|(_, line)| line.split_once(' ').map(|(_, command)| command.to_owned()))
        .filter(|command| !command.starts_with("LOGIN") && !command.starts_with("CAPABILITY"))
        .collect()
}

#[tokio::test]
async fn a_move_without_move_copies_marks_and_expunges_by_uid() {
    // UIDPLUS: only the moved UID is expunged, never a bare EXPUNGE that
    // would also take other messages already marked \Deleted.
    let commands = move_commands("IDLE UIDPLUS").await;
    assert_eq!(
        commands,
        [
            r#"UID COPY 5 "Trash""#,
            r"UID STORE 5 +FLAGS.SILENT (\Deleted)",
            "UID EXPUNGE 5",
            "LOGOUT",
        ],
    );
}

#[tokio::test]
async fn a_move_without_move_or_uidplus_expunges_nothing() {
    // Without UIDPLUS there is no safe way to remove only the moved
    // message, so the original stays, marked \Deleted.
    let commands = move_commands("IDLE").await;
    assert_eq!(
        commands,
        [
            r#"UID COPY 5 "Trash""#,
            r"UID STORE 5 +FLAGS.SILENT (\Deleted)",
            "LOGOUT",
        ],
    );
    assert!(
        commands.iter().all(|c| !c.contains("EXPUNGE")),
        "{commands:?}"
    );
}

#[tokio::test]
async fn untagged_warnings_during_append_do_not_end_it() {
    for warning in ["* NO [ALERT] mailbox nearly full\r\n", "* BAD odd line\r\n"] {
        let cert = Cert::self_signed("localhost");
        let script = Script {
            implicit: true,
            before_append_ok: warning,
            ..Script::default()
        };
        let (session, _, _) = open(&cert, script).await;
        let mut session = session.unwrap();
        // The server stored the message: its tagged OK follows the warning.
        session
            .append("Sent", &[], b"Subject: hi\r\n\r\nbody\r\n")
            .await
            .unwrap_or_else(|e| panic!("{warning:?}: {e:?}"));
        session.logout().await.unwrap();
    }
}

const NON_ASCII: &str = "pässwort";

fn lines_of(lines: &[(bool, String)]) -> Vec<&str> {
    lines.iter().map(|(_, line)| line.as_str()).collect()
}

#[tokio::test]
async fn a_non_ascii_password_signs_in_with_authenticate_plain() {
    // Over implicit TLS and after STARTTLS alike.
    for implicit in [true, false] {
        let cert = Cert::self_signed("localhost");
        let script = Script {
            implicit,
            signed_out_capabilities: "AUTH=PLAIN",
            ..Script::default()
        };
        let (session, lines, _) =
            open_as(&cert, script, Credentials::new("alice", NON_ASCII)).await;
        session.unwrap().logout().await.unwrap();
        let seen = lines_of(&lines);
        assert!(
            seen.iter().any(|l| l.contains("AUTHENTICATE PLAIN")),
            "{seen:?}"
        );
        assert!(!seen.iter().any(|l| l.contains("LOGIN")), "{seen:?}");
        // Guards against the password ever going out unencoded.
        assert!(!seen.iter().any(|l| l.contains(NON_ASCII)), "{seen:?}");
    }
}

#[tokio::test]
async fn a_wrong_non_ascii_password_is_a_sign_in_error() {
    let cert = Cert::self_signed("localhost");
    let script = Script {
        implicit: true,
        signed_out_capabilities: "AUTH=PLAIN",
        ..Script::default()
    };
    let (session, _, _) = open_as(&cert, script, Credentials::new("alice", "påsswort")).await;
    let err = session.err().unwrap();
    assert!(matches!(err, NetError::SignIn(_)), "{err:?}");
}

#[tokio::test]
async fn a_non_ascii_password_without_auth_plain_is_refused_and_nothing_is_sent() {
    let cert = Cert::self_signed("localhost");
    let script = Script {
        implicit: true,
        ..Script::default()
    };
    let (session, lines, _) = open_as(&cert, script, Credentials::new("alice", NON_ASCII)).await;
    let err = session.err().unwrap();
    assert!(matches!(err, NetError::SignIn(_)), "{err:?}");
    assert!(!err.to_string().contains(NON_ASCII));
    let seen = lines_of(&lines);
    assert!(
        !seen
            .iter()
            .any(|l| l.contains("LOGIN") || l.contains("AUTHENTICATE")),
        "{seen:?}"
    );
}

/// async-imap restarts its IDLE timer on every response, so keepalives
/// must not stretch `wait`.
#[tokio::test]
async fn idle_returns_quiet_at_its_deadline_despite_keepalives() {
    use envio_net::imap::Woke;
    use std::time::Duration;
    let cert = Cert::self_signed("localhost");
    let script = Script {
        keepalive: true,
        ..Script::default()
    };
    let (session, _, _) = open(&cert, script).await;
    let session = session.unwrap();
    let idled = tokio::time::timeout(
        Duration::from_secs(2),
        session.idle(Duration::from_millis(300), std::future::pending()),
    )
    .await
    .expect("idle outlived its wait");
    let (session, woke) = idled.unwrap();
    assert_eq!(woke, Woke::Quiet);
    session.logout().await.unwrap();
}
