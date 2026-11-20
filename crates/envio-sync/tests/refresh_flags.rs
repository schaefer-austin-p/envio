//! `refresh_flags`: the live keyword check Edit makes before linking a
//! draft to its original (#324 T4).

mod support;

use envio_net::NetError;
use envio_net::imap::{Flag, MessageFlags};
use envio_store::{Flags, MessageRowId};
use envio_sync::driver::{refresh_flags, sync_folder, sync_folders};
use support::fake::{FakeServer, NOW_MS};
use support::local::{ACCOUNT, Store};

fn with_keywords(flags: &[Flag], words: &[&str]) -> MessageFlags {
    MessageFlags::from_flags(
        flags
            .iter()
            .cloned()
            .chain(words.iter().map(|word| Flag::Keyword((*word).into()))),
    )
}

async fn synced() -> (FakeServer, Store, MessageRowId, u32) {
    let mut server = FakeServer::new();
    let uid = server.deliver_id("INBOX", "a");
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
    let id = store.db.ids_of(inbox, &[uid]).unwrap()[0];
    (server, store, id, uid)
}

#[tokio::test]
async fn it_returns_the_servers_keywords_and_refreshes_the_cache() {
    let (mut server, mut store, id, uid) = synced().await;
    let inbox = store.folder("INBOX").id;
    assert_eq!(store.db.keywords_of(inbox, uid).unwrap(), Some(vec![]));
    server.set_flags("INBOX", uid, with_keywords(&[Flag::Seen], &["$Forwarded"]));
    let message = store.db.message(id).unwrap();
    let got = refresh_flags(&mut server, &store.local(), &message)
        .await
        .unwrap();
    assert_eq!(got, Some(vec!["$Forwarded".to_owned()]));
    assert_eq!(
        store.db.keywords_of(inbox, uid).unwrap(),
        Some(vec!["$Forwarded".to_owned()])
    );
    assert!(store.db.message(id).unwrap().flags.contains(Flags::SEEN));
}

#[tokio::test]
async fn a_message_without_keywords_gives_an_empty_list() {
    let (mut server, mut store, id, _) = synced().await;
    let message = store.db.message(id).unwrap();
    let got = refresh_flags(&mut server, &store.local(), &message).await;
    assert_eq!(got.unwrap(), Some(vec![]));
}

#[tokio::test]
async fn a_deleted_or_missing_message_gives_none() {
    let (mut server, mut store, id, uid) = synced().await;
    let message = store.db.message(id).unwrap();
    server.set_flags("INBOX", uid, MessageFlags::from_flags([Flag::Deleted]));
    let got = refresh_flags(&mut server, &store.local(), &message)
        .await
        .unwrap();
    assert_eq!(got, None);

    let (mut server, mut store, id, uid) = synced().await;
    let message = store.db.message(id).unwrap();
    server
        .folders
        .get_mut("INBOX")
        .unwrap()
        .messages
        .remove(&uid);
    let got = refresh_flags(&mut server, &store.local(), &message)
        .await
        .unwrap();
    assert_eq!(got, None);
}

#[tokio::test]
async fn a_failed_fetch_is_an_error_and_a_changed_uidvalidity_gives_none() {
    let (mut server, mut store, id, _) = synced().await;
    let message = store.db.message(id).unwrap();
    server.fail_on("fetch_flags", NetError::Timeout);
    assert!(
        refresh_flags(&mut server, &store.local(), &message)
            .await
            .is_err()
    );

    let (mut server, mut store, id, _) = synced().await;
    let message = store.db.message(id).unwrap();
    server.folders.get_mut("INBOX").unwrap().uid_validity += 1;
    let got = refresh_flags(&mut server, &store.local(), &message)
        .await
        .unwrap();
    assert_eq!(got, None);
}
