//! "Update sign-in" (#342): an Outlook.com account on IMAP signs in for
//! Graph; only a personal tenant passes.

use envio_net::oauth::{CONSUMER_TENANT, ProviderKind};
use envio_store::{BackendKind, Migration};
use secrecy::SecretString;

use super::provider_tests::{ANNA, WORK, answer, config, outcomes, token_of, urls};
use crate::api::event::{OAuthFailure, OAuthOutcome};
use crate::worker::Worker;
use crate::worker::testing::{Sink, open};

/// The seeded account as an Outlook account on IMAP with its token.
fn imap_account(worker: &mut Worker<'_>) -> String {
    let key = worker.store.accounts.list()[0].key.clone();
    let mut settings = worker.store.accounts.get(&key).unwrap().settings.clone();
    settings.auth = Some("microsoft".into());
    settings.email = ANNA.into();
    worker.store.accounts.update(&key, settings).unwrap();
    worker
        .store
        .tokens
        .borrow_mut()
        .set(&key, ProviderKind::Microsoft, SecretString::from("imap-rt"))
        .unwrap();
    key
}

fn update(worker: &mut Worker<'_>, key: &str) {
    worker.start_oauth("", Some(key.into()), Some(ProviderKind::MicrosoftGraph));
}

#[test]
fn a_personal_tenant_keeps_both_tokens_and_starts_the_move() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    let key = imap_account(&mut worker);
    update(&mut worker, &key);
    assert!(urls(&sink)[0].contains("Mail.ReadWrite"));
    answer(
        &mut worker,
        &sink,
        super::provider_tests::tokens(ANNA, Some(CONSUMER_TENANT)),
    );
    assert_eq!(outcomes(&sink), vec![OAuthOutcome::Updated]);
    assert_eq!(
        token_of(&worker, &key, ProviderKind::MicrosoftGraph).as_deref(),
        Some("rt-new")
    );
    assert_eq!(
        token_of(&worker, &key, ProviderKind::Microsoft).as_deref(),
        Some("imap-rt"),
        "the IMAP token stays until the move ends"
    );
    let settings = &worker.store.accounts.get(&key).unwrap().settings;
    assert_eq!(settings.backend, BackendKind::Imap);
    assert_eq!(
        settings.migration,
        Some(Migration {
            step: 1,
            failures: 0
        })
    );
}

#[test]
fn a_work_tenant_stores_no_graph_token_and_the_account_stays_imap() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    let key = imap_account(&mut worker);
    update(&mut worker, &key);
    answer(
        &mut worker,
        &sink,
        super::provider_tests::tokens(ANNA, Some(WORK)),
    );
    assert_eq!(
        outcomes(&sink),
        vec![OAuthOutcome::Failed(OAuthFailure::NotPersonal)]
    );
    assert_eq!(token_of(&worker, &key, ProviderKind::MicrosoftGraph), None);
    let settings = &worker.store.accounts.get(&key).unwrap().settings;
    assert_eq!(settings.backend, BackendKind::Imap);
    assert!(settings.migration.is_none());
}

#[test]
fn a_token_without_a_tenant_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    let key = imap_account(&mut worker);
    update(&mut worker, &key);
    answer(&mut worker, &sink, super::provider_tests::tokens(ANNA, None));
    assert_eq!(
        outcomes(&sink),
        vec![OAuthOutcome::Failed(OAuthFailure::NoTenant)]
    );
    assert_eq!(token_of(&worker, &key, ProviderKind::MicrosoftGraph), None);
    assert!(
        worker
            .store
            .accounts
            .get(&key)
            .unwrap()
            .settings
            .migration
            .is_none()
    );
}

#[test]
fn an_account_already_moving_is_not_updated_again() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    let key = imap_account(&mut worker);
    let mut settings = worker.store.accounts.get(&key).unwrap().settings.clone();
    settings.migration = Some(Migration {
        step: 1,
        failures: 0,
    });
    worker.store.accounts.update(&key, settings).unwrap();
    update(&mut worker, &key);
    answer(
        &mut worker,
        &sink,
        super::provider_tests::tokens(ANNA, Some(CONSUMER_TENANT)),
    );
    // Already moving: this is a plain sign-in with the Graph scopes, not a
    // second move.
    assert_ne!(outcomes(&sink), vec![OAuthOutcome::Updated]);
}
