//! The wizard's provider pick (#382 T17): the tenant decides Graph or IMAP,
//! one second sign-in on a wrong guess, and nothing is saved on a refusal.
//! No network: the token answers are handed to the worker directly.

use envio_net::oauth::{CONSUMER_TENANT, Identity, Provider, ProviderKind, Tokens};
use envio_store::BackendKind;
use secrecy::{ExposeSecret, SecretString};

use super::*;
use crate::worker::Worker;
use crate::worker::testing::{Sink, open, seed};

const WORK: &str = "11111111-2222-3333-4444-555555555555";
const ANNA: &str = "anna@example.org";

fn config(dir: &std::path::Path) -> crate::core::Config {
    let (mut config, _) = seed(dir);
    config.oauth = vec![
        Provider::microsoft_template().with_client_id("test-client-id"),
        Provider::microsoft_graph_template().with_client_id("test-client-id"),
    ];
    config
}

fn tokens(who: &str, tid: Option<&str>) -> Tokens {
    Tokens {
        access: SecretString::from("at-1"),
        refresh: Some(SecretString::from("rt-new")),
        expires_in: 3600,
        identity: Some(Identity {
            email: Some(who.into()),
            preferred_username: None,
            tid: tid.map(Into::into),
        }),
    }
}

/// The token endpoint's answer to the code of the attempt in flight.
fn answer(worker: &mut Worker<'_>, tokens: Tokens) {
    let attempt = worker.oauth.as_mut().expect("an attempt");
    attempt.phase = Phase::Exchanging;
    let ticket = attempt.ticket;
    worker.oauth_exchanged(ExchangeResult {
        ticket,
        result: Ok(tokens),
    });
}

fn urls(sink: &Sink) -> Vec<String> {
    sink.events()
        .into_iter()
        .filter_map(|e| match e {
            Event::OAuthBrowser { url, .. } => Some(url),
            _ => None,
        })
        .collect()
}

fn mismatches(sink: &Sink) -> Vec<(String, String)> {
    sink.events()
        .into_iter()
        .filter_map(|e| match e {
            Event::OAuthMismatch { title, text, .. } => Some((title, text)),
            _ => None,
        })
        .collect()
}

fn outcomes(sink: &Sink) -> Vec<OAuthOutcome> {
    sink.events()
        .into_iter()
        .filter_map(|e| match e {
            Event::OAuthDone { outcome, .. } => Some(outcome),
            _ => None,
        })
        .collect()
}

/// The accounts beside the seeded one.
fn added(worker: &Worker<'_>) -> Vec<envio_store::Account> {
    worker
        .store
        .accounts
        .list()
        .iter()
        .filter(|a| a.settings.email != "me@example.org")
        .cloned()
        .collect()
}

fn token_of(worker: &Worker<'_>, key: &str, kind: ProviderKind) -> Option<String> {
    let tokens = worker.store.tokens.borrow();
    tokens.get(key, kind).map(|t| t.expose_secret().to_owned())
}

fn start(worker: &mut Worker<'_>, kind: ProviderKind) {
    worker.start_oauth(ANNA, None, Some(kind));
}

#[test]
fn a_consumer_tenant_after_the_outlook_pick_saves_a_graph_account() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    start(&mut worker, ProviderKind::MicrosoftGraph);
    assert!(urls(&sink)[0].contains("Mail.ReadWrite"));
    answer(&mut worker, tokens(ANNA, Some(CONSUMER_TENANT)));

    let saved = added(&worker);
    assert_eq!(saved.len(), 1);
    let settings = &saved[0].settings;
    assert_eq!(settings.backend, BackendKind::Graph);
    assert!(settings.imap.is_none() && settings.smtp.is_none());
    assert_eq!(settings.auth.as_deref(), Some("microsoft"));
    let key = &saved[0].key;
    assert_eq!(
        token_of(&worker, key, ProviderKind::MicrosoftGraph).as_deref(),
        Some("rt-new")
    );
    assert_eq!(token_of(&worker, key, ProviderKind::Microsoft), None);
    assert_eq!(outcomes(&sink), vec![OAuthOutcome::Added]);
    assert!(mismatches(&sink).is_empty());
}

#[test]
fn a_work_tenant_after_the_work_pick_saves_an_imap_account_for_any_domain() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    start(&mut worker, ProviderKind::Microsoft);
    assert!(urls(&sink)[0].contains("IMAP.AccessAsUser"));
    answer(&mut worker, tokens(ANNA, Some(WORK)));

    let saved = added(&worker);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].settings.backend, BackendKind::Imap);
    assert!(saved[0].settings.imap.is_some());
    assert_eq!(
        token_of(&worker, &saved[0].key, ProviderKind::Microsoft).as_deref(),
        Some("rt-new")
    );
    assert_eq!(outcomes(&sink), vec![OAuthOutcome::Added]);
}

#[test]
fn a_work_tenant_after_the_outlook_pick_asks_then_signs_in_once_more_with_imap_scopes() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    start(&mut worker, ProviderKind::MicrosoftGraph);
    answer(&mut worker, tokens(ANNA, Some(WORK)));

    // Nothing is saved while the question is open, and the token is not kept.
    assert!(added(&worker).is_empty());
    assert!(outcomes(&sink).is_empty());
    let asked = mismatches(&sink);
    assert_eq!(asked.len(), 1);
    assert!(asked[0].0.contains("work or school"), "{asked:?}");
    assert!(asked[0].1.contains(ANNA), "{asked:?}");
    assert_eq!(urls(&sink).len(), 1, "no second browser before the answer");

    worker.confirm_oauth(true);
    let urls = urls(&sink);
    assert_eq!(urls.len(), 2);
    assert!(urls[1].contains("IMAP.AccessAsUser"), "{}", urls[1]);
    assert_ne!(
        urls[0].split("state=").nth(1),
        urls[1].split("state=").nth(1),
        "a new attempt has its own state"
    );

    answer(&mut worker, tokens(ANNA, Some(WORK)));
    let saved = added(&worker);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].settings.backend, BackendKind::Imap);
    assert_eq!(
        token_of(&worker, &saved[0].key, ProviderKind::Microsoft).as_deref(),
        Some("rt-new")
    );
    assert_eq!(
        token_of(&worker, &saved[0].key, ProviderKind::MicrosoftGraph),
        None
    );
    assert_eq!(outcomes(&sink), vec![OAuthOutcome::Added]);
}

#[test]
fn a_personal_tenant_after_the_work_pick_says_so_and_ends_on_graph() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    start(&mut worker, ProviderKind::Microsoft);
    answer(&mut worker, tokens(ANNA, Some(CONSUMER_TENANT)));
    assert!(mismatches(&sink)[0].0.contains("personal"));
    worker.confirm_oauth(true);
    assert!(urls(&sink)[1].contains("Mail.ReadWrite"));
    answer(&mut worker, tokens(ANNA, Some(CONSUMER_TENANT)));
    assert_eq!(added(&worker)[0].settings.backend, BackendKind::Graph);
}

#[test]
fn declining_the_second_sign_in_saves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    start(&mut worker, ProviderKind::MicrosoftGraph);
    answer(&mut worker, tokens(ANNA, Some(WORK)));
    worker.confirm_oauth(false);
    assert!(added(&worker).is_empty());
    assert_eq!(outcomes(&sink), vec![OAuthOutcome::Cancelled]);
    assert!(worker.oauth.is_none());
}

#[test]
fn a_second_token_whose_tenant_is_wrong_again_is_refused_without_a_third_sign_in() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    start(&mut worker, ProviderKind::MicrosoftGraph);
    answer(&mut worker, tokens(ANNA, Some(WORK)));
    worker.confirm_oauth(true);
    // The IMAP sign-in came back personal: the wrong kind once more.
    answer(&mut worker, tokens(ANNA, Some(CONSUMER_TENANT)));

    assert!(added(&worker).is_empty());
    assert_eq!(mismatches(&sink).len(), 1, "no second question");
    assert_eq!(urls(&sink).len(), 2, "no third sign-in");
    assert_eq!(
        outcomes(&sink),
        vec![OAuthOutcome::Failed(OAuthFailure::Mismatch)]
    );
    assert!(worker.oauth.is_none());
}

#[test]
fn a_second_token_for_another_address_or_tenant_is_refused() {
    for (who, tid) in [
        ("bob@example.org", WORK),
        (ANNA, "99999999-2222-3333-4444-555555555555"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (config, sink) = (config(dir.path()), Sink::default());
        let mut worker = open(&config, &sink);
        start(&mut worker, ProviderKind::MicrosoftGraph);
        answer(&mut worker, tokens(ANNA, Some(WORK)));
        worker.confirm_oauth(true);
        answer(&mut worker, tokens(who, Some(tid)));
        assert!(added(&worker).is_empty(), "{who} {tid}");
        assert_eq!(
            outcomes(&sink),
            vec![OAuthOutcome::Failed(OAuthFailure::Mismatch)]
        );
    }
}

#[test]
fn a_graph_pick_with_no_tenant_in_the_token_saves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    start(&mut worker, ProviderKind::MicrosoftGraph);
    answer(&mut worker, tokens(ANNA, None));
    assert!(added(&worker).is_empty());
    assert_eq!(
        outcomes(&sink),
        vec![OAuthOutcome::Failed(OAuthFailure::Mismatch)]
    );
}

fn redirect_with(worker: &mut Worker<'_>, sink: &Sink, query: &str) {
    let url = urls(sink).pop().unwrap();
    let state = url
        .split(['?', '&'])
        .find_map(|p| p.strip_prefix("state="))
        .unwrap()
        .to_owned();
    worker.oauth_redirect(&format!("enviooauth://callback?{query}&state={state}"));
}

#[test]
fn consent_refused_by_the_user_saves_nothing_and_is_a_cancel() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    start(&mut worker, ProviderKind::MicrosoftGraph);
    redirect_with(&mut worker, &sink, "error=access_denied");
    assert!(added(&worker).is_empty());
    assert_eq!(outcomes(&sink), vec![OAuthOutcome::Cancelled]);
}

#[test]
fn consent_refused_by_an_admin_saves_nothing_and_says_admin() {
    for code in ["consent_required", "interaction_required"] {
        let dir = tempfile::tempdir().unwrap();
        let (config, sink) = (config(dir.path()), Sink::default());
        let mut worker = open(&config, &sink);
        start(&mut worker, ProviderKind::Microsoft);
        redirect_with(&mut worker, &sink, &format!("error={code}"));
        assert!(added(&worker).is_empty());
        assert_eq!(
            outcomes(&sink),
            vec![OAuthOutcome::Failed(OAuthFailure::Admin)],
            "{code}"
        );
    }
}

#[test]
fn another_error_code_is_not_an_admin_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    start(&mut worker, ProviderKind::Microsoft);
    redirect_with(&mut worker, &sink, "error=server_error");
    assert_eq!(
        outcomes(&sink),
        vec![OAuthOutcome::Failed(OAuthFailure::Other)]
    );
}

#[test]
fn the_pick_needs_its_provider_and_an_address_that_is_not_taken() {
    let dir = tempfile::tempdir().unwrap();
    let (mut config, sink) = (config(dir.path()), Sink::default());
    config.oauth.pop();
    let mut worker = open(&config, &sink);
    start(&mut worker, ProviderKind::MicrosoftGraph);
    worker.start_oauth("me@example.org", None, Some(ProviderKind::Microsoft));
    assert_eq!(
        outcomes(&sink),
        vec![
            OAuthOutcome::Failed(OAuthFailure::Unavailable),
            OAuthOutcome::Failed(OAuthFailure::Duplicate)
        ]
    );
    assert!(urls(&sink).is_empty());
}

#[test]
fn a_matching_tenant_still_asks_when_the_address_differs() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    start(&mut worker, ProviderKind::MicrosoftGraph);
    answer(
        &mut worker,
        tokens("other@example.org", Some(CONSUMER_TENANT)),
    );
    assert!(added(&worker).is_empty());
    assert!(
        sink.events()
            .iter()
            .any(|e| matches!(e, Event::OAuthConfirm { .. }))
    );
    worker.confirm_oauth(true);
    assert_eq!(added(&worker)[0].settings.backend, BackendKind::Graph);
}

#[test]
fn an_explicit_kind_on_sign_in_again_picks_that_providers_scopes() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    let key = worker.store.accounts.list()[0].key.clone();
    let mut settings = worker.store.accounts.get(&key).unwrap().settings.clone();
    settings.auth = Some("microsoft".into());
    worker.store.accounts.update(&key, settings).unwrap();
    worker
        .store
        .tokens
        .borrow_mut()
        .set(&key, ProviderKind::Microsoft, SecretString::from("rt"))
        .unwrap();
    worker.start_oauth("", Some(key), Some(ProviderKind::MicrosoftGraph));
    let url = urls(&sink).pop().expect("a browser link");
    assert!(url.contains("Mail.ReadWrite"), "{url}");
    // Not the wizard's routing: that is for a new account.
    assert!(!worker.oauth.as_ref().unwrap().routed);
}
