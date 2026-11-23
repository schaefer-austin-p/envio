//! The wizard's provider pick (#382 T17): the tenant decides Graph or IMAP,
//! one second sign-in on a wrong guess, and nothing is saved on a refusal.
//! No network: the token answers are handed to the worker directly.

use envio_net::oauth::{CONSUMER_TENANT, Identity, Provider, ProviderKind, Tokens};
use envio_store::BackendKind;
use secrecy::{ExposeSecret, SecretString};

use super::*;
use crate::worker::Worker;
use crate::worker::testing::{Sink, open, seed};

pub(super) const WORK: &str = "11111111-2222-3333-4444-555555555555";
pub(super) const ANNA: &str = "anna@example.org";

pub(super) fn config(dir: &std::path::Path) -> crate::core::Config {
    let (mut config, _) = seed(dir);
    config.oauth = [
        Provider::microsoft_template(),
        Provider::microsoft_graph_template(),
    ]
    .map(|mut p| {
        p.token_url = "http://127.0.0.1:1/token".into();
        p.with_client_id("test-client-id")
    })
    .to_vec();
    config
}

pub(super) fn tokens(who: &str, tid: Option<&str>) -> Tokens {
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

/// The token endpoint's answer to the code of the attempt in flight: the
/// real redirect carries the code in (the exchange job goes to a token URL
/// nobody listens on), then the answer is handed back with the ticket.
pub(super) fn answer(worker: &mut Worker<'_>, sink: &Sink, tokens: Tokens) {
    answer_with(worker, sink, Ok(tokens));
}

fn answer_with(worker: &mut Worker<'_>, sink: &Sink, result: Result<Tokens, TokenError>) {
    redirect_with(worker, sink, "code=c1");
    let attempt = worker.oauth.as_ref().expect("an attempt");
    assert!(matches!(attempt.phase, Phase::Exchanging));
    let ticket = attempt.ticket;
    worker.oauth_exchanged(ExchangeResult { ticket, result });
}

pub(super) fn urls(sink: &Sink) -> Vec<String> {
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

pub(super) fn outcomes(sink: &Sink) -> Vec<OAuthOutcome> {
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

pub(super) fn token_of(worker: &Worker<'_>, key: &str, kind: ProviderKind) -> Option<String> {
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
    answer(&mut worker, &sink, tokens(ANNA, Some(CONSUMER_TENANT)));

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
    answer(&mut worker, &sink, tokens(ANNA, Some(WORK)));

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
    answer(&mut worker, &sink, tokens(ANNA, Some(WORK)));

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

    answer(&mut worker, &sink, tokens(ANNA, Some(WORK)));
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
    answer(&mut worker, &sink, tokens(ANNA, Some(CONSUMER_TENANT)));
    assert!(mismatches(&sink)[0].0.contains("personal"));
    worker.confirm_oauth(true);
    assert!(urls(&sink)[1].contains("Mail.ReadWrite"));
    answer(&mut worker, &sink, tokens(ANNA, Some(CONSUMER_TENANT)));
    assert_eq!(added(&worker)[0].settings.backend, BackendKind::Graph);
}

#[test]
fn declining_the_second_sign_in_saves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    start(&mut worker, ProviderKind::MicrosoftGraph);
    answer(&mut worker, &sink, tokens(ANNA, Some(WORK)));
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
    answer(&mut worker, &sink, tokens(ANNA, Some(WORK)));
    worker.confirm_oauth(true);
    // The IMAP sign-in came back personal: the wrong kind once more.
    answer(&mut worker, &sink, tokens(ANNA, Some(CONSUMER_TENANT)));

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
        answer(&mut worker, &sink, tokens(ANNA, Some(WORK)));
        worker.confirm_oauth(true);
        answer(&mut worker, &sink, tokens(who, Some(tid)));
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
    answer(&mut worker, &sink, tokens(ANNA, None));
    assert!(added(&worker).is_empty());
    assert_eq!(
        outcomes(&sink),
        vec![OAuthOutcome::Failed(OAuthFailure::NoTenant)]
    );
    // Not "the second sign-in": there was none.
    assert!(!OAuthFailure::NoTenant.text().contains("second"));
    assert!(worker.oauth.is_none());
}

pub(super) fn redirect_with(worker: &mut Worker<'_>, sink: &Sink, query: &str) {
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
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    start(&mut worker, ProviderKind::Microsoft);
    redirect_with(&mut worker, &sink, "error=consent_required");
    assert!(added(&worker).is_empty());
    assert_eq!(
        outcomes(&sink),
        vec![OAuthOutcome::Failed(OAuthFailure::Admin)]
    );
}

#[test]
fn another_error_code_is_not_an_admin_refusal() {
    // `interaction_required` is a sign-in step (MFA), not an admin block.
    for code in ["server_error", "interaction_required"] {
        let dir = tempfile::tempdir().unwrap();
        let (config, sink) = (config(dir.path()), Sink::default());
        let mut worker = open(&config, &sink);
        start(&mut worker, ProviderKind::Microsoft);
        redirect_with(&mut worker, &sink, &format!("error={code}"));
        assert_eq!(
            outcomes(&sink),
            vec![OAuthOutcome::Failed(OAuthFailure::Other)],
            "{code}"
        );
    }
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
        &sink,
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

/// What the token endpoint says to a code it refuses, as the net layer reads it.
fn refusal(status: u16, error: &str) -> TokenError {
    let body = format!(r#"{{"error":"{error}"}}"#);
    envio_net::oauth::parse_token_reply(status, body.as_bytes()).unwrap_err()
}

#[test]
fn an_admin_block_at_the_code_exchange_says_admin() {
    // A work tenant that needs its admin answers 400 consent_required.
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    start(&mut worker, ProviderKind::Microsoft);
    answer_with(&mut worker, &sink, Err(refusal(400, "consent_required")));
    assert!(added(&worker).is_empty());
    assert_eq!(
        outcomes(&sink),
        vec![OAuthOutcome::Failed(OAuthFailure::Admin)]
    );
}

#[test]
fn other_refusals_at_the_code_exchange_are_not_admin() {
    for code in ["invalid_grant", "interaction_required"] {
        let dir = tempfile::tempdir().unwrap();
        let (config, sink) = (config(dir.path()), Sink::default());
        let mut worker = open(&config, &sink);
        start(&mut worker, ProviderKind::Microsoft);
        answer_with(&mut worker, &sink, Err(refusal(400, code)));
        assert_eq!(
            outcomes(&sink),
            vec![OAuthOutcome::Failed(OAuthFailure::Other)],
            "{code}"
        );
    }
}

/// A first sign-in that asks the question.
fn asked(worker: &mut Worker<'_>, sink: &Sink) {
    start(worker, ProviderKind::MicrosoftGraph);
    answer(worker, sink, tokens(ANNA, Some(WORK)));
    assert!(matches!(
        worker.oauth.as_ref().unwrap().phase,
        Phase::Mismatch
    ));
}

#[test]
fn a_cancel_while_the_question_is_open_ends_it_and_saves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    asked(&mut worker, &sink);
    worker.cancel_oauth();
    assert!(worker.oauth.is_none());
    assert!(added(&worker).is_empty());
    assert_eq!(outcomes(&sink), vec![OAuthOutcome::Cancelled]);
}

#[test]
fn a_timeout_while_the_question_is_open_ends_it_and_saves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    asked(&mut worker, &sink);
    let due = worker.oauth_deadline().unwrap();
    worker.oauth_timer(due);
    assert!(worker.oauth.is_none());
    assert!(added(&worker).is_empty());
    assert_eq!(
        outcomes(&sink),
        vec![OAuthOutcome::Failed(OAuthFailure::Timeout)]
    );
}

#[test]
fn the_first_attempts_redirect_after_the_second_sign_in_started_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    asked(&mut worker, &sink);
    let first = urls(&sink).pop().unwrap();
    worker.confirm_oauth(true);
    let state = first
        .split(['?', '&'])
        .find_map(|p| p.strip_prefix("state="))
        .unwrap();
    worker.oauth_redirect(&format!("enviooauth://callback?code=old&state={state}"));
    assert!(matches!(
        worker.oauth.as_ref().unwrap().phase,
        Phase::Waiting
    ));
    assert!(added(&worker).is_empty());
    assert!(outcomes(&sink).is_empty());
}

#[test]
fn a_second_token_with_no_identity_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    asked(&mut worker, &sink);
    worker.confirm_oauth(true);
    let mut bare = tokens(ANNA, Some(WORK));
    bare.identity = None;
    answer(&mut worker, &sink, bare);
    assert!(added(&worker).is_empty());
    assert_eq!(
        outcomes(&sink),
        vec![OAuthOutcome::Failed(OAuthFailure::Mismatch)]
    );
}

#[test]
fn a_failed_exchange_in_the_second_sign_in_saves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    asked(&mut worker, &sink);
    worker.confirm_oauth(true);
    answer_with(&mut worker, &sink, Err(refusal(400, "invalid_grant")));
    assert!(added(&worker).is_empty());
    assert!(worker.oauth.is_none());
    assert_eq!(
        outcomes(&sink),
        vec![OAuthOutcome::Failed(OAuthFailure::Other)]
    );
}

#[test]
fn a_denied_redirect_in_the_second_sign_in_saves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    asked(&mut worker, &sink);
    worker.confirm_oauth(true);
    redirect_with(&mut worker, &sink, "error=access_denied");
    assert!(added(&worker).is_empty());
    assert!(worker.oauth.is_none());
    assert_eq!(outcomes(&sink), vec![OAuthOutcome::Cancelled]);
}

/// The "Other" path with an address whose default is still IMAP (a known
/// consumer domain goes to Graph first, #384): no pick, so the first sign-in
/// uses the address's own provider (IMAP scopes) and the tenant routes it.
const WORK_DEFAULT_IMAP: &str = "anna@office365.com";

#[test]
fn on_the_other_path_a_work_tenant_saves_an_imap_account_without_a_question() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    worker.start_oauth(WORK_DEFAULT_IMAP, None, None);
    answer(&mut worker, &sink, tokens(WORK_DEFAULT_IMAP, Some(WORK)));
    let saved = added(&worker);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].settings.backend, BackendKind::Imap);
    assert_eq!(
        token_of(&worker, &saved[0].key, ProviderKind::Microsoft).as_deref(),
        Some("rt-new")
    );
    assert!(mismatches(&sink).is_empty());
    assert_eq!(urls(&sink).len(), 1);
    assert_eq!(outcomes(&sink), vec![OAuthOutcome::Added]);
}

#[test]
fn on_the_other_path_a_consumer_tenant_signs_in_with_graph_scopes_and_saves_graph() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    worker.start_oauth(WORK_DEFAULT_IMAP, None, None);
    assert!(!urls(&sink)[0].contains("Mail.ReadWrite"));
    answer(
        &mut worker,
        &sink,
        tokens(WORK_DEFAULT_IMAP, Some(CONSUMER_TENANT)),
    );
    // The IMAP-scoped token cannot serve Graph: one more sign-in, no question.
    assert!(mismatches(&sink).is_empty());
    assert!(added(&worker).is_empty());
    assert!(urls(&sink)[1].contains("Mail.ReadWrite"));
    answer(
        &mut worker,
        &sink,
        tokens(WORK_DEFAULT_IMAP, Some(CONSUMER_TENANT)),
    );
    let saved = added(&worker);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].settings.backend, BackendKind::Graph);
    assert_eq!(
        token_of(&worker, &saved[0].key, ProviderKind::MicrosoftGraph).as_deref(),
        Some("rt-new")
    );
    assert!(mismatches(&sink).is_empty());
    assert_eq!(urls(&sink).len(), 2);
    assert_eq!(outcomes(&sink), vec![OAuthOutcome::Added]);
}

const OUTLOOK: &str = "anna@outlook.com";

#[test]
fn a_consumer_address_with_no_pick_signs_in_once_and_saves_graph() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    worker.start_oauth(OUTLOOK, None, None);
    assert!(urls(&sink)[0].contains("Mail.ReadWrite"));
    answer(&mut worker, &sink, tokens(OUTLOOK, Some(CONSUMER_TENANT)));
    let saved = added(&worker);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].settings.backend, BackendKind::Graph);
    assert_eq!(
        token_of(&worker, &saved[0].key, ProviderKind::MicrosoftGraph).as_deref(),
        Some("rt-new")
    );
    assert_eq!(urls(&sink).len(), 1, "one consent");
    assert!(mismatches(&sink).is_empty());
    assert_eq!(outcomes(&sink), vec![OAuthOutcome::Added]);
}

#[test]
fn a_consumer_address_with_no_pick_and_a_work_token_signs_in_again_with_imap() {
    let dir = tempfile::tempdir().unwrap();
    let (config, sink) = (config(dir.path()), Sink::default());
    let mut worker = open(&config, &sink);
    worker.start_oauth(OUTLOOK, None, None);
    assert!(urls(&sink)[0].contains("Mail.ReadWrite"));
    answer(&mut worker, &sink, tokens(OUTLOOK, Some(WORK)));
    // The Graph token cannot serve a work tenant: one more sign-in, no question.
    assert!(mismatches(&sink).is_empty());
    assert!(added(&worker).is_empty());
    assert!(urls(&sink)[1].contains("IMAP.AccessAsUser"));
    answer(&mut worker, &sink, tokens(OUTLOOK, Some(WORK)));
    let saved = added(&worker);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].settings.backend, BackendKind::Imap);
    assert_eq!(
        token_of(&worker, &saved[0].key, ProviderKind::Microsoft).as_deref(),
        Some("rt-new")
    );
    assert_eq!(outcomes(&sink), vec![OAuthOutcome::Added]);
}
