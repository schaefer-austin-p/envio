//! "Load older" and the sync window's floor, against the fake server.

mod support;

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
    let first = load_older(&mut server, &mut store.local(), inbox)
        .await
        .unwrap();
    assert_eq!(first.fetched.len(), 50);
    assert_eq!((first.floor, first.exhausted), (51, false));
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

    let second = load_older(&mut server, &mut store.local(), inbox)
        .await
        .unwrap();
    assert_eq!(second.fetched.len(), 50);
    assert_eq!((second.floor, second.exhausted), (1, true));
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
    let error = load_older(&mut server, &mut store.local(), inbox)
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
    let older = load_older(&mut server, &mut store.local(), inbox)
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
        Some(older.floor),
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
async fn window_all_caches_everything() {
    let mut server = server_with_history();
    let mut store = Store::new();
    first_sync(&mut server, &mut store, None).await;
    assert_eq!(store.uids("INBOX").len(), 120);
    assert!(server.calls.iter().any(|call| call == "search_all"));
}
