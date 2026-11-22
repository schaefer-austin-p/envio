//! An unconfirmed send on a Graph account is looked for in Sent a few
//! times, 10 s apart, before the user is asked. Nothing is ever resent.

mod support;

use envio_net::NetError;
use envio_net::imap::Role;
use envio_store::{NewOutboxItem, Outbox, SendState};
use envio_sync::driver::{FileOpts, Filed, file_sent, sync_folders};
use envio_sync::plan::outbox::{RESOLVE_GAP_MS, RESOLVE_TRIES};
use support::fake::{FakeServer, NOW_MS, eml};
use support::local::{ACCOUNT, Store};

async fn store_with_sent(server: &mut FakeServer, with_sent: bool) -> Store {
    if with_sent {
        server.add_folder("Sent", Some(Role::Sent));
    }
    let mut store = Store::new();
    sync_folders(server, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    store
}

/// An item that may have been sent.
fn unconfirmed(store: &Store) -> String {
    let item = NewOutboxItem {
        send_after_ms: 0,
        account: ACCOUNT.into(),
        message_id: "<out@example.org>".into(),
        queued_at_ms: NOW_MS,
        from: "carol@example.org".into(),
        recipients: vec!["dave@example.org".into()],
    };
    let id = store.outbox.enqueue(item, &eml("out")).unwrap().id;
    store
        .outbox
        .set_state(&id, SendState::MayHaveBeenSent)
        .unwrap();
    id
}

fn meta(store: &Store, id: &str) -> (u32, i64) {
    let items = store.outbox.items().unwrap();
    let item = items.iter().find(|i| i.id == id).unwrap();
    (item.meta.resolve_tries, item.meta.resolve_after_ms)
}

async fn file(server: &mut FakeServer, store: &mut Store, now: i64, retries: bool) -> Filed {
    let opts = FileOpts {
        files_itself: true,
        now_ms: now,
        retries,
    };
    file_sent(server, &mut store.local(), ACCOUNT, opts)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_graph_send_is_looked_for_three_times_10_s_apart_then_asked_about() {
    assert_eq!((RESOLVE_TRIES, RESOLVE_GAP_MS), (3, 10_000));
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server, true).await;
    let id = unconfirmed(&store);
    let first = file(&mut server, &mut store, NOW_MS, true).await;
    assert_eq!(first.retry, [id.as_str()]);
    assert!(first.ask.is_empty());
    assert_eq!(meta(&store, &id), (1, NOW_MS + RESOLVE_GAP_MS));
    let at = NOW_MS + RESOLVE_GAP_MS;
    let second = file(&mut server, &mut store, at, true).await;
    assert_eq!(second.retry, [id.as_str()]);
    assert_eq!(meta(&store, &id), (2, at + RESOLVE_GAP_MS));
    let third = file(&mut server, &mut store, at + RESOLVE_GAP_MS, true).await;
    assert_eq!(third.ask, [id.as_str()]);
    assert!(third.retry.is_empty());
    assert_eq!(
        store.outbox.items().unwrap()[0].meta.state,
        SendState::MayHaveBeenSent
    );
}

#[tokio::test]
async fn a_run_before_the_wait_is_over_burns_no_try_and_does_not_look() {
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server, true).await;
    let id = unconfirmed(&store);
    file(&mut server, &mut store, NOW_MS, true).await;
    // A look now would fail on the select: it must not happen.
    server.fail_on("select", NetError::Timeout);
    let early = file(&mut server, &mut store, NOW_MS + RESOLVE_GAP_MS - 1, true).await;
    assert_eq!(early, Filed::default());
    assert_eq!(meta(&store, &id), (1, NOW_MS + RESOLVE_GAP_MS));
}

#[tokio::test]
async fn the_tries_survive_a_restart() {
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server, true).await;
    let id = unconfirmed(&store);
    file(&mut server, &mut store, NOW_MS, true).await;
    file(&mut server, &mut store, NOW_MS + RESOLVE_GAP_MS, true).await;
    store.outbox = Outbox::open(&store.dir.path().join("outbox")).unwrap();
    assert_eq!(meta(&store, &id).0, 2);
    let last = file(&mut server, &mut store, NOW_MS + 2 * RESOLVE_GAP_MS, true).await;
    assert_eq!(
        last.ask,
        [id.as_str()],
        "a restart neither resets nor skips"
    );
}

#[tokio::test]
async fn send_again_starts_the_count_afresh() {
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server, true).await;
    let id = unconfirmed(&store);
    file(&mut server, &mut store, NOW_MS, true).await;
    store.outbox.set_state(&id, SendState::Queued).unwrap();
    store
        .outbox
        .set_state(&id, SendState::MayHaveBeenSent)
        .unwrap();
    assert_eq!(meta(&store, &id), (0, 0));
    let again = file(&mut server, &mut store, NOW_MS + 1, true).await;
    assert_eq!(again.retry, [id.as_str()]);
}

#[tokio::test]
async fn a_later_look_that_finds_it_removes_it() {
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server, true).await;
    let id = unconfirmed(&store);
    file(&mut server, &mut store, NOW_MS, true).await;
    server.deliver_id("Sent", "out");
    let found = file(&mut server, &mut store, NOW_MS + RESOLVE_GAP_MS, true).await;
    assert_eq!(found.removed, [id]);
    assert!(store.outbox.items().unwrap().is_empty());
}

#[tokio::test]
async fn imap_still_asks_on_the_first_miss() {
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server, true).await;
    let id = unconfirmed(&store);
    let filed = file(&mut server, &mut store, NOW_MS, false).await;
    assert_eq!(filed.ask, [id.as_str()]);
    assert!(filed.retry.is_empty());
    assert_eq!(meta(&store, &id), (0, 0));
}

#[tokio::test]
async fn an_account_without_sent_asks_at_once() {
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server, false).await;
    let id = unconfirmed(&store);
    let filed = file(&mut server, &mut store, NOW_MS, true).await;
    assert_eq!(filed.ask, [id.as_str()]);
    assert!(filed.retry.is_empty());
}

#[tokio::test]
async fn a_refused_sent_select_waits_and_burns_no_try_and_asks_nothing() {
    let mut server = FakeServer::new();
    let mut store = store_with_sent(&mut server, true).await;
    let id = unconfirmed(&store);
    server.fail_on("select", NetError::Refused("no".into()));
    let filed = file(&mut server, &mut store, NOW_MS, true).await;
    assert_eq!(filed.retry, [id.as_str()]);
    assert!(filed.ask.is_empty() && filed.removed.is_empty());
    assert!(filed.relist);
    assert_eq!(meta(&store, &id), (0, NOW_MS + RESOLVE_GAP_MS));
}
