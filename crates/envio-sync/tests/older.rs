//! "Load older" and the sync window's floor, against the fake server.

mod support;

use envio_net::imap::{Flag, MessageFlags};
use envio_sync::ErrorKind;
use envio_sync::driver::{load_older, sync_folder, sync_folders};
use support::fake::{FakeServer, NOW_MS, NOW_SECS, eml};
use support::local::{ACCOUNT, Store};

/// 100 messages from 40 days ago (UIDs 1..=100), then 20 from now.
fn server_with_history() -> FakeServer {
    let mut server = FakeServer::new();
    for n in 0..100 {
        server.deliver("INBOX", &eml(&format!("old{n}")), NOW_SECS - 40 * 86_400);
    }
    for n in 0..20 {
        server.deliver_id("INBOX", &format!("new{n}"));
    }
    server
}

async fn first_sync(server: &mut FakeServer, store: &mut Store, window: Option<u32>) {
    sync_folders(server, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    let inbox = store.folder("INBOX").id;
    while sync_folder(server, &mut store.local(), inbox, NOW_MS, window, None)
        .await
        .unwrap()
        .more
    {}
}

fn fetches(server: &FakeServer) -> usize {
    server
        .calls
        .iter()
        .filter(|call| call.starts_with("fetch_summaries"))
        .count()
}

#[tokio::test]
async fn load_older_fetches_newest_first_in_one_transaction_per_batch() {
    let mut server = server_with_history();
    let mut store = Store::new();
    first_sync(&mut server, &mut store, Some(30)).await;
    assert_eq!(store.uids("INBOX"), (101..=120).collect::<Vec<_>>());
    let inbox = store.folder("INBOX").id;

    let before = fetches(&server);
    let first = load_older(
        &mut server,
        &mut store.local(),
        inbox,
        (NOW_MS, Some(30)),
        None,
    )
    .await
    .unwrap();
    assert_eq!(first.fetched.len(), 50);
    assert!(!first.exhausted);
    // One fetch, so one batch and one transaction.
    assert_eq!(fetches(&server), before + 1);
    assert_eq!(store.uids("INBOX").first(), Some(&51));
    // Newest first: the ids follow the UIDs from 100 down to 51.
    let uids: Vec<u32> = first
        .fetched
        .iter()
        .map(|id| store.db.message(*id).unwrap().uid)
        .collect();
    assert_eq!(uids, (51..=100).rev().collect::<Vec<_>>());

    let second = load_older(
        &mut server,
        &mut store.local(),
        inbox,
        (NOW_MS, Some(30)),
        Some(first.kept()),
    )
    .await
    .unwrap();
    assert_eq!(second.fetched.len(), 50);
    assert!(second.exhausted);
    assert_eq!(store.uids("INBOX"), (1..=120).collect::<Vec<_>>());
}

#[tokio::test]
async fn load_older_refuses_on_uidvalidity_change() {
    let mut server = server_with_history();
    let mut store = Store::new();
    first_sync(&mut server, &mut store, Some(30)).await;
    let inbox = store.folder("INBOX").id;
    server.renumber("INBOX");
    let before = fetches(&server);
    let error = load_older(
        &mut server,
        &mut store.local(),
        inbox,
        (NOW_MS, Some(30)),
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Refused);
    assert_eq!(fetches(&server), before);
    assert_eq!(store.uids("INBOX"), (101..=120).collect::<Vec<_>>());
}

#[tokio::test]
async fn a_sync_with_the_floor_keeps_older_rows_and_without_it_drops_them() {
    let mut server = server_with_history();
    let mut store = Store::new();
    first_sync(&mut server, &mut store, Some(30)).await;
    let inbox = store.folder("INBOX").id;
    let older = load_older(
        &mut server,
        &mut store.local(),
        inbox,
        (NOW_MS, Some(30)),
        None,
    )
    .await
    .unwrap();
    // A server-side deletion inside the window still goes.
    server
        .folders
        .get_mut("INBOX")
        .unwrap()
        .messages
        .remove(&110);
    let kept = sync_folder(
        &mut server,
        &mut store.local(),
        inbox,
        NOW_MS,
        Some(30),
        Some(older.kept()),
    )
    .await
    .unwrap();
    assert_eq!(kept.removed, 1);
    assert_eq!(store.uids("INBOX").len(), 50 + 19);
    assert!(!store.uids("INBOX").contains(&110));
    sync_folder(
        &mut server,
        &mut store.local(),
        inbox,
        NOW_MS,
        Some(30),
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        store.uids("INBOX"),
        (101..=120).filter(|uid| *uid != 110).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn a_whole_batch_of_deleted_mail_does_not_stall_load_older() {
    let mut server = server_with_history();
    for uid in 51..=100 {
        server.set_flags("INBOX", uid, MessageFlags::from_flags([Flag::Deleted]));
    }
    let mut store = Store::new();
    first_sync(&mut server, &mut store, Some(30)).await;
    let inbox = store.folder("INBOX").id;
    let first = load_older(
        &mut server,
        &mut store.local(),
        inbox,
        (NOW_MS, Some(30)),
        None,
    )
    .await
    .unwrap();
    // Everything in the batch was \Deleted: nothing is stored.
    assert!(first.fetched.is_empty());
    assert!(!first.exhausted);
    let second = load_older(
        &mut server,
        &mut store.local(),
        inbox,
        (NOW_MS, Some(30)),
        Some(first.kept()),
    )
    .await
    .unwrap();
    assert_eq!(second.fetched.len(), 50);
    assert!(second.exhausted);
}

#[tokio::test]
async fn window_all_caches_everything() {
    let mut server = server_with_history();
    let mut store = Store::new();
    first_sync(&mut server, &mut store, None).await;
    assert_eq!(store.uids("INBOX").len(), 120);
    assert!(server.calls.iter().any(|call| call == "search_all"));
}

/// 20 messages from now (UIDs 1..=20), then 100 from 40 days ago appended
/// after them (UIDs 21..=120): the old mail has the higher UIDs (#94).
fn server_with_old_mail_appended_last() -> FakeServer {
    let mut server = FakeServer::new();
    for n in 0..20 {
        server.deliver_id("INBOX", &format!("new{n}"));
    }
    for n in 0..100 {
        server.deliver("INBOX", &eml(&format!("old{n}")), NOW_SECS - 40 * 86_400);
    }
    server
}

#[tokio::test]
async fn load_older_reaches_old_mail_appended_after_the_window_mail() {
    let mut server = server_with_old_mail_appended_last();
    let mut store = Store::new();
    first_sync(&mut server, &mut store, Some(30)).await;
    assert_eq!(store.uids("INBOX"), (1..=20).collect::<Vec<_>>());
    let inbox = store.folder("INBOX").id;

    let first = load_older(
        &mut server,
        &mut store.local(),
        inbox,
        (NOW_MS, Some(30)),
        None,
    )
    .await
    .unwrap();
    assert_eq!(first.fetched.len(), 50);
    assert!(!first.exhausted);
    let second = load_older(
        &mut server,
        &mut store.local(),
        inbox,
        (NOW_MS, Some(30)),
        Some(first.kept()),
    )
    .await
    .unwrap();
    assert_eq!(second.fetched.len(), 50);
    assert!(second.exhausted);
    assert_eq!(store.uids("INBOX"), (1..=120).collect::<Vec<_>>());

    // A sync that holds what Load older reached keeps it all.
    sync_folder(
        &mut server,
        &mut store.local(),
        inbox,
        NOW_MS,
        Some(30),
        Some(second.kept()),
    )
    .await
    .unwrap();
    assert_eq!(store.uids("INBOX").len(), 120);
}

#[tokio::test]
async fn in_window_mail_that_load_older_meets_is_not_kept_when_deleted() {
    let mut server = server_with_history();
    let mut store = Store::new();
    first_sync(&mut server, &mut store, Some(30)).await;
    let inbox = store.folder("INBOX").id;
    // Mail that arrived since the last sync: in the window, not fetched yet.
    server.deliver_id("INBOX", "fresh");
    let older = load_older(
        &mut server,
        &mut store.local(),
        inbox,
        (NOW_MS, Some(30)),
        None,
    )
    .await
    .unwrap();
    // The user deletes it in webmail; the next sync must not keep its row.
    server
        .folders
        .get_mut("INBOX")
        .unwrap()
        .messages
        .remove(&121);
    sync_folder(
        &mut server,
        &mut store.local(),
        inbox,
        NOW_MS,
        Some(30),
        Some(older.kept()),
    )
    .await
    .unwrap();
    assert!(!store.uids("INBOX").contains(&121));
    assert_eq!(store.uids("INBOX").len(), 20 + 50);
}

#[tokio::test]
async fn mail_arriving_between_load_olders_two_searches_is_left_to_the_sync() {
    let mut server = server_with_history();
    let mut store = Store::new();
    first_sync(&mut server, &mut store, Some(30)).await;
    let inbox = store.folder("INBOX").id;
    server.deliver_after_next_search("late");
    let older = load_older(
        &mut server,
        &mut store.local(),
        inbox,
        (NOW_MS, Some(30)),
        None,
    )
    .await
    .unwrap();
    // UID 121 arrived mid-call: in the window, so the sync's, not kept.
    assert!(!store.uids("INBOX").contains(&121));
    server
        .folders
        .get_mut("INBOX")
        .unwrap()
        .messages
        .remove(&121);
    sync_folder(
        &mut server,
        &mut store.local(),
        inbox,
        NOW_MS,
        Some(30),
        Some(older.kept()),
    )
    .await
    .unwrap();
    assert!(!store.uids("INBOX").contains(&121));
}
