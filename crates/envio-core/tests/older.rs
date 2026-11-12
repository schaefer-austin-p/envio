//! "Load older": a batch of mail below the oldest cached message, and what
//! a later sync does with it.

mod support;

use std::time::{Duration, Instant};

use envio_core::{Command, Event, FolderId, FolderRole, ListRef, Settings};
use support::Harness;
use support::fake::{Kind, TlsServer, serve};

const EML: &[u8] = b"From: Ada <ada@example.org>\r\nSubject: s\r\n\r\nbody\r\n";

/// One account on a fake server holding UIDs 1 to 10, with only UID 10 cached.
fn seeded(h: &mut Harness, email: &str) -> (TlsServer, String, FolderId) {
    let server = serve(Kind::Older);
    let key = h.seed_account_at(email, Some("pw"), server.port);
    h.pin_imap(&key, &server.hex);
    let inbox = h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[(10, EML)]);
    (server, key, inbox)
}

fn lines(server: &TlsServer, needle: &str) -> usize {
    let log = server.log.lock().unwrap();
    log.iter().filter(|l| l.contains(needle)).count()
}

/// Waits (up to 10 s) until `server` was sent `count` lines with `needle`.
fn until_lines(server: &TlsServer, needle: &str, count: usize) {
    let end = Instant::now() + Duration::from_secs(10);
    while lines(server, needle) < count {
        assert!(Instant::now() < end, "never saw {count} x {needle}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn older_loaded(h: &Harness, list: ListRef) -> (usize, bool) {
    h.wait(|e| match e {
        Event::OlderLoaded {
            list: l,
            fetched,
            exhausted,
        } if *l == list => Some((*fetched, *exhausted)),
        _ => None,
    })
}

/// How many rows the cache lists for `list`; handled after any running step.
fn rows_of(h: &Harness, list: ListRef) -> usize {
    h.send(Command::ListMessages { list, after: None });
    h.wait(|e| match e {
        Event::Messages { list: l, rows, .. } if *l == list => Some(rows.len()),
        _ => None,
    })
}

#[test]
fn load_older_fetches_a_batch_and_reports_it() {
    let mut h = Harness::unstarted();
    h.foreground = true;
    let (server, _, inbox) = seeded(&mut h, "me@example.org");
    h.start_core();
    let list = ListRef::Folder(inbox);
    h.send(Command::LoadOlder { list });
    assert_eq!(older_loaded(&h, list), (9, true));
    assert!(lines(&server, "UID SEARCH UID 1:9") >= 1);
    h.wait(|e| match e {
        Event::MessagesChanged { folders } if folders.contains(&inbox) => Some(()),
        _ => None,
    });
    assert_eq!(rows_of(&h, list), 10);
}

#[test]
fn load_older_on_inboxes_asks_every_account() {
    let mut h = Harness::unstarted();
    h.foreground = true;
    let (a, _, _) = seeded(&mut h, "one@example.org");
    let (b, _, _) = seeded(&mut h, "two@example.org");
    h.start_core();
    h.send(Command::LoadOlder {
        list: ListRef::Inboxes,
    });
    // One event for both accounts: 9 each.
    assert_eq!(older_loaded(&h, ListRef::Inboxes), (18, true));
    for server in [&a, &b] {
        assert!(lines(server, "UID SEARCH UID 1:9") >= 1);
    }
    assert_eq!(rows_of(&h, ListRef::Inboxes), 20);
}

#[test]
fn a_sync_after_load_older_keeps_the_older_rows() {
    let mut h = Harness::unstarted();
    h.foreground = true;
    let (server, key, inbox) = seeded(&mut h, "me@example.org");
    h.start_core();
    let list = ListRef::Folder(inbox);
    h.send(Command::LoadOlder { list });
    older_loaded(&h, list);
    let before = lines(&server, "UID SEARCH SINCE");
    h.send(Command::Refresh { account: Some(key) });
    until_lines(&server, "UID SEARCH SINCE", before + 1);
    // The 2020 rows are outside the 30 day window but above the floor.
    assert_eq!(rows_of(&h, list), 10);
}

#[test]
fn changing_the_window_drops_the_floor_and_resyncs() {
    let mut h = Harness::unstarted();
    h.foreground = true;
    let (server, _, inbox) = seeded(&mut h, "me@example.org");
    h.start_core();
    let list = ListRef::Folder(inbox);
    h.send(Command::LoadOlder { list });
    older_loaded(&h, list);
    let before = lines(&server, "UID SEARCH SINCE");
    h.send(Command::SaveSettings(Settings {
        sync_window_days: 7,
        ..Settings::default()
    }));
    until_lines(&server, "UID SEARCH SINCE", before + 1);
    // Resynced without the floor: what Load older fetched is gone again.
    assert_eq!(rows_of(&h, list), 1);
}

#[test]
fn changing_the_window_announces_that_load_older_was_dropped() {
    let mut h = Harness::unstarted();
    h.foreground = true;
    let (_server, _, inbox) = seeded(&mut h, "me@example.org");
    h.start_core();
    let list = ListRef::Folder(inbox);
    h.send(Command::LoadOlder { list });
    older_loaded(&h, list);
    h.send(Command::SaveSettings(Settings {
        sync_window_days: 7,
        ..Settings::default()
    }));
    h.wait(|e| matches!(e, Event::OlderReset).then_some(()));
}

#[test]
fn load_older_offline_reports_older_unavailable() {
    let mut h = Harness::unstarted();
    h.foreground = true;
    // Nothing listens on port 1.
    let key = h.seed_account("me@example.org", Some("pw"));
    let inbox = h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[(10, EML)]);
    h.start_core();
    let list = ListRef::Folder(inbox);
    h.send(Command::LoadOlder { list });
    let text = h.wait(|e| match e {
        Event::OlderFailed { list: l, text } if *l == list => Some(text.clone()),
        _ => None,
    });
    assert_eq!(
        text,
        "Couldn't load older messages. Try again when you are online."
    );
}

#[test]
fn removing_an_account_does_not_strand_a_load_on_the_inboxes() {
    let mut h = Harness::unstarted();
    h.foreground = true;
    let (_, gone, _) = seeded(&mut h, "one@example.org");
    let (_, _, kept) = seeded(&mut h, "two@example.org");
    h.start_core();
    // The worker sleeps while both commands queue behind the stall, so the
    // removal is handled before any Older step can run.
    h.send(Command::Stall { ms: 500 });
    h.send(Command::LoadOlder {
        list: ListRef::Inboxes,
    });
    h.send(Command::RemoveAccount {
        account: gone,
        confirmed: true,
    });
    let (fetched, exhausted) = older_loaded(&h, ListRef::Inboxes);
    // The removed account did not answer: the total is the other's.
    assert_eq!(fetched, 9);
    assert!(!exhausted);
    assert_eq!(rows_of(&h, ListRef::Folder(kept)), 10);
}
