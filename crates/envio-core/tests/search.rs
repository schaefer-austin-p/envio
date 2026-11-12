//! Local search over headers and bodies, and Search on server.

mod support;

use std::time::{Duration, Instant};

use envio_core::api::view::MessageRow;
use envio_core::{Command, Event, FolderId, FolderRole, SearchScope};
use support::Harness;
use support::fake::{Kind, TlsServer, serve};

fn mail(subject: &str, body: &str) -> Vec<u8> {
    format!(
        "From: Ada <ada@example.org>\r\nSubject: {subject}\r\nMIME-Version: 1.0\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\r\n{body}\r\n"
    )
    .into_bytes()
}

fn local(h: &Harness, scope: SearchScope, query: &str) -> Vec<MessageRow> {
    h.send(Command::Search {
        scope: scope.clone(),
        query: query.into(),
    });
    h.wait(|e| match e {
        Event::SearchResults {
            scope: s,
            query: q,
            server: false,
            rows,
        } if *s == scope && q == query => Some(rows.clone()),
        _ => None,
    })
}

#[test]
fn local_search_finds_body_text() {
    let mut h = Harness::unstarted();
    let key = h.seed_account("me@example.org", Some("pw"));
    h.seed_folder_with(
        &key,
        "INBOX",
        FolderRole::Inbox,
        &[
            (1, &mail("plain subject", "the zebra crossed the road")),
            (2, &mail("another subject", "nothing here")),
        ],
    );
    h.start_core();
    let rows = local(&h, SearchScope::All, "zebra");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].subject, "message 1");
    assert_eq!(rows[0].account, key);
    // A word of the subject still matches.
    assert_eq!(local(&h, SearchScope::All, "message").len(), 2);
    assert!(local(&h, SearchScope::All, "giraffe").is_empty());
}

#[test]
fn scope_folder_limits_results() {
    let mut h = Harness::unstarted();
    let one = h.seed_account("one@example.org", Some("pw"));
    let two = h.seed_account("two@example.org", Some("pw"));
    let body = mail("s", "zebra");
    let inbox = h.seed_folder_with(&one, "INBOX", FolderRole::Inbox, &[(1, &body)]);
    let archive = h.seed_folder_with(&one, "Archive", FolderRole::Archive, &[(1, &body)]);
    h.seed_folder_with(&two, "INBOX", FolderRole::Inbox, &[(1, &body)]);
    h.start_core();
    let folders =
        |rows: &[MessageRow]| -> Vec<FolderId> { rows.iter().map(|r| r.folder).collect() };
    assert_eq!(local(&h, SearchScope::All, "zebra").len(), 3);
    let in_one = local(&h, SearchScope::Account(one.clone()), "zebra");
    assert_eq!(in_one.len(), 2);
    assert!(in_one.iter().all(|r| r.account == one));
    assert_eq!(
        folders(&local(&h, SearchScope::Folder(archive), "zebra")),
        [archive]
    );
    assert_eq!(
        folders(&local(&h, SearchScope::Folder(inbox), "zebra")),
        [inbox]
    );
    assert!(local(&h, SearchScope::Account(two), "zebra").len() == 1);
}

/// The search lines a server was sent, with the folder selected before each.
fn searches(server: &TlsServer) -> Vec<(String, String)> {
    let log = server.log.lock().unwrap();
    let mut selected = String::new();
    let mut found = Vec::new();
    for line in log.iter() {
        let command = line.split_once(' ').map_or("", |(_, rest)| rest.trim_end());
        if let Some(rest) = command.strip_prefix("SELECT ") {
            selected = rest.trim_matches('"').to_owned();
        } else if command.starts_with("UID SEARCH") {
            found.push((selected.clone(), command.to_owned()));
        }
    }
    found
}

/// Waits (up to 10 s) for the answer to a server search of `scope`;
/// returns the failures that came before it and the rows.
fn server_answer(h: &Harness, scope: &SearchScope) -> (Vec<String>, Vec<MessageRow>) {
    let end = Instant::now() + Duration::from_secs(10);
    let mut failures = Vec::new();
    loop {
        let left = end.saturating_duration_since(Instant::now());
        match h.events.recv_timeout(left).expect("no server answer") {
            Event::SearchFailed { scope: s, text, .. } if s == *scope => failures.push(text),
            Event::SearchResults {
                scope: s,
                server: true,
                rows,
                ..
            } if s == *scope => return (failures, rows),
            _ => {}
        }
    }
}

#[test]
fn server_search_of_all_searches_each_inbox() {
    let (a, b) = (serve(Kind::Mailbox), serve(Kind::Mailbox));
    let mut h = Harness::unstarted();
    h.foreground = true;
    let one = h.seed_account_at("one@example.org", Some("pw"), a.port);
    let two = h.seed_account_at("two@example.org", Some("pw"), b.port);
    h.pin_imap(&one, &a.hex);
    h.pin_imap(&two, &b.hex);
    let body = mail("s", "zebra");
    h.seed_folder_with(&one, "INBOX", FolderRole::Inbox, &[(1, &body)]);
    h.seed_folder_with(&one, "Archive", FolderRole::Archive, &[(1, &body)]);
    h.seed_folder_with(&two, "INBOX", FolderRole::Inbox, &[(1, &body)]);
    h.start_core();
    h.send(Command::SearchServer {
        scope: SearchScope::All,
        query: "zebra".into(),
    });
    let (failures, _) = server_answer(&h, &SearchScope::All);
    assert!(failures.is_empty(), "{failures:?}");
    for server in [&a, &b] {
        let first = searches(server).into_iter().next().expect("never searched");
        assert_eq!(first.0, "INBOX");
        assert_eq!(first.1, "UID SEARCH TEXT \"zebra\"");
    }
    // Neither server was asked about the Archive.
    assert!(searches(&a).iter().all(|(folder, _)| folder != "Archive"));
}

#[test]
fn an_offline_account_reports_search_failed() {
    let good = serve(Kind::Mailbox);
    let mut h = Harness::unstarted();
    h.foreground = true;
    let up = h.seed_account_at("up@example.org", Some("pw"), good.port);
    h.pin_imap(&up, &good.hex);
    // Nothing listens on port 1.
    let down = h.seed_account("down@example.org", Some("pw"));
    let body = mail("s", "zebra");
    h.seed_folder_with(&up, "INBOX", FolderRole::Inbox, &[(1, &body)]);
    h.seed_folder_with(&down, "INBOX", FolderRole::Inbox, &[(1, &body)]);
    h.start_core();
    h.send(Command::SearchServer {
        scope: SearchScope::All,
        query: "zebra".into(),
    });
    let (failures, _) = server_answer(&h, &SearchScope::All);
    assert_eq!(failures.len(), 1, "{failures:?}");
    // The reachable account was asked.
    assert_eq!(searches(&good).len(), 1);
}

#[test]
fn a_broken_account_is_never_connected_for_a_search() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut h = Harness::unstarted();
    h.foreground = true;
    let broken = h.seed_account_at("broken@example.org", Some("pw"), port);
    h.pin_imap(&broken, "not-a-pin");
    // With an Inbox to search, only the broken-account guard stops the
    // search from being queued (and so left unanswered).
    h.seed_folder_with(
        &broken,
        "INBOX",
        FolderRole::Inbox,
        &[(1, &mail("s", "zebra"))],
    );
    h.start_core();
    let scope = SearchScope::Account(broken);
    h.send(Command::SearchServer {
        scope: scope.clone(),
        query: "zebra".into(),
    });
    let end = Instant::now() + Duration::from_secs(5);
    let failed = loop {
        let left = end.saturating_duration_since(Instant::now());
        match h.events.recv_timeout(left).expect("no SearchFailed") {
            Event::SearchFailed { scope: s, text, .. } if s == scope => break text,
            Event::SearchResults { server: true, .. } => panic!("a broken account answered"),
            _ => {}
        }
    };
    assert!(!failed.is_empty());
    assert!(
        !failed.contains("can't be searched"),
        "failed for lack of an inbox, not as a broken account: {failed}"
    );
    assert!(listener.accept().is_err(), "connected a broken account");
}
