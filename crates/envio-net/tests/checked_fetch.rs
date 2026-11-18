//! A tagged NO or BAD on a FETCH (or UID EXPUNGE) is an error, never an
//! empty or short result: async-imap's streams drop the tagged reply.
//! The scripted server refuses the nth command; the call must say so.

mod support;

use envio_net::imap::{Flag, ImapSession};
use envio_net::{Credentials, Endpoint, Fingerprint, NetError, Roots, Security};
use support::certs::Cert;
use support::fake_imap::{Script, Transcript, serve};

async fn session(script: Script) -> ImapSession {
    session_logged(script).await.0
}

async fn session_logged(script: Script) -> (ImapSession, Transcript) {
    let cert = Cert::self_signed("localhost");
    let (port, log) = serve(
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
    let session = ImapSession::open(&endpoint, &Roots::empty(), &Credentials::new("a", "b"))
        .await
        .unwrap();
    (session, log)
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
            search_all_untagged: "* SEARCH 8 9\r\n",
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
        search_all_untagged: "* SEARCH 8 9\r\n",
        fetch_canned: leak_all(vec![reply]),
        ..Script::default()
    })
    .await;
    s.select("INBOX").await.unwrap();
    assert_eq!(s.search_message_id("<a@b>").await.unwrap(), vec![9]);
}

#[tokio::test]
async fn a_deep_message_id_scan_reaches_a_copy_outside_the_newest_200() {
    let header = "Message-ID: <a@b>\r\n\r\n";
    let n = header.len();
    // 450 messages: the scans cover 251-450, 51-250, then 1-50, where the
    // copy is.
    let reply = leak(format!(
        "* 3 FETCH (UID 3 FLAGS (\\Seen) BODY[HEADER.FIELDS (MESSAGE-ID)] {{{n}}}\r\n{header})\r\n"
    ));
    let all = leak(format!(
        "* SEARCH {}\r\n",
        (1..=450)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    ));
    let script = || Script {
        select_exists: 450,
        search_untagged: "* SEARCH\r\n",
        search_all_untagged: all,
        fetch_canned: leak_all(vec!["", "", reply]),
        ..Script::default()
    };
    let mut s = session(script()).await;
    s.select("Drafts").await.unwrap();
    assert!(s.search_message_id("<a@b>").await.unwrap().is_empty());
    let mut s = session(script()).await;
    s.select("Drafts").await.unwrap();
    assert_eq!(s.search_message_id_deep("<a@b>").await.unwrap(), vec![3]);
}

#[tokio::test]
async fn a_scan_names_the_folders_uids_not_numbers_counted_from_select() {
    let header = "Message-ID: <a@b>\r\n\r\n";
    let reply = leak(format!(
        "* 2 FETCH (UID 4 FLAGS (\\Seen) BODY[HEADER.FIELDS (MESSAGE-ID)] {{{}}}\r\n{header})\r\n",
        header.len()
    ));
    let (mut s, log) = session_logged(Script {
        select_exists: 450,
        search_untagged: "* SEARCH\r\n",
        // After the expunge and the append the folder holds three.
        search_all_untagged: "* SEARCH 2 3 4\r\n",
        expunge_untagged: "* 1 EXPUNGE\r\n",
        before_append_ok: "* 451 EXISTS\r\n",
        fetch_canned: leak_all(vec![reply]),
        ..Script::default()
    })
    .await;
    s.select("Drafts").await.unwrap();
    s.delete(&[1]).await.unwrap();
    s.append("Drafts", &[Flag::Draft], b"Subject: x\r\n\r\nx")
        .await
        .unwrap();
    assert_eq!(s.search_message_id_deep("<a@b>").await.unwrap(), vec![4]);
    let fetches: Vec<_> = log
        .lines()
        .into_iter()
        .filter(|(_, line)| line.contains("FETCH"))
        .collect();
    assert_eq!(fetches.len(), 1, "{fetches:?}");
    assert!(fetches[0].1.contains("UID FETCH 2:4 "), "{fetches:?}");
}

#[tokio::test]
async fn a_miss_is_trusted_once_the_server_has_found_a_message_by_header() {
    let (mut s, log) = session_logged(Script {
        select_exists: 5,
        search_untagged: "* SEARCH 3\r\n",
        search_later_untagged: "* SEARCH\r\n",
        search_all_untagged: "* SEARCH 1 2 3\r\n",
        ..Script::default()
    })
    .await;
    s.select("Drafts").await.unwrap();
    assert_eq!(s.search_message_id_deep("<a@b>").await.unwrap(), vec![3]);
    assert!(s.search_message_id_deep("<c@d>").await.unwrap().is_empty());
    assert!(!log.lines().iter().any(|(_, line)| line.contains("FETCH")));
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

#[tokio::test]
async fn a_refused_uid_store_is_an_error() {
    // A read-only mailbox refuses \Deleted; the delete must not go on.
    for (refusal, busy) in REFUSALS {
        let script = Script {
            store_refusal: refusal,
            ..Script::default()
        };
        let mut s = session(script).await;
        let result = s.store_flags(&[4], true, &[Flag::Seen]).await;
        check(refusal, busy, &result.unwrap_err());
        let mut s = session(script).await;
        check(refusal, busy, &s.delete(&[4]).await.unwrap_err());
    }
}

/// What `take_changes` says after a UID EXPUNGE whose reply carried `lines`.
async fn changed_by_expunge(lines: &'static str) -> bool {
    let mut s = session(Script {
        expunge_untagged: lines,
        ..Script::default()
    })
    .await;
    assert!(!s.take_changes());
    s.delete(&[4]).await.unwrap();
    s.take_changes()
}

#[tokio::test]
async fn changes_seen_during_a_uid_expunge_are_reported() {
    assert!(changed_by_expunge("* 9 EXISTS\r\n").await);
    assert!(changed_by_expunge("* 3 FETCH (UID 3 FLAGS (\\Seen))\r\n").await);
    assert!(changed_by_expunge("* VANISHED 5\r\n").await);
    // The EXPUNGE lines are the command's own result.
    assert!(!changed_by_expunge("* 4 EXPUNGE\r\n").await);
}

const SUMMARY: &str = "* 1 FETCH (UID 3 FLAGS (\\Seen) RFC822.SIZE 9 BODY[HEADER] {2}\r\n\r\n)\r\n";
const BODY: &str = "* 1 FETCH (UID 7 BODY[] {2}\r\nhi)\r\n";

/// What `take_changes` says after a FETCH (summaries, or a body) whose
/// reply had `unsolicited` lines ahead of the normal one.
async fn changed_by_fetch(unsolicited: &str, summary: bool) -> bool {
    let normal = if summary { SUMMARY } else { BODY };
    let mut s = session(Script {
        fetch_canned: leak_all(vec![leak(format!("{unsolicited}{normal}"))]),
        ..Script::default()
    })
    .await;
    assert!(!s.take_changes());
    if summary {
        assert_eq!(s.fetch_summaries(&[3]).await.unwrap().len(), 1);
    } else {
        assert!(s.fetch_body(7, 2).await.unwrap().is_some());
    }
    s.take_changes()
}

#[tokio::test]
async fn changes_seen_during_a_fetch_are_reported() {
    for summary in [true, false] {
        for line in [
            "* 9 EXISTS\r\n",
            "* 2 EXPUNGE\r\n",
            "* VANISHED 5\r\n",
            // No UID: an unsolicited flag change.
            "* 5 FETCH (FLAGS (\\Seen))\r\n",
        ] {
            assert!(changed_by_fetch(line, summary).await, "{line:?} {summary}");
        }
        // Nothing unsolicited, nothing changed.
        assert!(!changed_by_fetch("", summary).await, "{summary}");
    }
}
