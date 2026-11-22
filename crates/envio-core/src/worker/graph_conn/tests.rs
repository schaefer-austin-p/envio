//! A Graph account's connection: what `Worker::backend` and `begin_open`
//! make of it. No network: the client is built but never asked.

use std::cell::RefCell;
use std::rc::Rc;

use envio_net::Roots;
use envio_store::AccountSettings;

use crate::health::AccountStatus;
use crate::problem::Problem;
use crate::worker::backend::Backend;
use crate::worker::conn::{Conn, Live};
use crate::worker::testing::{Sink, into_graph, open, seed};
use crate::worker::token::TokenCells;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn a_graph_account_is_given_a_graph_backend() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut worker = open(&config, &sink);
    into_graph(&mut worker, &seeded.account);
    let backend = runtime().block_on(worker.backend(&seeded.account));
    assert!(matches!(backend, Ok(Backend::Graph(_))));
}

#[test]
fn a_paused_graph_account_is_given_no_backend() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut worker = open(&config, &sink);
    into_graph(&mut worker, &seeded.account);
    worker.live.get_mut(&seeded.account).unwrap().health.status = AccountStatus::SignInFailed;
    let backend = runtime().block_on(worker.backend(&seeded.account));
    assert!(matches!(backend, Err(Problem::SignIn)));
}

#[test]
fn a_graph_account_with_no_graph_provider_is_a_sign_in_problem() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut worker = open(&config, &sink);
    into_graph(&mut worker, &seeded.account);
    worker.store.token_cells = TokenCells::new(
        Vec::new(),
        Roots::empty(),
        Rc::new(RefCell::new(
            crate::oauth_tokens::OAuthTokens::open(dir.path()).unwrap(),
        )),
    );
    crate::cause::take_seen();
    let backend = runtime().block_on(worker.backend(&seeded.account));
    assert!(matches!(backend, Err(Problem::SignIn)));
    let seen = crate::cause::take_seen();
    assert!(seen[0].contains(crate::cause::NO_PROVIDER), "{seen:?}");
}

#[test]
fn a_graph_account_starts_without_idle() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut worker = open(&config, &sink);
    let imap = worker.store.accounts.get(&seeded.account).unwrap().clone();
    assert_eq!(Live::for_account(&imap).idle_capable, None);
    let settings = AccountSettings {
        backend: envio_store::BackendKind::Graph,
        imap: None,
        smtp: None,
        ..imap.settings.clone()
    };
    worker
        .store
        .accounts
        .update(&seeded.account, settings)
        .unwrap();
    let graph = worker.store.accounts.get(&seeded.account).unwrap();
    assert_eq!(Live::for_account(graph).idle_capable, Some(false));
}

#[test]
fn a_graph_account_has_no_idle_to_open() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut worker = open(&config, &sink);
    into_graph(&mut worker, &seeded.account);
    // An IMAP login would fail here: the account has no IMAP server.
    assert!(worker.begin_open(&seeded.account, i64::MAX).is_ok());
    assert!(matches!(worker.live[&seeded.account].conn, Conn::Closed));
}
