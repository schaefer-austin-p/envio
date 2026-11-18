//! The Inbox sync reports new mail for the notifier.

mod support;

use envio_mime::Address;
use envio_net::NetError;
use envio_net::imap::{Flag, MessageFlags, Role};
use envio_sync::driver::{Options, Synced, sync_folder, sync_folder_as, sync_folders};
use support::fake::{FakeServer, NOW_MS, NOW_SECS};
use support::local::{ACCOUNT, Store};

async fn sync(server: &mut FakeServer, store: &mut Store, folder: &str) -> Synced {
    sync_folders(server, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    let id = store.folder(folder).id;
    sync_folder(server, &mut store.local(), id, NOW_MS, Some(30), None)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_first_sync_reports_no_new_mail() {
    let mut server = FakeServer::new();
    server.deliver_id("INBOX", "a");
    server.deliver_id("INBOX", "b");
    let mut store = Store::new();
    assert_eq!(
        sync(&mut server, &mut store, "INBOX").await.new_mail.count,
        0
    );
}

#[tokio::test]
async fn later_unseen_inbox_mail_is_counted() {
    let mut server = FakeServer::new();
    server.deliver_id("INBOX", "a");
    let mut store = Store::new();
    sync(&mut server, &mut store, "INBOX").await;

    server.deliver_id("INBOX", "b");
    let seen = server.deliver_id("INBOX", "c");
    server.set_flags("INBOX", seen, MessageFlags::from_flags([Flag::Seen]));
    let got = sync(&mut server, &mut store, "INBOX").await.new_mail;
    assert_eq!(got.count, 1);
    assert_eq!(got.first, Some(("Ada".into(), "About b".into())));

    // Nothing new on the next one.
    assert_eq!(
        sync(&mut server, &mut store, "INBOX").await.new_mail.count,
        0
    );
}

#[tokio::test]
async fn own_addresses_are_not_new_mail() {
    let mut server = FakeServer::new();
    server.deliver_id("INBOX", "a");
    let mut store = Store::new();
    sync(&mut server, &mut store, "INBOX").await;
    server.deliver_id("INBOX", "b");
    let id = store.folder("INBOX").id;
    let own = [Address::new(None, "ADA@example.org")];
    let got = sync_folder_as(
        &mut server,
        &mut store.local(),
        id,
        Options {
            now_ms: NOW_MS,
            window_days: Some(30),
            kept: None,
            own: &own,
            window: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(got.new_mail.count, 0);
}

#[tokio::test]
async fn other_folders_report_no_new_mail() {
    let mut server = FakeServer::new();
    server.add_folder("Archive", Some(Role::Archive));
    let mut store = Store::new();
    sync(&mut server, &mut store, "Archive").await;
    server.deliver_id("Archive", "a");
    assert_eq!(
        sync(&mut server, &mut store, "Archive")
            .await
            .new_mail
            .count,
        0
    );
}

fn inbox_of(count: usize) -> FakeServer {
    let mut server = FakeServer::new();
    for n in 0..count {
        server.deliver_id("INBOX", &format!("first{n}"));
    }
    server
}

fn deliver_more(server: &mut FakeServer, count: usize) {
    for n in 0..count {
        server.deliver_id("INBOX", &format!("later{n}"));
    }
}

#[tokio::test]
async fn a_uidvalidity_reset_reports_no_new_mail() {
    let mut server = FakeServer::new();
    server.deliver_id("INBOX", "a");
    let mut store = Store::new();
    sync(&mut server, &mut store, "INBOX").await;
    server.deliver_id("INBOX", "b");
    server.renumber("INBOX");
    assert_eq!(
        sync(&mut server, &mut store, "INBOX").await.new_mail.count,
        0
    );
}

#[tokio::test]
async fn a_resumed_first_sync_reports_no_new_mail() {
    let mut server = inbox_of(250);
    let mut store = Store::new();
    loop {
        let synced = sync(&mut server, &mut store, "INBOX").await;
        assert_eq!(synced.new_mail.count, 0);
        if !synced.more {
            break;
        }
    }
}

#[tokio::test]
async fn a_sync_in_two_calls_reports_the_total_once() {
    let mut server = inbox_of(1);
    let mut store = Store::new();
    sync(&mut server, &mut store, "INBOX").await;
    deliver_more(&mut server, 120);
    let first = sync(&mut server, &mut store, "INBOX").await;
    assert!(first.more);
    assert_eq!(first.new_mail.count, 0);
    let last = sync(&mut server, &mut store, "INBOX").await;
    assert!(!last.more);
    assert_eq!(last.new_mail.count, 120);
}

#[tokio::test]
async fn a_cut_sync_still_reports_what_it_stored_when_retried() {
    let mut server = inbox_of(1);
    let mut store = Store::new();
    sync(&mut server, &mut store, "INBOX").await;
    deliver_more(&mut server, 120);
    server.fail_after("fetch_summaries", 1, NetError::Timeout);
    sync_folders(&mut server, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    let id = store.folder("INBOX").id;
    let cut = sync_folder(&mut server, &mut store.local(), id, NOW_MS, Some(30), None).await;
    assert!(cut.is_err());
    let retried = sync(&mut server, &mut store, "INBOX").await;
    assert!(!retried.more);
    assert_eq!(retried.new_mail.count, 120);
}

fn from_eml(from: &str) -> Vec<u8> {
    format!(
        "From: {from}\r\nTo: bob@example.org\r\nSubject: Hi\r\n\
         Message-ID: <m@example.org>\r\nDate: Thu, 14 Jan 2027 10:00:00 +0000\r\n\r\nHello\r\n"
    )
    .into_bytes()
}

/// Syncs the Inbox after delivering a message with this From header.
async fn after_from(from: &str, own: &[Address]) -> Synced {
    let mut server = FakeServer::new();
    server.deliver_id("INBOX", "a");
    let mut store = Store::new();
    sync(&mut server, &mut store, "INBOX").await;
    server.deliver("INBOX", &from_eml(from), NOW_SECS);
    let id = store.folder("INBOX").id;
    let how = Options {
        now_ms: NOW_MS,
        window_days: Some(30),
        kept: None,
        own,
        window: None,
    };
    sync_folder_as(&mut server, &mut store.local(), id, how)
        .await
        .unwrap()
}

#[tokio::test]
async fn the_first_of_several_from_addresses_is_the_sender() {
    let own = [Address::new(None, "me@own.org")];
    let got = after_from("Me <me@own.org>, Other <o@x.org>", &own).await;
    assert_eq!(got.new_mail.count, 0);
    let got = after_from("a@x.org, b@y.org", &[]).await;
    assert_eq!(got.new_mail.first, Some(("a@x.org".into(), "Hi".into())));
}
