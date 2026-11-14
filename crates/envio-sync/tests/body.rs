//! Opening a body: a big message comes in parts, a small one whole, and
//! what is stored for a partial one says so.

mod support;

use envio_store::{BodyState, MessageRowId};
use envio_sync::driver::{
    PARTIAL_ABOVE, fetch_body, fetch_part, fetch_whole_body, sync_folder, sync_folders,
};
use support::fake::{FakeServer, NOW_MS, eml};
use support::local::{ACCOUNT, Store};

/// One message of `padding` extra bytes in the INBOX, synced.
async fn synced(padding: usize) -> (FakeServer, Store, MessageRowId) {
    let mut server = FakeServer::new();
    let mut raw = eml("big");
    raw.extend(std::iter::repeat_n(b'x', padding));
    server.deliver("INBOX", &raw, support::fake::NOW_SECS);
    let mut store = Store::new();
    sync_folders(&mut server, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    let inbox = store.folder("INBOX").id;
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
    let uid = store.uids("INBOX")[0];
    let id = store.db.ids_of(inbox, &[uid]).unwrap()[0];
    (server, store, id)
}

fn called(server: &FakeServer, op: &str) -> bool {
    server.calls.iter().any(|c| c.starts_with(op))
}

#[tokio::test]
async fn a_small_message_is_fetched_whole() {
    let (mut server, mut store, id) = synced(1000).await;
    fetch_body(&mut server, &mut store.local(), id)
        .await
        .unwrap();
    assert!(called(&server, "fetch_body") && !called(&server, "fetch_partial"));
    assert_eq!(store.db.message(id).unwrap().body, BodyState::Downloaded);
}

#[tokio::test]
async fn a_big_message_is_fetched_in_parts_and_marked_partial() {
    let (mut server, mut store, id) = synced(PARTIAL_ABOVE as usize).await;
    let got = fetch_body(&mut server, &mut store.local(), id)
        .await
        .unwrap();
    assert!(called(&server, "fetch_partial") && !called(&server, "fetch_body"));
    assert!(got.is_some());
    assert_eq!(store.db.message(id).unwrap().body, BodyState::Partial);
}

#[tokio::test]
async fn asking_for_the_whole_message_replaces_the_partial_body() {
    let (mut server, mut store, id) = synced(PARTIAL_ABOVE as usize).await;
    fetch_body(&mut server, &mut store.local(), id)
        .await
        .unwrap();
    fetch_whole_body(&mut server, &mut store.local(), id)
        .await
        .unwrap();
    assert!(called(&server, "fetch_body"));
    assert_eq!(store.db.message(id).unwrap().body, BodyState::Downloaded);
}

#[tokio::test]
async fn one_part_is_fetched_on_its_own_and_not_stored() {
    let (mut server, mut store, id) = synced(PARTIAL_ABOVE as usize).await;
    let part = fetch_part(&mut server, &mut store.local(), id, "2", 1000)
        .await
        .unwrap();
    assert_eq!(part.as_deref(), Some(&b"part 2"[..]));
    assert_eq!(store.db.message(id).unwrap().body, BodyState::Missing);
}

#[tokio::test]
async fn a_part_of_a_changed_folder_is_none() {
    let (mut server, mut store, id) = synced(10).await;
    server.folders.get_mut("INBOX").unwrap().uid_validity += 1;
    let part = fetch_part(&mut server, &mut store.local(), id, "2", 10)
        .await
        .unwrap();
    assert_eq!(part, None);
}
