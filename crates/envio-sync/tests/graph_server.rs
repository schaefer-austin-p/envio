//! `GraphServer` on envio-net's scripted Graph server and a file-backed
//! cache: folders, refresh, select and the trait hooks.

mod support;

use envio_net::NetError;
use envio_net::graph::mock::{Action, Server};
use envio_net::imap::Role;
use envio_store::GraphMap;
use envio_sync::ErrorKind;
use envio_sync::SyncError;
use envio_sync::driver::sync_folders;
use envio_sync::server::MailServer;
use support::graph::{folder_walk, graph_server, msg, page, requests_with};
use support::local::{ACCOUNT, Store};

const HIGH: u32 = 1 << 31;

fn script(parts: Vec<Vec<Action>>) -> Vec<Action> {
    parts.into_iter().flatten().collect()
}

#[tokio::test]
async fn folder_paths_roles_and_filters() {
    let server = Server::start(folder_walk("F-INBOX")).await;
    let store = Store::on_file();
    let mut gs = graph_server(&server, &store.db_path);
    let folders = gs.list_folders().await.unwrap();
    let got: Vec<(&str, Option<Role>)> =
        folders.iter().map(|f| (f.path.as_str(), f.role)).collect();
    assert_eq!(
        got,
        [
            ("INBOX", Some(Role::Inbox)),
            ("Drafts", Some(Role::Drafts)),
            ("Sent Items", Some(Role::Sent)),
            ("A\u{2215}B\u{2024}c", None),
            ("INBOX/Child", None),
        ]
    );
    assert!(folders.iter().all(|f| f.selectable));
}

#[tokio::test]
async fn a_multi_call_sync_drains_the_delta_once() {
    let server = Server::start(script(vec![
        folder_walk("F-INBOX"),
        vec![
            page(&[msg("m1", "2026-11-21T10:00:00Z", false)], true, "p2"),
            page(&[msg("m2", "2026-11-21T11:00:00Z", false)], false, "d1"),
        ],
    ]))
    .await;
    let mut store = Store::on_file();
    let mut gs = graph_server(&server, &store.db_path);
    sync_folders(&mut gs, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    gs.refresh("INBOX").await.unwrap();
    gs.refresh("INBOX").await.unwrap();
    gs.refresh("INBOX").await.unwrap();
    assert_eq!(requests_with(&server, "/delta").len(), 2);
}

#[tokio::test]
async fn a_replayed_delta_after_a_crash_gives_the_same_uids() {
    let items = [
        msg("m1", "2026-11-21T10:00:00Z", false),
        msg("m2", "2026-11-21T09:00:00Z", true),
    ];
    let server = Server::start(script(vec![
        folder_walk("F-INBOX"),
        vec![page(&items, false, "d1")],
    ]))
    .await;
    let mut store = Store::on_file();
    let map = GraphMap::open(&store.db_path).unwrap();
    let mut gs = graph_server(&server, &store.db_path);
    sync_folders(&mut gs, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    let inbox = store.folder("INBOX").id;
    gs.refresh("INBOX").await.unwrap();
    let before = (
        map.uid_of(inbox, "m1").unwrap(),
        map.uid_of(inbox, "m2").unwrap(),
    );
    drop(gs); // no commit_folder: the pending delta is lost
    for a in folder_walk("F-INBOX") {
        server.push(a);
    }
    server.push(page(&items, false, "d1"));
    let mut gs = graph_server(&server, &store.db_path);
    gs.refresh("INBOX").await.unwrap();
    let after = (
        map.uid_of(inbox, "m1").unwrap(),
        map.uid_of(inbox, "m2").unwrap(),
    );
    assert_eq!(before, after);
    assert_eq!(map.count(inbox).unwrap(), 2);
    assert_eq!(map.folder(inbox).unwrap().unwrap().delta_link, "");
}

#[tokio::test]
async fn an_expired_link_resets_the_folder_and_select_shows_the_new_generation() {
    let server = Server::start(script(vec![
        folder_walk("F-INBOX"),
        vec![page(
            &[msg("m1", "2026-11-21T10:00:00Z", false)],
            false,
            "d1",
        )],
    ]))
    .await;
    let mut store = Store::on_file();
    let map = GraphMap::open(&store.db_path).unwrap();
    let mut gs = graph_server(&server, &store.db_path);
    sync_folders(&mut gs, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    let inbox = store.folder("INBOX").id;
    gs.refresh("INBOX").await.unwrap();
    store.db.step(|db| gs.commit_folder(db, inbox)).unwrap();
    let old = gs.select("INBOX").await.unwrap();
    assert_eq!(old.exists, 1);
    server.push(Action::json(
        410,
        r#"{"error":{"code":"syncStateNotFound"}}"#,
    ));
    server.push(page(&[], false, "d2"));
    gs.refresh("INBOX").await.unwrap();
    let new = gs.select("INBOX").await.unwrap();
    assert_ne!(new.uid_validity, old.uid_validity);
    assert_ne!(new.uid_validity, 0);
    assert_eq!((new.exists, new.uid_next), (0, HIGH));
    assert_eq!(map.uid_of(inbox, "m1").unwrap(), None);
    let reads = requests_with(&server, "/delta");
    assert!(reads[1].path.contains("token=d1") && !reads[2].path.contains("token"));
}

#[tokio::test]
async fn a_changed_graph_id_in_commit_folders_resets_the_folder() {
    let server = Server::start(script(vec![
        folder_walk("F-INBOX"),
        vec![page(
            &[msg("m1", "2026-11-21T10:00:00Z", false)],
            false,
            "d1",
        )],
        folder_walk("F-INBOX-2"),
    ]))
    .await;
    let mut store = Store::on_file();
    let map = GraphMap::open(&store.db_path).unwrap();
    let mut gs = graph_server(&server, &store.db_path);
    sync_folders(&mut gs, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    let inbox = store.folder("INBOX").id;
    gs.refresh("INBOX").await.unwrap();
    let old = map.folder(inbox).unwrap().unwrap();
    assert_eq!(old.graph_id, "F-INBOX");
    sync_folders(&mut gs, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    let now = map.folder(inbox).unwrap().unwrap();
    assert_eq!(now.graph_id, "F-INBOX-2");
    assert_ne!(now.generation, old.generation);
    assert_eq!(map.count(inbox).unwrap(), 0);
}

#[tokio::test]
async fn new_ids_get_uids_in_received_order_and_select_reports_them() {
    let items = [
        msg("old", "2026-11-21T09:00:00Z", false),
        msg("new", "2026-11-21T11:00:00Z", false),
        msg("mid", "2026-11-21T10:00:00Z", false),
    ];
    let server = Server::start(script(vec![
        folder_walk("F-INBOX"),
        vec![page(&items, false, "d1")],
    ]))
    .await;
    let mut store = Store::on_file();
    let map = GraphMap::open(&store.db_path).unwrap();
    let mut gs = graph_server(&server, &store.db_path);
    sync_folders(&mut gs, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    let inbox = store.folder("INBOX").id;
    gs.refresh("INBOX").await.unwrap();
    let uid = |id: &str| map.uid_of(inbox, id).unwrap().unwrap();
    assert_eq!(
        (uid("old"), uid("mid"), uid("new")),
        (HIGH, HIGH + 1, HIGH + 2)
    );
    let state = gs.select("INBOX").await.unwrap();
    assert_eq!((state.uid_next, state.exists), (HIGH + 3, 3));
    assert!(state.highest_modseq.is_some());
}

#[tokio::test]
async fn a_flag_only_change_keeps_the_rest_and_moves_the_modseq() {
    let server = Server::start(script(vec![
        folder_walk("F-INBOX"),
        vec![page(
            &[msg("m1", "2026-11-21T10:00:00Z", false)],
            false,
            "d1",
        )],
    ]))
    .await;
    let mut store = Store::on_file();
    let map = GraphMap::open(&store.db_path).unwrap();
    let mut gs = graph_server(&server, &store.db_path);
    sync_folders(&mut gs, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    let inbox = store.folder("INBOX").id;
    gs.refresh("INBOX").await.unwrap();
    store.db.step(|db| gs.commit_folder(db, inbox)).unwrap();
    let before = map.row(inbox, HIGH).unwrap().unwrap();
    server.push(page(&[r#"{"id":"m1","isRead":true}"#.into()], false, "d2"));
    gs.refresh("INBOX").await.unwrap();
    let after = map.row(inbox, HIGH).unwrap().unwrap();
    assert_eq!((before.flags, after.flags), (0, 1));
    assert_eq!(after.received_ms, before.received_ms);
    assert!(after.modseq > before.modseq);
}

#[tokio::test]
async fn a_folder_not_committed_yet_is_busy_and_an_unknown_one_is_refused() {
    let server = Server::start(folder_walk("F-INBOX")).await;
    let store = Store::on_file();
    let mut gs = graph_server(&server, &store.db_path);
    let err = gs.refresh("INBOX").await.unwrap_err();
    assert!(matches!(err, NetError::Busy(_)), "{err:?}");
    assert_eq!(SyncError::from(err).kind(), ErrorKind::Temporary);
    for a in folder_walk("F-INBOX") {
        server.push(a);
    }
    let err = gs.refresh("Nope").await.unwrap_err();
    assert!(matches!(err, NetError::Refused(_)), "{err:?}");
}

#[tokio::test]
async fn a_new_server_on_the_same_cache_selects_without_a_folder_sync() {
    let server = Server::start(folder_walk("F-INBOX")).await;
    let mut store = Store::on_file();
    let mut first = graph_server(&server, &store.db_path);
    sync_folders(&mut first, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    let inbox = store.folder("INBOX").id;
    let generation = GraphMap::open(&store.db_path)
        .unwrap()
        .folder(inbox)
        .unwrap()
        .unwrap()
        .generation;
    drop(first);
    for a in folder_walk("F-INBOX") {
        server.push(a);
    }
    let mut gs = graph_server(&server, &store.db_path);
    let state = gs.select("INBOX").await.unwrap();
    assert_eq!(state.uid_validity, generation);
}
