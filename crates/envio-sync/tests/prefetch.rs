//! Several bodies in one session (#102).

mod support;

use envio_net::NetError;
use envio_store::{BodyState, MessageRowId};
use envio_sync::driver::{prefetch_bodies, sync_folder, sync_folders};
use support::fake::{FakeServer, NOW_MS};
use support::local::{ACCOUNT, Store};

async fn synced(count: usize) -> (FakeServer, Store, Vec<MessageRowId>) {
    let mut server = FakeServer::new();
    for n in 0..count {
        server.deliver_id("INBOX", &format!("m{n}"));
    }
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
    let ids = store.uids("INBOX");
    let ids = store.db.ids_of(inbox, &ids).unwrap();
    (server, store, ids)
}

#[tokio::test]
async fn one_select_serves_every_body_and_each_is_stored() {
    let (mut server, mut store, ids) = synced(3).await;
    let inbox = store.folder("INBOX").id;
    server.calls.clear();
    let got = prefetch_bodies(&mut server, &mut store.local(), inbox, &ids)
        .await
        .unwrap();
    assert!(got.error.is_none());
    assert_eq!(got.done.len(), 3);
    assert!(got.done.iter().all(|(_, eml)| eml.is_some()));
    let selects = server
        .calls
        .iter()
        .filter(|c| c.starts_with("select"))
        .count();
    assert_eq!(selects, 1, "{:?}", server.calls);
    for (id, eml) in got.done {
        let message = store.db.message(id).unwrap();
        assert_eq!(message.body, BodyState::Downloaded);
        assert_eq!(store.bodies.read(inbox, message.uid).unwrap(), eml.unwrap());
    }
}

#[tokio::test]
async fn a_message_gone_from_the_server_is_none_and_its_row_goes() {
    let (mut server, mut store, ids) = synced(2).await;
    let inbox = store.folder("INBOX").id;
    let gone = store.db.message(ids[0]).unwrap().uid;
    server
        .folders
        .get_mut("INBOX")
        .unwrap()
        .messages
        .remove(&gone);
    let got = prefetch_bodies(&mut server, &mut store.local(), inbox, &ids)
        .await
        .unwrap();
    assert_eq!(got.done[0], (ids[0], None));
    assert!(got.done[1].1.is_some());
    assert!(store.db.message(ids[0]).is_err());
}

#[tokio::test]
async fn a_changed_uidvalidity_gives_none_for_all_and_keeps_the_rows() {
    let (mut server, mut store, ids) = synced(2).await;
    let inbox = store.folder("INBOX").id;
    server.folders.get_mut("INBOX").unwrap().uid_validity += 1;
    let got = prefetch_bodies(&mut server, &mut store.local(), inbox, &ids)
        .await
        .unwrap();
    assert!(got.done.iter().all(|(_, eml)| eml.is_none()));
    assert_eq!(store.db.message(ids[0]).unwrap().body, BodyState::Missing);
    assert!(!server.calls.iter().any(|c| c.starts_with("fetch_body")));
}

#[tokio::test]
async fn a_connection_error_comes_back_with_what_was_stored() {
    let (mut server, mut store, ids) = synced(3).await;
    let inbox = store.folder("INBOX").id;
    server.fail_after("fetch_body", 1, NetError::Timeout);
    // The second fetch breaks: the first body is kept and reported, and the
    // error is not swallowed, so the caller can drop the session.
    let got = prefetch_bodies(&mut server, &mut store.local(), inbox, &ids)
        .await
        .unwrap();
    assert_eq!(got.done.len(), 1);
    assert!(got.error.is_some());
    assert_eq!(
        store.db.message(ids[0]).unwrap().body,
        BodyState::Downloaded
    );
    // With nothing fetched before it the same holds: an empty `done`.
    server.clear_failures();
    server.fail_on("fetch_body", NetError::Timeout);
    let rest = &ids[1..];
    let got = prefetch_bodies(&mut server, &mut store.local(), inbox, rest)
        .await
        .unwrap();
    assert!(got.done.is_empty());
    assert!(got.error.is_some());
}

#[tokio::test]
async fn a_refused_select_is_the_calls_error() {
    let (mut server, mut store, ids) = synced(1).await;
    let inbox = store.folder("INBOX").id;
    server.fail_on("select", NetError::Refused("no".into()));
    assert!(
        prefetch_bodies(&mut server, &mut store.local(), inbox, &ids)
            .await
            .is_err()
    );
}
