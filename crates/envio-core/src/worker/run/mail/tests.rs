//! The Drafts step over an in-memory server: when the list is told.

use std::collections::VecDeque;

use envio_net::imap::{Caps, Flag, FlagUpdate, Folder, FolderState, Summary};
use envio_net::{NetError, Result};
use envio_store::{Draft, FolderRole, RefusalKind, SendState};

use super::*;
use crate::worker::testing::{Seeded, Sink, into_graph, open, seed};

/// Answers SELECT and LIST; APPEND and DELETE follow a script (OK when it
/// runs out); everything else is never asked.
#[derive(Default)]
struct Scripted {
    appends: VecDeque<NetError>,
    delete_error: Option<String>,
    appended: usize,
    deletes: usize,
    /// Sent has no copy of anything.
    miss: bool,
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
        Ok(if self.miss { vec![] } else { vec![1] })
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

/// An unconfirmed send is filed on a server with no copy in Sent: whether
/// the user is asked at once, and what the Outbox says of the next look
/// (tries, not-before).
fn file_unconfirmed(graph: bool) -> (bool, (u32, i64)) {
    let r = rig();
    let account = r.seeded.account.clone();
    let mut w = opened(&r);
    w.store
        .db
        .upsert_folder(&account, "Sent", FolderRole::Sent)
        .unwrap();
    if graph {
        into_graph(&mut w, &account);
    }
    let new = envio_store::NewOutboxItem {
        send_after_ms: 0,
        account: account.clone(),
        message_id: "<a@example.org>".into(),
        queued_at_ms: 1,
        from: "me@example.org".into(),
        recipients: vec!["ada@example.org".into()],
    };
    let id = w
        .store
        .outbox
        .enqueue(new, b"Subject: x\r\n\r\nhi")
        .unwrap()
        .id;
    // Seeding only (the grep test allows one `set_state` call).
    let seed_state = envio_store::Outbox::set_state;
    seed_state(&w.store.outbox, &id, SendState::MayHaveBeenSent).unwrap();
    let mut server = Scripted {
        miss: true,
        ..Scripted::default()
    };
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(file(&mut w, &account, &mut server))
        .unwrap();
    let items = w.store.outbox.items().unwrap();
    let meta = &items[0].meta;
    (
        w.asked.contains(&id),
        (meta.resolve_tries, meta.resolve_after_ms),
    )
}

#[test]
fn a_graph_miss_in_sent_is_looked_for_again_before_the_user_is_asked() {
    let (asked, (tries, after)) = file_unconfirmed(true);
    assert!(!asked);
    assert_eq!(tries, 1);
    assert!(after > 0, "the next look has a time");
}

/// An IMAP Outlook account at step 1 of the move, with one accepted item
/// whose Sent copy is refused (quota) every time.
fn refused_filing_while_moving(w: &mut Worker<'_>, account: &str, runs: usize) {
    w.store
        .db
        .upsert_folder(account, "Sent", FolderRole::Sent)
        .unwrap();
    let mut settings = w.store.accounts.get(account).unwrap().settings.clone();
    settings.auth = Some("microsoft".into());
    settings.migration = Some(envio_store::Migration {
        step: 1,
        failures: 0,
    });
    w.store.accounts.update(account, settings).unwrap();
    let new = envio_store::NewOutboxItem {
        send_after_ms: 0,
        account: account.into(),
        message_id: "<a@example.org>".into(),
        queued_at_ms: 1,
        from: "me@example.org".into(),
        recipients: vec!["ada@example.org".into()],
    };
    let id = w
        .store
        .outbox
        .enqueue(new, b"Subject: x\r\n\r\nhi")
        .unwrap()
        .id;
    let seed_state = envio_store::Outbox::set_state;
    seed_state(&w.store.outbox, &id, SendState::Accepted).unwrap();
    for _ in 0..runs {
        let mut server = Scripted {
            miss: true,
            appends: VecDeque::from([quota()]),
            ..Scripted::default()
        };
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(file(w, account, &mut server))
            .unwrap();
    }
}

#[test]
fn a_refused_sent_copy_counts_as_a_failed_run_of_the_move() {
    let r = rig();
    let account = r.seeded.account.clone();
    let mut w = opened(&r);
    refused_filing_while_moving(&mut w, &account, 2);
    let m = w.store.accounts.get(&account).unwrap().settings.migration;
    assert_eq!(m.unwrap().failures, 2);
}

#[test]
fn a_refused_sent_copy_does_not_count_while_actions_are_pending() {
    let r = rig();
    let account = r.seeded.account.clone();
    let mut w = opened(&r);
    let target = envio_store::MessageRef {
        account: account.clone(),
        folder: "INBOX".into(),
        uidvalidity: 1,
        uid: 1,
        message_id: None,
    };
    w.store
        .journal
        .push(1, target, envio_store::ActionKind::Delete)
        .unwrap();
    refused_filing_while_moving(&mut w, &account, 3);
    let m = w.store.accounts.get(&account).unwrap().settings.migration;
    assert_eq!(m.unwrap().failures, 0, "Replay owns the counter");
}

#[test]
fn full_runs_between_refused_filings_do_not_hide_a_stuck_move() {
    let r = rig();
    let account = r.seeded.account.clone();
    let mut w = opened(&r);
    refused_filing_while_moving(&mut w, &account, 1);
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    for _ in 0..2 {
        // A full run plans a Replay with nothing to do.
        rt.block_on(super::super::sync::replay(
            &mut w,
            &account,
            &mut Scripted::default(),
        ))
        .unwrap();
        let mut server = Scripted {
            miss: true,
            appends: VecDeque::from([quota()]),
            ..Scripted::default()
        };
        rt.block_on(file(&mut w, &account, &mut server)).unwrap();
    }
    let m = w.store.accounts.get(&account).unwrap().settings.migration;
    assert_eq!(m.unwrap().failures, 3);
    w.migrate_all(crate::clock::now_ms());
    let stuck = r.sink.events().into_iter().rev().find_map(|e| match e {
        Event::Migration { state: Some(s), .. } => Some(s),
        _ => None,
    });
    let stuck = stuck.unwrap();
    assert_eq!(stuck.stage, crate::api::view::MigrationStage::Stuck);
}

#[test]
fn an_item_that_cannot_be_filed_ends_stuck_and_switch_anyway_moves_on() {
    let r = rig();
    let account = r.seeded.account.clone();
    let mut w = opened(&r);
    refused_filing_while_moving(&mut w, &account, 3);
    w.migrate_all(crate::clock::now_ms());
    let stuck = r
        .sink
        .events()
        .into_iter()
        .rev()
        .find_map(|e| match e {
            Event::Migration { state: Some(s), .. } => Some(s),
            _ => None,
        })
        .unwrap();
    assert_eq!(stuck.stage, crate::api::view::MigrationStage::Stuck);
    assert!(stuck.can_switch, "Switch anyway is offered");
    assert!(
        !stuck.hint.contains("drops"),
        "nothing is dropped: the mail was sent, only its Sent copy is missing: {}",
        stuck.hint
    );
    w.switch_anyway(&account);
    let settings = &w.store.accounts.get(&account).unwrap().settings;
    assert_eq!(settings.backend, BackendKind::Graph);
    assert!(settings.migration.is_none());
    assert_eq!(settings.dropped_note, None, "no change was dropped");
}

#[test]
fn an_imap_miss_in_sent_asks_the_user_at_once() {
    let (asked, meta) = file_unconfirmed(false);
    assert!(asked);
    assert_eq!(meta, (0, 0));
}
