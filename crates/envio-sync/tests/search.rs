//! "Search on server" against the fake server.

mod support;

use envio_store::FolderId;
use envio_sync::driver::{search_server, sync_folder, sync_folders};
use support::fake::{FakeServer, NOW_MS, NOW_SECS, eml};
use support::local::{ACCOUNT, Store};

const OLD: i64 = NOW_SECS - 90 * 86_400;

/// Syncs the INBOX (the last thirty days) and returns it.
async fn synced_inbox(server: &mut FakeServer, store: &mut Store) -> FolderId {
    sync_folders(server, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    let id = store.folder("INBOX").id;
    sync_folder(server, &mut store.local(), id, NOW_MS)
        .await
        .unwrap();
    id
}

fn fetches(server: &FakeServer) -> Vec<&String> {
    server
        .calls
        .iter()
        .filter(|call| call.starts_with("fetch_summaries"))
        .collect()
}

#[tokio::test]
async fn server_search_fetches_only_uncached_matches() {
    let mut server = FakeServer::new();
    server.deliver("INBOX", &eml("alpha"), NOW_SECS);
    server.deliver("INBOX", &eml("beta"), NOW_SECS);
    let mut store = Store::new();
    let folder = synced_inbox(&mut server, &mut store).await;
    // Older than the window: sync did not fetch it.
    let old = server.deliver("INBOX", &eml("alpha-old"), OLD);
    server.calls.clear();

    let ids = search_server(&mut server, &mut store.local(), folder, "alpha")
        .await
        .unwrap();
    assert_eq!(ids.len(), 2);
    assert_eq!(fetches(&server), [&format!("fetch_summaries [{old}]")]);
    assert!(store.uids("INBOX").contains(&old));
}

#[tokio::test]
async fn server_search_returns_cached_and_fetched() {
    let mut server = FakeServer::new();
    let recent = server.deliver("INBOX", &eml("alpha"), NOW_SECS);
    server.deliver("INBOX", &eml("beta"), NOW_SECS);
    let mut store = Store::new();
    let folder = synced_inbox(&mut server, &mut store).await;
    let old = server.deliver("INBOX", &eml("alpha-old"), OLD);

    let ids = search_server(&mut server, &mut store.local(), folder, "alpha")
        .await
        .unwrap();
    let mut want = store.db.ids_of(folder, &[recent, old]).unwrap();
    want.sort();
    let mut got = ids.clone();
    got.sort();
    assert_eq!(want.len(), 2);
    assert_eq!(got, want);
    // Asking again fetches nothing.
    server.calls.clear();
    let again = search_server(&mut server, &mut store.local(), folder, "alpha")
        .await
        .unwrap();
    assert_eq!(again.len(), 2);
    assert!(fetches(&server).is_empty());
}

#[tokio::test]
async fn matches_are_fetched_in_batches_of_fifty() {
    let mut server = FakeServer::new();
    let mut store = Store::new();
    let folder = synced_inbox(&mut server, &mut store).await;
    for n in 0..120 {
        server.deliver("INBOX", &eml(&format!("alpha{n}")), OLD);
    }
    server.calls.clear();
    let ids = search_server(&mut server, &mut store.local(), folder, "alpha")
        .await
        .unwrap();
    assert_eq!(ids.len(), 120);
    assert_eq!(fetches(&server).len(), 3);
}

#[tokio::test]
async fn a_folder_whose_uidvalidity_changed_is_refused() {
    let mut server = FakeServer::new();
    server.deliver("INBOX", &eml("alpha"), NOW_SECS);
    let mut store = Store::new();
    let folder = synced_inbox(&mut server, &mut store).await;
    server.folders.get_mut("INBOX").unwrap().uid_validity += 1;
    server.calls.clear();
    let result = search_server(&mut server, &mut store.local(), folder, "alpha").await;
    assert!(result.is_err());
    assert!(fetches(&server).is_empty());
}

#[tokio::test]
async fn server_search_keeps_the_newest_200() {
    let mut server = FakeServer::new();
    let mut store = Store::new();
    let folder = synced_inbox(&mut server, &mut store).await;
    let uids: Vec<u32> = (0..450)
        .map(|n| server.deliver("INBOX", &eml(&format!("alpha{n}")), OLD))
        .collect();
    server.calls.clear();
    let ids = search_server(&mut server, &mut store.local(), folder, "alpha")
        .await
        .unwrap();
    assert_eq!(ids.len(), 200);
    assert!(fetches(&server).len() <= 4);
    let newest = &uids[250..];
    let mut want = store.db.ids_of(folder, newest).unwrap();
    want.sort();
    let mut got = ids.clone();
    got.sort();
    assert_eq!(got, want);
    assert!(store.uids("INBOX").iter().all(|u| *u >= newest[0]));
}
