//! A tagged NO or BAD on a FETCH (or UID EXPUNGE) is an error, never an
//! empty or short result: async-imap's streams drop the tagged reply.
//! The scripted server refuses the nth command; the call must say so.

mod support;

use envio_net::imap::ImapSession;
use envio_net::{Credentials, Endpoint, Fingerprint, NetError, Roots, Security};
use support::certs::Cert;
use support::fake_imap::{Script, serve};

async fn session(script: Script) -> ImapSession {
    let cert = Cert::self_signed("localhost");
    let (port, _) = serve(
        &cert,
        Script {
            implicit: true,
            fetch_flags: true,
            ..script
        },
    )
    .await;
    let endpoint = Endpoint {
        pin: Some(Fingerprint::of(&cert.der)),
        ..Endpoint::new("localhost", port, Security::Tls)
    };
    ImapSession::open(&endpoint, &Roots::empty(), &Credentials::new("a", "b"))
        .await
        .unwrap()
}

/// The three refusals and whether each must be transient.
const REFUSALS: [(&str, bool); 3] = [
    ("NO [OVERQUOTA] nope", false),
    ("BAD bad command", false),
    ("NO [UNAVAILABLE] try later", true),
];

fn check(refusal: &str, busy: bool, error: &NetError) {
    assert_eq!(error.is_temporary(), busy, "{refusal}: {error:?}");
    let expected = if refusal.starts_with("BAD") {
        matches!(error, NetError::Protocol(_))
    } else {
        matches!(error, NetError::Refused(_) | NetError::Busy(_))
    };
    assert!(expected, "{refusal}: {error:?}");
}

/// A reply as a `'static` script line.
fn leak(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

fn leak_all(lines: Vec<&'static str>) -> &'static [&'static str] {
    Box::leak(lines.into_boxed_slice())
}

const STRUCTURE: &str = "* 1 FETCH (UID 7 BODYSTRUCTURE (\"TEXT\" \"PLAIN\" (\"CHARSET\" \"utf-8\") NIL NIL \"7BIT\" 5 1))\r\n";
const HEADER: &str = "* 1 FETCH (UID 7 BODY[HEADER] {15}\r\nSubject: Hi\r\n\r\n)\r\n";

#[tokio::test]
async fn a_refused_message_id_scan_is_an_error_not_not_found() {
    // The Outbox sends again on "not found".
    for (refusal, busy) in REFUSALS {
        let mut s = session(Script {
            select_exists: 5,
            search_untagged: "* SEARCH\r\n",
            fetch_refused_at: 1,
            fetch_refusal: refusal,
            ..Script::default()
        })
        .await;
        s.select("INBOX").await.unwrap();
        let result = s.search_message_id("<a@b>").await;
        check(refusal, busy, &result.unwrap_err());
    }
}

#[tokio::test]
async fn a_message_id_scan_still_finds_a_live_match() {
    let header = "Message-ID: <a@b>\r\n\r\n";
    let n = header.len();
    let reply = leak(format!(
        "* 5 FETCH (UID 9 FLAGS (\\Seen) BODY[HEADER.FIELDS (MESSAGE-ID)] {{{n}}}\r\n{header})\r\n\
         * 4 FETCH (UID 8 FLAGS (\\Deleted) BODY[HEADER.FIELDS (MESSAGE-ID)] {{{n}}}\r\n{header})\r\n"
    ));
    let mut s = session(Script {
        select_exists: 5,
        search_untagged: "* SEARCH\r\n",
        fetch_canned: leak_all(vec![reply]),
        ..Script::default()
    })
    .await;
    s.select("INBOX").await.unwrap();
    assert_eq!(s.search_message_id("<a@b>").await.unwrap(), vec![9]);
}

#[tokio::test]
async fn refused_summaries_are_an_error_not_an_empty_list() {
    for (refusal, busy) in REFUSALS {
        let mut s = session(Script {
            fetch_refused_at: 1,
            fetch_refusal: refusal,
            ..Script::default()
        })
        .await;
        let result = s.fetch_summaries(&[3]).await;
        check(refusal, busy, &result.unwrap_err());
    }
}

#[tokio::test]
async fn summaries_still_parse() {
    let header = "Subject: Hi\r\n\r\n";
    let reply = leak(format!(
        "* 1 FETCH (UID 3 FLAGS (\\Seen $x) RFC822.SIZE 120 \
         INTERNALDATE \"08-Nov-2026 10:00:00 +0000\" BODY[HEADER] {{{}}}\r\n{header})\r\n",
        header.len()
    ));
    let mut s = session(Script {
        fetch_canned: leak_all(vec![reply]),
        ..Script::default()
    })
    .await;
    let got = s.fetch_summaries(&[3]).await.unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].uid, 3);
    assert_eq!(got[0].size, 120);
    assert_eq!(got[0].internal_date, Some(1_794_132_000));
    assert!(got[0].flags.seen);
    assert_eq!(got[0].header, header.as_bytes());
}

#[tokio::test]
async fn a_refused_body_fetch_is_an_error_not_gone() {
    for (refusal, busy) in REFUSALS {
        let mut s = session(Script {
            fetch_refused_at: 1,
            fetch_refusal: refusal,
            ..Script::default()
        })
        .await;
        let result = s.fetch_body(7, 10).await;
        check(refusal, busy, &result.unwrap_err());
    }
}

#[tokio::test]
async fn a_refused_section_fetch_is_an_error_not_gone() {
    for (refusal, busy) in REFUSALS {
        let mut s = session(Script {
            fetch_refused_at: 1,
            fetch_refusal: refusal,
            ..Script::default()
        })
        .await;
        let result = s.fetch_section(7, "2", 10).await;
        check(refusal, busy, &result.unwrap_err());
    }
}

#[tokio::test]
async fn a_section_fetch_still_joins_mime_and_body() {
    let reply = "* 1 FETCH (UID 7 BODY[2.MIME] {4}\r\nA: b BODY[2] {2}\r\nhi)\r\n";
    let mut s = session(Script {
        fetch_canned: leak_all(vec![reply]),
        ..Script::default()
    })
    .await;
    let got = s.fetch_section(7, "2", 10).await.unwrap().unwrap();
    assert_eq!(got, b"A: bhi");
}

/// `fetch_partial` asks for the structure, the header, then the parts:
/// the nth of these is refused.
async fn partial_refused(nth: usize, refusal: &'static str) -> NetError {
    let canned: &'static [&'static str] = match nth {
        1 => &[],
        2 => &[STRUCTURE],
        _ => &[STRUCTURE, HEADER],
    };
    let mut s = session(Script {
        fetch_canned: canned,
        fetch_refused_at: nth,
        fetch_refusal: refusal,
        ..Script::default()
    })
    .await;
    s.fetch_partial(7).await.unwrap_err()
}

#[tokio::test]
async fn a_refused_partial_fetch_is_an_error_at_every_step() {
    for nth in 1..=3 {
        for (refusal, busy) in REFUSALS {
            check(refusal, busy, &partial_refused(nth, refusal).await);
        }
    }
}

#[tokio::test]
async fn a_refused_uid_expunge_is_an_error() {
    for (refusal, busy) in REFUSALS {
        let mut s = session(Script {
            expunge_refusal: refusal,
            ..Script::default()
        })
        .await;
        let result = s.delete(&[4]).await;
        check(refusal, busy, &result.unwrap_err());
    }
}
