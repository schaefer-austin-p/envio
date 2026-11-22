//! A Graph account's token lives under the Graph provider kind, not the
//! IMAP one, on every path that saves, looks up or pauses it.

use envio_net::oauth::ProviderKind;
use envio_store::{AccountSettings, BackendKind};
use secrecy::{ExposeSecret, SecretString};

use crate::health::AccountStatus;
use crate::problem::Problem;
use crate::worker::testing::{Sink, into_graph, open, seed};

fn token(worker: &super::Worker<'_>, key: &str, kind: ProviderKind) -> Option<String> {
    worker
        .store
        .tokens
        .borrow()
        .get(key, kind)
        .map(|t| t.expose_secret().to_owned())
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn signing_in_again_saves_the_token_under_the_graph_kind() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut worker = open(&config, &sink);
    into_graph(&mut worker, &seeded.account);
    worker.sign_in_again(&seeded.account, SecretString::from("new"));
    assert_eq!(
        token(&worker, &seeded.account, ProviderKind::MicrosoftGraph).as_deref(),
        Some("new")
    );
    assert_eq!(
        token(&worker, &seeded.account, ProviderKind::Microsoft),
        None
    );
}

#[test]
fn a_graph_account_is_stranded_only_without_a_graph_token() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut worker = open(&config, &sink);
    into_graph(&mut worker, &seeded.account);
    let account = worker.store.accounts.get(&seeded.account).unwrap().clone();
    worker.live.remove(&seeded.account);
    assert!(!worker.stranded(&account), "its Graph token is there");
    // A token under the IMAP kind is not this account's.
    worker
        .store
        .tokens
        .borrow_mut()
        .remove(&seeded.account, ProviderKind::MicrosoftGraph)
        .unwrap();
    worker
        .store
        .tokens
        .borrow_mut()
        .set(
            &seeded.account,
            ProviderKind::Microsoft,
            SecretString::from("x"),
        )
        .unwrap();
    assert!(worker.stranded(&account));
}

#[test]
fn a_saved_graph_account_keeps_its_token_under_the_graph_kind() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut worker = open(&config, &sink);
    let base = worker
        .store
        .accounts
        .get(&seeded.account)
        .unwrap()
        .settings
        .clone();
    let settings = AccountSettings {
        backend: BackendKind::Graph,
        imap: None,
        smtp: None,
        email: "new@example.org".into(),
        auth: Some("microsoft".into()),
        ..base
    };
    let info = worker
        .save_oauth_account(settings, SecretString::from("rt2"))
        .unwrap();
    assert_eq!(
        token(&worker, &info.key, ProviderKind::MicrosoftGraph).as_deref(),
        Some("rt2")
    );
    assert_eq!(token(&worker, &info.key, ProviderKind::Microsoft), None);
}

/// The Graph cell of `account` refuses to refresh once it is paused.
fn graph_cell_is_paused(worker: &super::Worker<'_>, account: &str) -> bool {
    let handle = worker
        .store
        .token_cells
        .handle(account, ProviderKind::MicrosoftGraph)
        .unwrap();
    matches!(
        runtime().block_on(handle.access(true)),
        Err(Problem::SignIn)
    )
}

#[test]
fn a_status_report_pauses_the_graph_token_cell() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut worker = open(&config, &sink);
    into_graph(&mut worker, &seeded.account);
    assert!(!graph_cell_is_paused(&worker, &seeded.account));
    worker.live.get_mut(&seeded.account).unwrap().health.status = AccountStatus::SignInFailed;
    worker.report(&seeded.account, &AccountStatus::Ready);
    assert!(graph_cell_is_paused(&worker, &seeded.account));
}

#[test]
fn a_failed_step_pauses_the_graph_token_cell() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut worker = open(&config, &sink);
    into_graph(&mut worker, &seeded.account);
    // The cell exists before the pause, as it does once a connection ran.
    assert!(!graph_cell_is_paused(&worker, &seeded.account));
    crate::worker::run::failed(&mut worker, &seeded.account, "sync", &Problem::SignIn);
    assert!(graph_cell_is_paused(&worker, &seeded.account));
}
