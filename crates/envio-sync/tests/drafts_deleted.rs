//! A draft deleted (or moved to Trash) in the server's Drafts folder loses
//! its local file once the delete has reached the server, so Upload and
//! Edit cannot bring it back (#284). Until then Undo can still restore it.

mod support;

use std::collections::BTreeSet;

use envio_mime::Address;
use envio_net::imap::Role;
use envio_store::{ActionKind, Draft};
use envio_sync::UNDO_MS;
use envio_sync::driver::{
    DraftRun, forget_deleted_drafts, record, replay, sync_drafts, sync_folder, sync_folders,
};
use support::fake::{FakeServer, NOW_MS};
use support::local::{ACCOUNT, Store};

const LATER: i64 = NOW_MS + UNDO_MS;

fn draft(body: &str, updated_at_ms: i64) -> Draft {
    Draft {
        account: ACCOUNT.into(),
        to: vec!["bob@example.org".into()],
        subject: "Plans".into(),
        body: body.into(),
        updated_at_ms,
        ..Draft::default()
    }
}

/// A draft uploaded and cached in the Drafts folder: its id.
async fn uploaded(server: &mut FakeServer, store: &mut Store, body: &str) -> String {
    let id = store.drafts.create(&draft(body, 1)).unwrap();
    let from = Address::new(Some("Carol"), "carol@example.org");
    let open = BTreeSet::new();
    let run = DraftRun {
        account: ACCOUNT,
        from: &from,
        open: &open,
        now_ms: NOW_MS,
    };
    sync_drafts(server, &store.db, &store.drafts, &run)
        .await
        .unwrap();
    let drafts = store.folder("Drafts").id;
    sync_folder(server, &mut store.local(), drafts, NOW_MS, Some(30), None)
        .await
        .unwrap();
    id
}

async fn setup() -> (FakeServer, Store) {
    let mut server = FakeServer::new();
    server.add_folder("Drafts", Some(Role::Drafts));
    server.add_folder("Trash", Some(Role::Trash));
    let mut store = Store::new();
    sync_folders(&mut server, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    (server, store)
}

fn queue(store: &mut Store, kind: ActionKind) {
    let drafts = store.folder("Drafts").id;
    let row = store.db.message_at(drafts, 1).unwrap().unwrap().id;
    record(&mut store.local(), row, kind, NOW_MS).unwrap();
}

async fn replay_and_forget(
    server: &mut FakeServer,
    store: &mut Store,
    now_ms: i64,
    open: &BTreeSet<String>,
) -> Vec<String> {
    let replayed = replay(server, &mut store.local(), ACCOUNT, now_ms)
        .await
        .unwrap();
    forget_deleted_drafts(
        (&store.db, &store.drafts),
        ACCOUNT,
        &replayed.committed,
        open,
    )
    .unwrap()
}

#[tokio::test]
async fn a_delete_in_drafts_removes_the_local_draft_after_it_reached_the_server() {
    let (mut server, mut store) = setup().await;
    let id = uploaded(&mut server, &mut store, "text").await;
    queue(&mut store, ActionKind::Delete);
    // Inside the Undo window the draft is still there.
    let early = replay_and_forget(&mut server, &mut store, LATER - 1, &BTreeSet::new()).await;
    assert!(early.is_empty());
    assert!(store.drafts.load(&id).is_ok());
    let gone = replay_and_forget(&mut server, &mut store, LATER, &BTreeSet::new()).await;
    assert_eq!(gone, std::slice::from_ref(&id));
    assert!(store.drafts.load(&id).is_err());
    assert!(store.drafts.list().unwrap().is_empty());
}

#[tokio::test]
async fn a_move_to_trash_removes_the_local_draft_too() {
    let (mut server, mut store) = setup().await;
    let id = uploaded(&mut server, &mut store, "text").await;
    queue(&mut store, ActionKind::Move { to: "Trash".into() });
    let gone = replay_and_forget(&mut server, &mut store, LATER, &BTreeSet::new()).await;
    assert_eq!(gone, std::slice::from_ref(&id));
    assert!(store.drafts.load(&id).is_err());
}

#[tokio::test]
async fn a_draft_with_newer_unsent_edits_survives_the_delete_of_its_old_copy() {
    let (mut server, mut store) = setup().await;
    let id = uploaded(&mut server, &mut store, "text").await;
    store.drafts.save(&id, &draft("newer text", 2)).unwrap();
    queue(&mut store, ActionKind::Delete);
    let gone = replay_and_forget(&mut server, &mut store, LATER, &BTreeSet::new()).await;
    assert!(gone.is_empty());
    assert_eq!(store.drafts.load(&id).unwrap().body, "newer text");
}
