//! A Graph `Backend` hands every `MailServer` hook the sync steps rely on to
//! its `GraphServer` (#415): the caps, `refresh`, `commit_folders` and
//! `commit_folder`. Each test sees the effect on a mock Graph server or in
//! the uid map, so a hook that does nothing for Graph fails it.

use tokio::runtime::Runtime;

use envio_net::graph::mock::{Action, Server};
use envio_store::{FolderId, GraphMap};
use envio_sync::driver::sync_folders;
use envio_sync::server::MailServer;

use crate::worker::backend::Backend;
use crate::worker::testing::{Sink, into_graph, open, seed};
use crate::worker::{Worker, graph_conn::TEST_HOST};

const FOLDERS: &str = r#"{"value":[{"id":"F-INBOX","displayName":"Inbox","parentFolderId":"ROOT","wellKnownName":"inbox","childFolderCount":0}]}"#;
const DELTA: &str = r#"{"@odata.deltaLink":"https://graph.microsoft.com/v1.0/me/mailFolders('F')/messages/delta?$token=d1","value":[]}"#;

/// What a test sees of the Graph account: the worker's cache and the map.
struct Seen {
    server: Server,
    map: GraphMap,
    inbox: FolderId,
}

/// A Graph account whose folders were listed (and committed) through its
/// `Backend`, then `steps` run on the same backend.
fn run(steps: impl FnOnce(&mut Worker<'_>, &mut Backend, FolderId, &Runtime)) -> Seen {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let server = runtime.block_on(Server::start(vec![
        Action::json(200, FOLDERS),
        Action::json(200, DELTA),
    ]));
    TEST_HOST.with(|h| *h.borrow_mut() = Some((server.port(), server.roots())));
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut worker = open(&config, &sink);
    into_graph(&mut worker, &seeded.account);
    let mut backend = runtime.block_on(worker.backend(&seeded.account)).unwrap();
    assert!(matches!(backend, Backend::Graph(_)));
    runtime
        .block_on(sync_folders(
            &mut backend,
            &mut worker.store.local(),
            &seeded.account,
        ))
        .unwrap();
    let inbox = worker
        .store
        .db
        .folders(&seeded.account)
        .unwrap()
        .into_iter()
        .find(|f| f.path == "INBOX")
        .unwrap()
        .id;
    steps(&mut worker, &mut backend, inbox, &runtime);
    let map = GraphMap::open(&worker.store.paths.db()).unwrap();
    // `server` outlives the runtime's work; its requests are all in.
    Seen { server, map, inbox }
}

fn refresh(backend: &mut Backend, runtime: &Runtime) {
    runtime.block_on(backend.refresh("INBOX")).unwrap();
}

#[test]
fn a_graph_backend_reports_the_graph_caps() {
    let seen = run(|_, backend, _, _| {
        let caps = MailServer::caps(backend);
        assert!(caps.no_partial);
    });
    drop(seen);
}

#[test]
fn commit_folders_reaches_the_graph_server() {
    let seen = run(|_, _, _, _| {});
    let folder = seen.map.folder(seen.inbox).unwrap().unwrap();
    assert_eq!(folder.graph_id, "F-INBOX");
}

#[test]
fn refresh_reaches_the_graph_server() {
    let seen = run(|_, backend, _, rt| refresh(backend, rt));
    let reads = seen
        .server
        .requests()
        .into_iter()
        .filter(|r| r.path.contains("/delta"))
        .count();
    assert_eq!(reads, 1);
}

#[test]
fn commit_folder_reaches_the_graph_server() {
    let seen = run(|worker, backend, inbox, rt| {
        refresh(backend, rt);
        worker
            .store
            .db
            .step(|db| MailServer::commit_folder(backend, db, inbox))
            .unwrap();
    });
    let folder = seen.map.folder(seen.inbox).unwrap().unwrap();
    assert!(
        folder.delta_link.ends_with("token=d1"),
        "{}",
        folder.delta_link
    );
}
