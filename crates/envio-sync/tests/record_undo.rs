//! User actions apply to the cache at once, survive a sync, and undo.

mod support;

use envio_store::{ActionKind, BodyState, Flags, MessageRowId};
use envio_sync::driver::{fetch_body, record, sync_folder, sync_folders, undo};
use support::fake::{FakeServer, NOW_MS};
use support::local::{ACCOUNT, Store};

async fn synced(ids: &[&str]) -> (FakeServer, Store) {
    let mut server = FakeServer::new();
    for id in ids {
        server.deliver_id("INBOX", id);
    }
    let mut store = Store::new();
    sync(&mut server, &mut store).await;
    (server, store)
}

async fn sync(server: &mut FakeServer, store: &mut Store) {
    sync_folders(server, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    let inbox = store.folder("INBOX").id;
    sync_folder(server, &mut store.local(), inbox, NOW_MS)
        .await
        .unwrap();
}

fn row(store: &Store, uid: u32) -> MessageRowId {
    let inbox = store.folder("INBOX").id;
    store.db.message_at(inbox, uid).unwrap().unwrap().id
}

fn read() -> ActionKind {
    ActionKind::SetFlags {
        add: Flags::SEEN,
        remove: Flags::empty(),
    }
}

#[tokio::test]
async fn a_flag_change_shows_at_once_and_survives_a_sync() {
    let (mut server, mut store) = synced(&["a"]).await;
    let id = row(&store, 1);
    record(&mut store.local(), id, read(), NOW_MS).unwrap();
    assert_eq!(store.db.message(id).unwrap().flags, Flags::SEEN);
    let action = store.journal.pending().next().unwrap().clone();
    assert_eq!(action.target.uid, 1);
    assert_eq!(action.target.uidvalidity, 7);
    assert_eq!(action.target.message_id.as_deref(), Some("<a@example.org>"));
    sync(&mut server, &mut store).await;
    assert_eq!(store.db.message(row(&store, 1)).unwrap().flags, Flags::SEEN);
}

#[tokio::test]
async fn undo_puts_the_flags_back_and_drops_the_action() {
    let (_server, mut store) = synced(&["a"]).await;
    let id = row(&store, 1);
    let taken = record(&mut store.local(), id, read(), NOW_MS).unwrap();
    assert!(undo(&mut store.local(), &taken).unwrap());
    assert_eq!(store.db.message(id).unwrap().flags, Flags::empty());
    assert_eq!(store.journal.pending().count(), 0);
    assert!(!undo(&mut store.local(), &taken).unwrap(), "only once");
}

#[tokio::test]
async fn a_deleted_message_stays_hidden_until_undone() {
    let (mut server, mut store) = synced(&["a", "b"]).await;
    let id = row(&store, 1);
    fetch_body(&mut server, &mut store.local(), id)
        .await
        .unwrap();
    let taken = record(&mut store.local(), id, ActionKind::Delete, NOW_MS).unwrap();
    assert_eq!(store.subjects("INBOX"), ["About b"]);
    sync(&mut server, &mut store).await;
    assert_eq!(
        store.subjects("INBOX"),
        ["About b"],
        "sync doesn't bring it back"
    );
    assert!(undo(&mut store.local(), &taken).unwrap());
    let back = store.db.message(row(&store, 1)).unwrap();
    assert_eq!(back.envelope.subject, "About a");
    assert_eq!(back.body, BodyState::Downloaded);
    assert_eq!(back.preview, "Hello about a.");
}

#[tokio::test]
async fn undo_after_a_uidvalidity_reset_leaves_the_new_message_alone() {
    let (mut server, mut store) = synced(&["a", "b"]).await;
    let id = row(&store, 1);
    let taken = record(&mut store.local(), id, ActionKind::Delete, NOW_MS).unwrap();
    // The server renumbers; uid 1 is now a different message.
    server.folders.get_mut("INBOX").unwrap().messages.remove(&1);
    server.renumber("INBOX");
    sync(&mut server, &mut store).await;
    assert_eq!(store.subjects("INBOX"), ["About b"]);
    assert_eq!(store.uids("INBOX"), [1]);
    assert!(undo(&mut store.local(), &taken).unwrap());
    assert_eq!(store.subjects("INBOX"), ["About b"], "not overwritten");
    assert_eq!(store.journal.pending().count(), 0, "the action is undone");
}

#[tokio::test]
async fn undoing_a_flag_change_makes_the_next_sync_check_every_flag() {
    let (_server, mut store) = synced(&["a"]).await;
    assert!(store.folder("INBOX").sync.unwrap().highestmodseq.is_some());
    let id = row(&store, 1);
    let taken = record(&mut store.local(), id, read(), NOW_MS).unwrap();
    assert!(undo(&mut store.local(), &taken).unwrap());
    let sync = store.folder("INBOX").sync.unwrap();
    assert_eq!(sync.highestmodseq, None);
    assert_eq!(sync.uidvalidity, 7, "the rest of the state stays");
}

#[tokio::test]
async fn undo_into_a_folder_that_has_gone_still_succeeds() {
    let mut server = FakeServer::new();
    server.add_folder("Old", None);
    server.deliver_id("Old", "a");
    let mut store = Store::new();
    sync_folders(&mut server, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    let old = store.folder("Old").id;
    sync_folder(&mut server, &mut store.local(), old, NOW_MS)
        .await
        .unwrap();
    let id = store.db.message_at(old, 1).unwrap().unwrap().id;
    let taken = record(&mut store.local(), id, ActionKind::Delete, NOW_MS).unwrap();
    server.folders.remove("Old");
    sync_folders(&mut server, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    assert!(store.db.folder_by_path(ACCOUNT, "Old").unwrap().is_none());
    assert!(undo(&mut store.local(), &taken).unwrap());
    assert_eq!(store.journal.pending().count(), 0);
}
