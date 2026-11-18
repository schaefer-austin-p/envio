//! The Inbox sync reports new mail for the notifier.

mod support;

use envio_mime::Address;
use envio_net::imap::{Flag, MessageFlags, Role};
use envio_sync::driver::{Options, Synced, sync_folder, sync_folder_as, sync_folders};
use support::fake::{FakeServer, NOW_MS};
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
