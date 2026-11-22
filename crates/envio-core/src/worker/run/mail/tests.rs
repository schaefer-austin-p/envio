//! The Drafts step over an in-memory server: when the list is told.

use std::collections::VecDeque;

use envio_net::imap::{Caps, Flag, FlagUpdate, Folder, FolderState, Summary};
use envio_net::{NetError, Result};
use envio_store::{Draft, FolderRole, RefusalKind, SendState};

use super::*;
use crate::worker::testing::{Seeded, Sink, open, seed};

/// Answers SELECT and LIST; APPEND and DELETE follow a script (OK when it
/// runs out); everything else is never asked.
#[derive(Default)]
struct Scripted {
    appends: VecDeque<NetError>,
    delete_error: Option<String>,
    appended: usize,
    deletes: usize,
}

impl MailServer for Scripted {
    fn caps(&self) -> Caps {
        Caps::default()
    }
    async fn list_folders(&mut self) -> Result<Vec<Folder>> {
        unreachable!()
    }
    async fn select(&mut self, _path: &str) -> Result<FolderState> {
        Ok(FolderState {
            uid_validity: 1,
            uid_next: 2,
            exists: 1,
            highest_modseq: None,
        })
    }
    async fn search_since(&mut self, _: i64) -> Result<Vec<u32>> {
        unreachable!()
    }
    async fn search_all(&mut self) -> Result<Vec<u32>> {
        unreachable!()
    }
    async fn search_message_id(&mut self, _: &str) -> Result<Vec<u32>> {
        Ok(vec![1])
    }
    async fn search_message_id_deep(&mut self, _: &str) -> Result<Vec<u32>> {
        Ok(vec![1])
    }
    async fn search_text(&mut self, _: &str) -> Result<Vec<u32>> {
        unreachable!()
    }
    async fn fetch_summaries(&mut self, _: &[u32]) -> Result<Vec<Summary>> {
        unreachable!()
    }
    async fn fetch_flags(&mut self, _: &[u32], _: Option<u64>) -> Result<Vec<FlagUpdate>> {
        unreachable!()
    }
    async fn fetch_body(&mut self, _: u32, _: usize) -> Result<Option<Vec<u8>>> {
        unreachable!()
    }
    async fn fetch_partial(&mut self, _: u32) -> Result<Option<Vec<u8>>> {
        unreachable!()
    }
    async fn fetch_section(&mut self, _: u32, _: &str, _: usize) -> Result<Option<Vec<u8>>> {
        unreachable!()
    }
    async fn store_flags(&mut self, _: &[u32], _: bool, _: &[Flag]) -> Result<()> {
        unreachable!()
    }
    async fn move_to(&mut self, _: &[u32], _: &str) -> Result<()> {
        unreachable!()
    }
    async fn delete(&mut self, _: &[u32]) -> Result<()> {
        self.deletes += 1;
        match &self.delete_error {
            Some(text) => Err(NetError::Refused(text.clone())),
            None => Ok(()),
        }
    }
    async fn append(&mut self, _: &str, _: &[Flag], _: &[u8]) -> Result<()> {
        self.appended += 1;
        match self.appends.pop_front() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

struct Rig {
    _dir: tempfile::TempDir,
    config: crate::core::Config,
    seeded: Seeded,
    sink: Sink,
}

fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    Rig {
        _dir: dir,
        config,
        seeded,
        sink: Sink::default(),
    }
}

fn draft(account: &str, subject: &str) -> Draft {
    Draft {
        account: account.into(),
        to: vec!["Bob <bob@example.org>".into()],
        subject: subject.into(),
        body: "body".into(),
        updated_at_ms: 5_000,
        ..Draft::default()
    }
}

fn opened<'s>(r: &'s Rig) -> Worker<'s> {
    let w = open(&r.config, &r.sink);
    w.store
        .db
        .upsert_folder(&r.seeded.account, "Drafts", FolderRole::Drafts)
        .unwrap();
    w
}

fn run(w: &mut Worker<'_>, r: &Rig, server: &mut Scripted) -> std::result::Result<(), Problem> {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(sync_drafts_on(w, &r.seeded.account, server))
}

fn told(r: &Rig) -> usize {
    r.sink
        .events()
        .iter()
        .filter(|e| matches!(e, Event::LocalDraftsChanged { .. }))
        .count()
}

fn quota() -> NetError {
    NetError::Refused("code: None, info: Some(\"[OVERQUOTA] full\")".into())
}

#[test]
fn a_refusal_tells_the_list_once_and_a_run_inside_the_wait_says_nothing() {
    let r = rig();
    let mut w = opened(&r);
    let id = w
        .store
        .drafts
        .create(&draft(&r.seeded.account, "a"))
        .unwrap();
    let mut server = Scripted::default();
    server.appends.push_back(quota());
    run(&mut w, &r, &mut server).unwrap();
    let stored = w.store.drafts.load(&id).unwrap().server;
    assert_eq!(stored.refusal, Some(RefusalKind::OverQuota));
    assert_eq!(told(&r), 1);
    // Not due again yet: nothing is asked and nothing changes.
    run(&mut w, &r, &mut server).unwrap();
    assert_eq!(server.appended, 1);
    assert_eq!(told(&r), 1);
}

#[test]
fn an_upload_tells_the_list() {
    let r = rig();
    let mut w = opened(&r);
    w.store
        .drafts
        .create(&draft(&r.seeded.account, "a"))
        .unwrap();
    run(&mut w, &r, &mut Scripted::default()).unwrap();
    assert_eq!(told(&r), 1);
}

#[test]
fn a_refusal_before_a_lost_connection_still_tells_the_list() {
    let r = rig();
    let mut w = opened(&r);
    for subject in ["a", "b"] {
        w.store
            .drafts
            .create(&draft(&r.seeded.account, subject))
            .unwrap();
    }
    let mut server = Scripted::default();
    server.appends.push_back(quota());
    server.appends.push_back(NetError::Io(std::io::Error::new(
        std::io::ErrorKind::ConnectionReset,
        "gone",
    )));
    assert!(run(&mut w, &r, &mut server).is_err());
    assert_eq!(told(&r), 1);
}

#[test]
fn a_refused_server_copy_delete_changes_nothing_a_row_shows() {
    let r = rig();
    let mut w = opened(&r);
    let id = w
        .store
        .drafts
        .create(&draft(&r.seeded.account, "a"))
        .unwrap();
    let copy = "<x@example.org>";
    assert!(w.store.drafts.mark_uploading(&id, copy).unwrap());
    assert!(w.store.drafts.mark_uploaded(&id, copy, 5_000).unwrap());
    w.store.drafts.delete(&id).unwrap();
    let mut server = Scripted {
        delete_error: Some("NO [NOPERM] read-only".into()),
        ..Scripted::default()
    };
    run(&mut w, &r, &mut server).unwrap();
    assert_eq!(server.deletes, 1, "the delete was tried and refused");
    assert_eq!(told(&r), 0);
}

/// A Graph account with one queued message, and a mock Graph server that
/// answers the send with `reply`.
fn graph_send(
    reply: envio_net::graph::mock::Action,
) -> (
    Rig,
    tokio::runtime::Runtime,
    envio_net::graph::mock::Server,
    String,
) {
    use envio_net::graph::mock::Server;

    let r = rig();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let server = runtime.block_on(Server::start(vec![reply]));
    crate::worker::graph_conn::TEST_HOST
        .with(|h| *h.borrow_mut() = Some((server.port(), server.roots())));
    let new = envio_store::NewOutboxItem {
        send_after_ms: 0,
        account: r.seeded.account.clone(),
        message_id: "<a@example.org>".into(),
        queued_at_ms: 1,
        from: "me@example.org".into(),
        recipients: vec!["ada@example.org".into()],
    };
    let w = open(&r.config, &r.sink);
    let id = w
        .store
        .outbox
        .enqueue(new, b"Subject: x\r\n\r\nhi")
        .unwrap()
        .id;
    drop(w);
    (r, runtime, server, id)
}

fn send_graph(
    r: &Rig,
    runtime: &tokio::runtime::Runtime,
    id: &str,
) -> (Worker<'static>, std::result::Result<(), Problem>, SendState) {
    let sink: &'static Sink = Box::leak(Box::default());
    let mut w = open(&r.config, sink);
    crate::worker::testing::into_graph(&mut w, &r.seeded.account);
    let result = runtime.block_on(send(&mut w, &r.seeded.account));
    let state = w
        .store
        .outbox
        .items()
        .unwrap()
        .into_iter()
        .find(|i| i.id == id)
        .unwrap()
        .meta
        .state;
    (w, result, state)
}

#[test]
fn a_graph_send_goes_through_the_graph_connection_and_leaves_it_in_place() {
    use envio_net::graph::mock::Action;
    let (r, runtime, server, id) = graph_send(Action::json(202, ""));
    let (w, result, state) = send_graph(&r, &runtime, &id);
    assert_eq!(result, Ok(()));
    assert_eq!(state, SendState::Accepted);
    assert_eq!(server.requests().len(), 1);
    assert!(matches!(
        w.live[&r.seeded.account].conn,
        Conn::Ready(Backend::Graph(_))
    ));
    assert!(w.live[&r.seeded.account].wants.file);
}

#[test]
fn an_unconfirmed_graph_send_is_never_requeued_and_the_connection_is_kept() {
    use envio_net::graph::mock::Action;
    let (r, runtime, server, id) = graph_send(Action::json(504, ""));
    let (w, result, state) = send_graph(&r, &runtime, &id);
    assert_eq!(result, Ok(()));
    assert_eq!(state, SendState::MayHaveBeenSent);
    assert_eq!(server.requests().len(), 1, "sent once, not retried");
    assert!(matches!(
        w.live[&r.seeded.account].conn,
        Conn::Ready(Backend::Graph(_))
    ));
}

#[test]
fn a_refused_graph_send_fails_the_item_and_keeps_the_connection() {
    use envio_net::graph::mock::Action;
    let body = r#"{"error":{"code":"ErrorInvalidRecipients","message":"no"}}"#;
    let (r, runtime, _server, id) = graph_send(Action::json(400, body));
    let (w, result, state) = send_graph(&r, &runtime, &id);
    assert_eq!(result, Ok(()));
    assert!(matches!(state, SendState::Failed { .. }), "{state:?}");
    assert!(matches!(
        w.live[&r.seeded.account].conn,
        Conn::Ready(Backend::Graph(_))
    ));
}

#[test]
fn graph_files_its_sent_mail_itself() {
    assert!(files_itself_for(BackendKind::Graph, ""));
    assert!(!files_itself_for(BackendKind::Imap, "smtp.example.org"));
    assert!(files_itself_for(BackendKind::Imap, "smtp.gmail.com"));
}
