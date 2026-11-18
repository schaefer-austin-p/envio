//! A first sync split over several calls searches the window at the start and at the end only (#98).

mod support;

use envio_sync::driver::{Options, Synced, WindowMemo, sync_folder_as, sync_folders};
use support::fake::{FakeServer, NOW_MS};
use support::local::{ACCOUNT, Store};

/// 450 messages: several calls of `BATCHES_PER_STEP` batches are needed.
fn busy() -> FakeServer {
    let mut server = FakeServer::new();
    for n in 0..450 {
        server.deliver_id("INBOX", &format!("m{n}"));
    }
    server
}

async fn part(server: &mut FakeServer, store: &mut Store, window: Option<WindowMemo>) -> Synced {
    sync_folders(server, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    let id = store.folder("INBOX").id;
    let how = Options {
        now_ms: NOW_MS,
        window_days: Some(30),
        kept: None,
        own: &[],
        window,
    };
    sync_folder_as(server, &mut store.local(), id, how)
        .await
        .unwrap()
}

async fn finish(server: &mut FakeServer, store: &mut Store, mut synced: Synced) {
    while synced.more {
        synced = part(server, store, synced.window.take()).await;
    }
}

fn searches(server: &FakeServer) -> usize {
    server
        .calls
        .iter()
        .filter(|call| call.starts_with("search_"))
        .count()
}

#[tokio::test]
async fn a_first_sync_over_several_calls_searches_the_window_once() {
    let mut server = busy();
    let mut store = Store::new();
    let first = part(&mut server, &mut store, None).await;
    assert!(first.more);
    finish(&mut server, &mut store, first).await;
    assert_eq!(store.uids("INBOX").len(), 450);
    // The first part and the last (which checks for expunges), not five.
    assert_eq!(searches(&server), 2, "{:?}", server.calls);
}

#[tokio::test]
async fn mail_that_arrives_mid_sync_is_still_fetched() {
    let mut server = busy();
    let mut store = Store::new();
    let first = part(&mut server, &mut store, None).await;
    assert!(first.more);
    server.deliver_id("INBOX", "late");
    finish(&mut server, &mut store, first).await;
    assert_eq!(store.uids("INBOX").len(), 451);
}

#[tokio::test]
async fn a_new_uidvalidity_ignores_the_remembered_window() {
    let mut server = busy();
    let mut store = Store::new();
    let first = part(&mut server, &mut store, None).await;
    server.renumber("INBOX");
    let before = searches(&server);
    part(&mut server, &mut store, first.window).await;
    assert_eq!(searches(&server), before + 1);
    // Rows of the old numbering are gone; the new first part is cached.
    let uids = store.uids("INBOX");
    assert_eq!(uids.len(), 100, "{uids:?}");
}

#[tokio::test]
async fn a_first_sync_finishes_when_the_next_uids_were_expunged_mid_sync() {
    let mut server = busy();
    let mut store = Store::new();
    let first = part(&mut server, &mut store, None).await;
    assert!(first.more);
    // Newest first: the first part took 450..=351. Expunge the next 120.
    let folder = server.folders.get_mut("INBOX").unwrap();
    for uid in 231..=350 {
        folder.messages.remove(&uid);
    }
    let mut synced = first;
    for _ in 0..30 {
        if !synced.more {
            break;
        }
        synced = part(&mut server, &mut store, synced.window.take()).await;
    }
    assert!(!synced.more, "never finished");
    assert_eq!(store.uids("INBOX"), server.uids("INBOX"));
    assert!(synced.window.is_none());
}

#[tokio::test]
async fn a_changed_window_searches_again() {
    let mut server = busy();
    let mut store = Store::new();
    let first = part(&mut server, &mut store, None).await;
    let before = searches(&server);
    let id = store.folder("INBOX").id;
    let how = Options {
        now_ms: NOW_MS,
        window_days: None,
        kept: None,
        own: &[],
        window: first.window,
    };
    sync_folder_as(&mut server, &mut store.local(), id, how)
        .await
        .unwrap();
    assert_eq!(searches(&server), before + 1);
}

#[tokio::test]
async fn another_folders_window_is_ignored() {
    let mut server = busy();
    server.add_folder("Archive", None);
    let mut store = Store::new();
    let first = part(&mut server, &mut store, None).await;
    let mut memo = first.window.unwrap();
    memo.folder = store.folder("Archive").id;
    let before = searches(&server);
    part(&mut server, &mut store, Some(memo)).await;
    assert_eq!(searches(&server), before + 1);
}

#[tokio::test]
async fn the_window_is_dropped_once_the_sync_is_finished() {
    let mut server = busy();
    let mut store = Store::new();
    let mut synced = part(&mut server, &mut store, None).await;
    assert!(synced.window.is_some());
    while synced.more {
        synced = part(&mut server, &mut store, synced.window.take()).await;
    }
    assert!(synced.window.is_none());
}

#[tokio::test]
async fn a_batch_with_some_uids_expunged_still_finishes_with_the_right_rows() {
    let mut server = busy();
    let mut store = Store::new();
    let first = part(&mut server, &mut store, None).await;
    assert!(first.more);
    // Every other UID of the next 120 goes: each batch is half there.
    let folder = server.folders.get_mut("INBOX").unwrap();
    for uid in (231..=350).filter(|uid| uid % 2 == 0) {
        folder.messages.remove(&uid);
    }
    let mut synced = first;
    for _ in 0..30 {
        if !synced.more {
            break;
        }
        synced = part(&mut server, &mut store, synced.window.take()).await;
    }
    assert!(!synced.more, "never finished");
    assert_eq!(store.uids("INBOX"), server.uids("INBOX"));
}
