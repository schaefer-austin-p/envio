//! A big folder is synced in parts of at most `BATCHES_PER_STEP` batches.

mod support;

use envio_sync::driver::{BATCHES_PER_STEP, Synced, fetch_body, sync_folder, sync_folders};
use support::fake::{FakeServer, NOW_MS};
use support::local::{ACCOUNT, Store};

async fn step(server: &mut FakeServer, store: &mut Store) -> Synced {
    sync_folders(server, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    let inbox = store.folder("INBOX").id;
    sync_folder(server, &mut store.local(), inbox, NOW_MS)
        .await
        .unwrap()
}

fn big_inbox(count: usize) -> FakeServer {
    let mut server = FakeServer::new();
    for n in 0..count {
        server.deliver_id("INBOX", &format!("m{n}"));
    }
    server
}

#[tokio::test]
async fn a_first_sync_of_250_messages_takes_three_steps() {
    let mut server = big_inbox(250);
    let mut store = Store::new();
    let mut steps = Vec::new();
    loop {
        let synced = step(&mut server, &mut store).await;
        steps.push((synced.fetched, synced.more));
        if !synced.more {
            break;
        }
        assert!(steps.len() < 10, "never ends");
    }
    assert_eq!(BATCHES_PER_STEP, 2);
    assert_eq!(steps, [(100, true), (100, true), (50, false)]);
    assert_eq!(store.uids("INBOX"), (1..=250).collect::<Vec<_>>());
}

#[tokio::test]
async fn parts_fetch_each_message_once() {
    let mut server = big_inbox(250);
    let mut store = Store::new();
    while step(&mut server, &mut store).await.more {}
    let mut asked: Vec<u32> = Vec::new();
    for call in server
        .calls
        .iter()
        .filter(|call| call.starts_with("fetch_summaries"))
    {
        let list = call
            .trim_start_matches("fetch_summaries [")
            .trim_end_matches(']');
        asked.extend(list.split(", ").map(|uid| uid.parse::<u32>().unwrap()));
    }
    asked.sort_unstable();
    assert_eq!(asked, (1..=250).collect::<Vec<_>>());
}

#[tokio::test]
async fn finish_is_written_only_after_the_last_part() {
    let mut server = big_inbox(250);
    let mut store = Store::new();
    for _ in 0..2 {
        assert!(step(&mut server, &mut store).await.more);
        let state = store.folder("INBOX").sync.unwrap();
        assert_eq!((state.uidvalidity, state.uidnext), (7, 0), "not finished");
    }
    assert!(!step(&mut server, &mut store).await.more);
    assert_eq!(store.folder("INBOX").sync.unwrap().uidnext, 251);
}

#[tokio::test]
async fn a_continuation_does_not_reset_the_folder() {
    let mut server = big_inbox(250);
    let mut store = Store::new();
    step(&mut server, &mut store).await;
    let inbox = store.folder("INBOX").id;
    let kept = store.db.message_at(inbox, 250).unwrap().unwrap();
    fetch_body(&mut server, &mut store.local(), kept.id)
        .await
        .unwrap();
    let body_dir = store.dir.path().join("bodies").join(inbox.0.to_string());
    assert!(body_dir.exists());
    let synced = step(&mut server, &mut store).await;
    assert_eq!((synced.removed, synced.fetched), (0, 100));
    assert!(body_dir.exists(), "no reset: the body files stay");
    assert_eq!(
        store.db.message_at(inbox, 250).unwrap().unwrap().id,
        kept.id
    );
    assert_eq!(store.uids("INBOX").len(), 200);
    assert_eq!(store.folder("INBOX").sync.unwrap().uidvalidity, 7);
}

#[tokio::test]
async fn a_small_folder_is_done_in_one_step() {
    let mut server = big_inbox(100);
    let mut store = Store::new();
    let synced = step(&mut server, &mut store).await;
    assert_eq!((synced.fetched, synced.more), (100, false));
    assert_eq!(store.folder("INBOX").sync.unwrap().uidnext, 101);
}
