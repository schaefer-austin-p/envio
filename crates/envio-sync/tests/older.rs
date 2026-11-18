//! "Load older" and the sync window's floor, against the fake server.

mod support;

use envio_net::imap::{Flag, MessageFlags};
use envio_sync::ErrorKind;
use envio_sync::driver::{load_older, sync_folder, sync_folders};
use support::fake::{FakeServer, NOW_MS, NOW_SECS, eml, eml_undated};
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
async fn load_older_fetches_newest_first_a_batch_at_a_time() {
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
        None,
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
async fn without_a_tail_a_whole_batch_of_deleted_mail_ends_the_call_and_the_next_goes_on() {
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
        None,
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
        None,
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

const DAY: i64 = 86_400;

/// `count` messages that arrived `age` seconds before NOW, one second apart
/// (the last UID the newest), with no Date header: the date is the arrival.
fn deliver_aged(server: &mut FakeServer, count: i64, age: i64) {
    for n in 0..count {
        server.deliver(
            "INBOX",
            &eml_undated(&format!("m{age}-{n}")),
            NOW_SECS - age + n,
        );
    }
}

/// 20 messages from now (UIDs 1..=20, the sync window's), then more by UID
/// as `layers` says (count, age in seconds).
async fn windowed_with(layers: &[(i64, i64)]) -> (FakeServer, Store, envio_store::FolderId) {
    let mut server = FakeServer::new();
    for n in 0..20 {
        server.deliver_id("INBOX", &format!("new{n}"));
    }
    for (count, age) in layers {
        deliver_aged(&mut server, *count, *age);
    }
    let mut store = Store::new();
    first_sync(&mut server, &mut store, Some(30)).await;
    let inbox = store.folder("INBOX").id;
    (server, store, inbox)
}

/// One call with `kept` and `tail`.
async fn older(
    (server, store, inbox): (&mut FakeServer, &mut Store, envio_store::FolderId),
    kept: Option<envio_sync::plan::sync::Kept>,
    tail: Option<i64>,
) -> envio_sync::driver::Older {
    load_older(
        server,
        &mut store.local(),
        inbox,
        (NOW_MS, Some(30)),
        kept,
        tail,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn load_older_goes_on_past_a_batch_dated_above_the_tail() {
    // By UID: 21..=70 are the oldest, 71..=120 newer, 121..=170 between.
    let (mut server, mut store, inbox) =
        windowed_with(&[(50, 60 * DAY), (50, 35 * DAY), (50, 40 * DAY)]).await;
    let first = older((&mut server, &mut store, inbox), None, Some(NOW_SECS)).await;
    assert_eq!(first.fetched.len(), 50);
    // The next batch (UIDs 71..=120) is dated above the 40-day tail: the
    // call also fetches the one below it.
    let tail = NOW_SECS - 40 * DAY;
    let second = older(
        (&mut server, &mut store, inbox),
        Some(first.kept()),
        Some(tail),
    )
    .await;
    assert_eq!(second.fetched.len(), 100);
    assert!(second.exhausted);
}

#[tokio::test]
async fn load_older_without_a_tail_fetches_one_batch() {
    let (mut server, mut store, inbox) = windowed_with(&[(50, 60 * DAY), (50, 35 * DAY)]).await;
    let before = fetches(&server);
    let first = older((&mut server, &mut store, inbox), None, None).await;
    assert_eq!(first.fetched.len(), 50);
    assert_eq!(fetches(&server) - before, 1);
}

#[tokio::test]
async fn load_older_stops_at_the_cap_when_nothing_gets_below_the_tail() {
    // 600 messages, all dated above the tail.
    let (mut server, mut store, inbox) = windowed_with(&[(600, 35 * DAY)]).await;
    let before = fetches(&server);
    let tail = Some(NOW_SECS - 40 * DAY);
    let first = older((&mut server, &mut store, inbox), None, tail).await;
    assert_eq!(first.fetched.len(), 500);
    assert!(!first.exhausted);
    assert_eq!(fetches(&server) - before, 10);
}

#[tokio::test]
async fn mail_dated_equal_to_the_tail_is_not_below_it() {
    // UIDs 21..=70 are older; 71..=120 are dated exactly at the tail, and
    // sort above rows already shown there (strictly before counts).
    let (mut server, mut store, inbox) = windowed_with(&[(50, 60 * DAY)]).await;
    for n in 0..50 {
        server.deliver(
            "INBOX",
            &eml_undated(&format!("tie{n}")),
            NOW_SECS - 40 * DAY,
        );
    }
    let tail = Some(NOW_SECS - 40 * DAY);
    let first = older((&mut server, &mut store, inbox), None, tail).await;
    assert_eq!(first.fetched.len(), 100);
}

#[tokio::test]
async fn an_outlier_in_the_cache_does_not_decide() {
    // UIDs 21..=70 at 60 days, 71..=120 at 35 days, then UID 121 dated
    // 1970 (an outlier).
    let (mut server, mut store, inbox) = windowed_with(&[(50, 60 * DAY), (50, 35 * DAY)]).await;
    server.deliver("INBOX", &eml_undated("outlier"), 1);
    let first = older((&mut server, &mut store, inbox), None, Some(NOW_SECS)).await;
    // The outlier is cached now, so a cache-wide floor would be 1970.
    assert_eq!(first.fetched.len(), 50);
    // The caller's tail is the 35-day mail's; the next batch (UIDs 22..=71,
    // 49 of them 60 days old) gets below it and ends the call.
    let before = fetches(&server);
    let second = older(
        (&mut server, &mut store, inbox),
        Some(first.kept()),
        Some(NOW_SECS - 35 * DAY),
    )
    .await;
    assert_eq!(second.fetched.len(), 50);
    assert_eq!(fetches(&server) - before, 1);
}

#[tokio::test]
async fn one_outlier_in_a_batch_does_not_end_the_call() {
    // UIDs 21..=70 at 60 days; 71..=119 at 35 days, and UID 120 dated 1970:
    // that batch has one row below the 40-day tail and 49 above it.
    let (mut server, mut store, inbox) = windowed_with(&[(50, 60 * DAY), (49, 35 * DAY)]).await;
    server.deliver("INBOX", &eml_undated("outlier"), 1);
    let tail = Some(NOW_SECS - 40 * DAY);
    let first = older((&mut server, &mut store, inbox), None, tail).await;
    // One row below is not enough: the call goes on to UIDs 21..=70.
    assert_eq!(first.fetched.len(), 100);
    assert!(first.exhausted);
}

#[tokio::test]
async fn a_failure_after_the_first_batch_keeps_what_was_stored() {
    let (mut server, mut store, inbox) = windowed_with(&[(50, 60 * DAY), (50, 35 * DAY)]).await;
    server.fail_after(
        "fetch_summaries",
        1,
        envio_net::NetError::Io(std::io::Error::other("reset")),
    );
    // Batch one (UIDs 71..=120, 35 days) is above the 40-day tail: the
    // call goes on, and the second batch fails.
    let tail = Some(NOW_SECS - 40 * DAY);
    let result = older((&mut server, &mut store, inbox), None, tail).await;
    assert_eq!(result.fetched.len(), 50);
    assert!(!result.exhausted);
    assert_eq!(store.uids("INBOX").len(), 70);
    // The failed batch is not planned: the next call fetches it.
    let next = older((&mut server, &mut store, inbox), Some(result.kept()), tail).await;
    assert_eq!(next.fetched.len(), 50);
}

/// A batch of 50 with `below` rows dated before the 40-day tail and the
/// rest after it (UIDs 21..=70 are 62 days old and wait behind it).
async fn fetched_with_below(below: i64) -> usize {
    let (mut server, mut store, inbox) =
        windowed_with(&[(50, 62 * DAY), (50 - below, 35 * DAY), (below, 60 * DAY)]).await;
    let tail = Some(NOW_SECS - 40 * DAY);
    older((&mut server, &mut store, inbox), None, tail)
        .await
        .fetched
        .len()
}

#[tokio::test]
async fn ten_rows_below_the_tail_end_the_call_and_nine_do_not() {
    // Nine below: the call goes on to the 62-day mail.
    assert_eq!(fetched_with_below(9).await, 100);
    // Ten below: it stops after the batch.
    assert_eq!(fetched_with_below(10).await, 50);
}

#[tokio::test]
async fn a_call_with_a_tail_ends_when_the_server_is_exhausted() {
    // Five messages, all above the tail: fewer than ten can never be below
    // it, so only exhaustion ends the call.
    let (mut server, mut store, inbox) = windowed_with(&[(5, 35 * DAY)]).await;
    let before = fetches(&server);
    let tail = Some(NOW_SECS - 40 * DAY);
    let first = older((&mut server, &mut store, inbox), None, tail).await;
    assert_eq!(first.fetched.len(), 5);
    assert!(first.exhausted);
    assert_eq!(fetches(&server) - before, 1);
}

#[tokio::test]
async fn with_a_tail_a_batch_of_deleted_mail_does_not_end_the_call() {
    // UIDs 71..=120 (the first batch) are \Deleted; 21..=70 are dated
    // below the tail.
    let (mut server, mut store, inbox) = windowed_with(&[(50, 60 * DAY), (50, 35 * DAY)]).await;
    for uid in 71..=120 {
        server.set_flags("INBOX", uid, MessageFlags::from_flags([Flag::Deleted]));
    }
    let before = fetches(&server);
    let tail = Some(NOW_SECS - 40 * DAY);
    let first = older((&mut server, &mut store, inbox), None, tail).await;
    assert_eq!(first.fetched.len(), 50);
    assert!(first.exhausted);
    assert_eq!(fetches(&server) - before, 2);
}
