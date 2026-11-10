//! A whole sync run, and removing an account with its unsent work.

mod support;

use envio_net::NetError;
use envio_net::imap::Role;
use envio_store::{ActionKind, Draft, Flags, NewOutboxItem, SendState};
use envio_sync::driver::{
    Unsent, fetch_body, has_due_mail, record, remove_account, sync_account, unsent,
};
use envio_sync::{ErrorKind, UNDO_MS};
use support::fake::{FakeServer, NOW_MS, eml};
use support::local::{ACCOUNT, Store};

const LATER: i64 = NOW_MS + UNDO_MS;

fn read() -> ActionKind {
    ActionKind::SetFlags {
        add: Flags::SEEN,
        remove: Flags::empty(),
    }
}

fn take(store: &mut Store, uid: u32, kind: ActionKind) {
    let inbox = store.folder("INBOX").id;
    let id = store.db.message_at(inbox, uid).unwrap().unwrap().id;
    record(&mut store.local(), id, kind, NOW_MS).unwrap();
}

fn queue(store: &Store, account: &str, id: &str) -> String {
    let item = NewOutboxItem {
        account: account.into(),
        message_id: format!("<{id}@example.org>"),
        queued_at_ms: NOW_MS,
        from: "carol@example.org".into(),
        recipients: vec!["dave@example.org".into()],
    };
    store.outbox.enqueue(item, &eml(id)).unwrap().id
}

async fn synced(server: &mut FakeServer) -> Store {
    server.add_folder("Archive", Some(Role::Archive));
    server.deliver_id("Archive", "old");
    server.deliver_id("INBOX", "a");
    let mut store = Store::new();
    sync_account(server, &mut store.local(), ACCOUNT, NOW_MS)
        .await
        .unwrap();
    store
}

#[tokio::test]
async fn a_run_replays_first_then_syncs_inbox_first() {
    let mut server = FakeServer::new();
    let mut store = synced(&mut server).await;
    assert_eq!(store.subjects("Archive"), ["About old"]);
    take(&mut store, 1, read());
    let run = sync_account(&mut server, &mut store.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    assert_eq!(run.replayed.done, 1);
    let order: Vec<_> = run.folders.iter().map(|(id, _)| *id).collect();
    assert_eq!(
        order,
        [store.folder("INBOX").id, store.folder("Archive").id]
    );
    let store_flag = server
        .calls
        .iter()
        .position(|c| c.starts_with("store_flags"));
    let first_search = server
        .calls
        .iter()
        .rposition(|c| c.starts_with("search_since"));
    assert!(store_flag < first_search);
    assert!(server.message("INBOX", 1).unwrap().flags.seen);
}

#[tokio::test]
async fn a_replay_that_stops_ends_the_run() {
    let mut server = FakeServer::new();
    let mut store = synced(&mut server).await;
    take(&mut store, 1, read());
    server.fail_on("store_flags", NetError::Timeout);
    let run = sync_account(&mut server, &mut store.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    assert_eq!(run.replayed.stopped, Some(ErrorKind::Temporary));
    assert!(run.folders.is_empty());
}

#[tokio::test]
async fn smtp_is_needed_only_for_queued_mail() {
    let mut server = FakeServer::new();
    let mut store = synced(&mut server).await;
    assert!(!has_due_mail(&store.local(), ACCOUNT, NOW_MS).unwrap());
    let id = queue(&store, ACCOUNT, "out");
    assert!(has_due_mail(&store.local(), ACCOUNT, NOW_MS).unwrap());
    store.outbox.set_state(&id, SendState::Accepted).unwrap();
    assert!(!has_due_mail(&store.local(), ACCOUNT, NOW_MS).unwrap());
}

#[tokio::test]
async fn removing_an_account_counts_then_drops_all_it_holds() {
    let mut server = FakeServer::new();
    let mut store = synced(&mut server).await;
    let inbox = store.folder("INBOX").id;
    let id = store.db.message_at(inbox, 1).unwrap().unwrap().id;
    fetch_body(&mut server, &mut store.local(), id)
        .await
        .unwrap();
    take(&mut store, 1, read());
    queue(&store, ACCOUNT, "out");
    let other = queue(&store, "1800", "theirs");
    let draft = Draft {
        account: ACCOUNT.into(),
        subject: "Half written".into(),
        ..Draft::default()
    };
    store.drafts.create(&draft).unwrap();
    let (local, drafts) = store.with_drafts();
    let counts = unsent(&local, drafts, ACCOUNT).unwrap();
    assert_eq!(
        counts,
        Unsent {
            outbox: 1,
            drafts: 1,
            actions: 1
        }
    );
    let (mut local, drafts) = store.with_drafts();
    remove_account(&mut local, drafts, ACCOUNT).unwrap();
    let (local, drafts) = store.with_drafts();
    assert_eq!(unsent(&local, drafts, ACCOUNT).unwrap(), Unsent::default());
    assert!(store.db.folders(ACCOUNT).unwrap().is_empty());
    assert!(
        !store
            .dir
            .path()
            .join("bodies")
            .join(inbox.0.to_string())
            .exists()
    );
    let left: Vec<String> = store
        .outbox
        .items()
        .unwrap()
        .into_iter()
        .map(|i| i.id)
        .collect();
    assert_eq!(left, [other], "another account's mail stays");
}

#[tokio::test]
async fn a_refused_folder_does_not_block_the_ones_after_it() {
    let mut server = FakeServer::new();
    server.add_folder("Alpha", None);
    server.add_folder("Beta", None);
    server.deliver_id("Beta", "late");
    let mut store = Store::new();
    server.fail_on_detail("select", "Alpha", NetError::Refused("no".into()));
    let run = sync_account(&mut server, &mut store.local(), ACCOUNT, NOW_MS)
        .await
        .unwrap();
    assert_eq!(run.refused, [store.folder("Alpha").id]);
    assert_eq!(run.stopped, None);
    assert_eq!(store.subjects("Beta"), ["About late"]);
}

#[tokio::test]
async fn a_timeout_in_a_folder_ends_the_run_and_keeps_the_replay() {
    let mut server = FakeServer::new();
    let mut store = synced(&mut server).await;
    take(&mut store, 1, read());
    server.fail_on_detail("select", "Archive", NetError::Timeout);
    let run = sync_account(&mut server, &mut store.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    assert_eq!(run.stopped, Some(ErrorKind::Temporary));
    assert_eq!(run.replayed.done, 1);
    assert!(run.refused.is_empty());
}
