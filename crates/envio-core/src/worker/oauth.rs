//! Signing in with a provider in the browser (#219): one attempt at a time,
//! in memory only. The verifier, the code and the tokens stay in this
//! module's state and the setup thread's job; no event or log carries them.

use envio_net::oauth::{
    Identity, Pkce, Provider, ProviderKind, RedirectError, TokenError, Tokens, authorize_url,
    identity_matches, parse_redirect, provider_kind_for,
};
use envio_store::{AccountSettings, BackendKind};
use secrecy::SecretString;

use self::route::{Route, Seen, route};
use super::Worker;
use super::conn::Live;
use crate::api::event::{Event, OAuthFailure, OAuthOffer, OAuthOutcome};
use crate::clock::now_ms;
use crate::mask;
use crate::servers::{ServerAddr, store_server};
use crate::setup_thread::{ExchangeJob, ExchangeResult, SetupJob, next_ticket};
use crate::text;

mod route;

/// How long the user has to sign in (and to answer the identity question).
const TIMEOUT_MS: i64 = 5 * 60 * 1000;

enum Phase {
    /// The browser is open; the redirect has not come.
    Waiting,
    /// The code is with the setup thread.
    Exchanging,
    /// Signed in as someone else than the typed address: waits for the
    /// answer, holding the refresh token (memory only).
    Confirming(SecretString),
    /// The token is the other kind of account than the pick; waits for the
    /// answer to the one second sign-in. No token is kept.
    Mismatch,
}

pub(crate) struct Attempt {
    /// The typed address, or the account's own for "Sign in again".
    email: String,
    /// The account being signed in again.
    account: Option<String>,
    provider: Provider,
    pkce: Pkce,
    deadline_ms: i64,
    /// Told to the setup thread; only its answer is used.
    ticket: u64,
    phase: Phase,
    /// The tenant of the token decides Graph or IMAP (a new account).
    routed: bool,
    /// The wizard's pick chose the scopes; without one ("Other") a wrong
    /// guess is corrected with no question.
    picked: bool,
    /// Who the first token named, once the second sign-in was asked for.
    first: Option<Seen>,
}

/// What the id token says about the typed address (decision 9).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Match,
    /// Ask; the text names this address, if the token gave one.
    Ask(Option<String>),
}

pub(crate) fn judge(typed: &str, identity: Option<&Identity>) -> Verdict {
    match identity {
        Some(id) if identity_matches(typed, id) => Verdict::Match,
        Some(id) => Verdict::Ask(id.email.clone().or_else(|| id.preferred_username.clone())),
        None => Verdict::Ask(None),
    }
}

/// Why the token request failed. A refusal the provider will repeat (policy,
/// blocked client) is `Blocked`; a reused or expired code is `Other`.
pub(crate) fn failure_of(error: &TokenError) -> OAuthFailure {
    match error {
        TokenError::Temporary => OAuthFailure::Offline,
        TokenError::Refused(code) if is_admin_code(code) => OAuthFailure::Admin,
        TokenError::Refused(_) => OAuthFailure::Blocked,
        // The net layer reads a 400 `consent_required` as `Revoked` (right
        // for a refresh); at the code exchange it is the admin's block.
        TokenError::Revoked(code) if is_admin_code(code) => OAuthFailure::Admin,
        TokenError::Revoked(_) => OAuthFailure::Other,
    }
}

/// The provider's `error` code for a tenant whose admin has not allowed
/// the app: trying again will not help. `interaction_required` is not one
/// (it asks for a step such as MFA). `access_denied` stays a user cancel;
/// T20 must check it against a real admin block on a work tenant.
fn is_admin_code(code: &str) -> bool {
    code == "consent_required"
}

fn provider_name(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Microsoft | ProviderKind::MicrosoftGraph => "Microsoft",
    }
}

fn timeout_ms(_email: &str) -> i64 {
    #[cfg(feature = "test-hooks")]
    if let Some(ms) = crate::hooks::oauth_timeout(_email) {
        return ms;
    }
    TIMEOUT_MS
}

impl Worker<'_> {
    pub(crate) fn provider_of(&self, kind: ProviderKind) -> Option<Provider> {
        self.providers.iter().find(|p| p.kind == kind).cloned()
    }

    /// `Detect` for an address whose provider is set up: the servers are the
    /// provider's, nothing is looked up. `false`: no provider, so the caller
    /// does what it always did.
    pub(crate) fn detect_with_provider(&mut self, email: &str) -> bool {
        let Some(kind) = provider_kind_for(email) else {
            return false;
        };
        let Some(provider) = self.provider_of(kind) else {
            return false;
        };
        let email = email.trim().to_owned();
        let name = email.rsplit_once('@').map_or("", |(local, _)| local);
        let addr = |e: &envio_net::Endpoint| ServerAddr {
            host: e.host.clone(),
            port: e.port,
            security: e.security,
        };
        let (imap, smtp) = (addr(&provider.imap), addr(&provider.smtp));
        let name_hint = name.to_owned();
        let setup = crate::api::command::AccountSetup {
            name: name_hint.clone(),
            email: email.clone(),
            username: email.clone(),
            imap,
            smtp,
            source: Some(envio_net::autoconfig::Source::Provider),
            needs_confirm: false,
        };
        let provider_name = provider_name(kind);
        let servers = text::setup::servers(&setup.imap, &setup.smtp);
        self.emit(Event::Detected {
            email,
            setup,
            text: text::oauth::found(provider_name),
            servers,
            oauth: Some(OAuthOffer {
                provider: provider_name.to_owned(),
                name_hint,
            }),
        });
        true
    }

    /// A Microsoft address with no Microsoft provider set up (sign-in is off
    /// until #382, or the build has no client ID): refused before any
    /// lookup, since autoconfig would only lead to a password that Microsoft
    /// does not take. `false`: not a Microsoft address, or sign-in is on.
    pub(crate) fn refuse_microsoft(&mut self, email: &str) -> bool {
        let Some(kind) = provider_kind_for(email) else {
            return false;
        };
        if self.provider_of(kind).is_some() {
            return false;
        }
        let (title, text) = text::setup::microsoft_refused();
        self.emit(Event::SetupRefused {
            email: email.trim().to_owned(),
            title,
            text,
            manual_ok: false,
        });
        true
    }

    /// `kind`: the provider to sign in with. A new account from the wizard
    /// passes its pick (any address); `None` takes the address's own
    /// provider, or the account's own for "Sign in again".
    pub(crate) fn start_oauth(
        &mut self,
        email: &str,
        account: Option<String>,
        kind: Option<ProviderKind>,
    ) {
        // A new account is routed by its token's tenant, picked or not.
        let (routed, picked) = (account.is_none(), kind.is_some());
        let (email, kind) = match &account {
            Some(key) => match self.store.accounts.get(key) {
                Some(a) if a.settings.is_oauth() && a.broken.is_none() => (
                    a.settings.email.clone(),
                    kind.unwrap_or_else(|| super::token::kind_of(&a.settings)),
                ),
                _ => return self.refuse(email.to_owned(), OAuthFailure::NotOAuth),
            },
            None => {
                let email = email.trim().to_owned();
                let Some(kind) = kind.or_else(|| provider_kind_for(&email)) else {
                    return self.refuse(email, OAuthFailure::Unavailable);
                };
                if self.email_taken(&email) {
                    return self.refuse(email, OAuthFailure::Duplicate);
                }
                (email, kind)
            }
        };
        let Some(provider) = self.provider_of(kind) else {
            return self.refuse(email, OAuthFailure::Unavailable);
        };
        let pkce = Pkce::new();
        let url = authorize_url(&provider, &pkce, &email);
        log::info!("starting a sign-in for {}", mask::address(&email));
        let deadline_ms = now_ms().saturating_add(timeout_ms(&email));
        self.oauth = Some(Attempt {
            email: email.clone(),
            account,
            provider,
            pkce,
            deadline_ms,
            ticket: next_ticket(),
            phase: Phase::Waiting,
            routed,
            picked,
            first: None,
        });
        self.emit(Event::OAuthBrowser {
            email,
            url,
            user_code: None,
        });
    }

    /// A start that cannot begin is answered, so the screen does not wait.
    fn refuse(&mut self, email: String, why: OAuthFailure) {
        self.emit(Event::OAuthDone {
            email,
            outcome: OAuthOutcome::Failed(why),
        });
    }

    /// Ends the attempt, whatever its state, and says how.
    fn finish_oauth(&mut self, outcome: OAuthOutcome) {
        if let Some(attempt) = self.oauth.take() {
            self.emit(Event::OAuthDone {
                email: attempt.email,
                outcome,
            });
        }
    }

    pub(crate) fn oauth_redirect(&mut self, url: &str) {
        let Some(attempt) = self
            .oauth
            .as_mut()
            .filter(|a| matches!(a.phase, Phase::Waiting))
        else {
            log::info!("a sign-in link arrived with no sign-in waiting");
            return self.notice(text::oauth::stale_link());
        };
        match parse_redirect(url, &attempt.pkce.state, &attempt.provider.redirect_uri) {
            Ok(redirect) => {
                attempt.phase = Phase::Exchanging;
                let job = ExchangeJob {
                    email: attempt.email.clone(),
                    ticket: attempt.ticket,
                    provider: attempt.provider.clone(),
                    roots: self.roots.clone(),
                    code: redirect.code,
                    verifier: attempt.pkce.verifier.clone(),
                };
                self.setup.send(SetupJob::Exchange(Box::new(job)));
            }
            Err(RedirectError::Denied) => self.finish_oauth(OAuthOutcome::Cancelled),
            Err(RedirectError::Server(code)) => {
                let why = if is_admin_code(&code) {
                    OAuthFailure::Admin
                } else {
                    OAuthFailure::Other
                };
                self.finish_oauth(OAuthOutcome::Failed(why));
            }
            Err(RedirectError::Mismatch | RedirectError::Malformed) => {
                log::info!("ignored a sign-in link that does not belong to this sign-in");
                self.notice(text::oauth::stale_link());
            }
        }
    }

    /// The setup thread's answer to the code; one for a replaced or ended
    /// attempt is dropped.
    pub(super) fn oauth_exchanged(&mut self, done: ExchangeResult) {
        let current = self
            .oauth
            .as_ref()
            .is_some_and(|a| a.ticket == done.ticket && matches!(a.phase, Phase::Exchanging));
        if !current {
            log::info!("dropped a stale sign-in result");
            return;
        }
        let tokens = match done.result {
            Ok(tokens) => tokens,
            Err(error) => {
                log::warn!("the sign-in code was not accepted ({error:?})");
                return self.finish_oauth(OAuthOutcome::Failed(failure_of(&error)));
            }
        };
        self.judged(tokens);
    }

    fn judged(&mut self, tokens: Tokens) {
        let Some(attempt) = self.oauth.as_mut() else {
            return;
        };
        let Some(refresh) = tokens.refresh else {
            log::warn!("the sign-in gave no refresh token");
            return self.finish_oauth(OAuthOutcome::Failed(OAuthFailure::Other));
        };
        if attempt.routed {
            match route(
                attempt.provider.kind,
                attempt.first.as_ref(),
                tokens.identity.as_ref(),
            ) {
                Route::Fits => {}
                Route::Refuse => {
                    let why = if attempt.first.is_none() {
                        OAuthFailure::NoTenant
                    } else {
                        OAuthFailure::Mismatch
                    };
                    return self.finish_oauth(OAuthOutcome::Failed(why));
                }
                Route::Other { .. } if !attempt.picked => {
                    // No pick to be wrong about: the tenant decides, with
                    // the other scopes and no question.
                    attempt.first = Some(Seen::of(tokens.identity.as_ref()));
                    return self.second_sign_in();
                }
                Route::Other { personal } => {
                    let seen = Seen::of(tokens.identity.as_ref());
                    let who = seen.who().unwrap_or(&attempt.email).to_owned();
                    let (title, text) = text::oauth::mismatch(&who, personal);
                    attempt.first = Some(seen);
                    attempt.phase = Phase::Mismatch;
                    let email = attempt.email.clone();
                    return self.emit(Event::OAuthMismatch { email, title, text });
                }
            }
        }
        match judge(&attempt.email, tokens.identity.as_ref()) {
            Verdict::Match => self.go_on(refresh),
            Verdict::Ask(signed_in_as) => {
                let text = text::oauth::confirm(signed_in_as.as_deref(), &attempt.email);
                let email = attempt.email.clone();
                attempt.phase = Phase::Confirming(refresh);
                self.emit(Event::OAuthConfirm {
                    email,
                    signed_in_as,
                    text,
                });
            }
        }
    }

    pub(crate) fn confirm_oauth(&mut self, accept: bool) {
        if self
            .oauth
            .as_ref()
            .is_some_and(|a| matches!(a.phase, Phase::Mismatch))
        {
            return if accept {
                self.second_sign_in()
            } else {
                self.finish_oauth(OAuthOutcome::Cancelled)
            };
        }
        let confirming = self
            .oauth
            .as_ref()
            .is_some_and(|a| matches!(a.phase, Phase::Confirming(_)));
        if !confirming {
            return self.notice(text::oauth::nothing_to_confirm());
        }
        let Some(Phase::Confirming(refresh)) = self
            .oauth
            .as_mut()
            .map(|a| std::mem::replace(&mut a.phase, Phase::Waiting))
        else {
            return;
        };
        if accept {
            self.go_on(refresh);
        } else {
            self.finish_oauth(OAuthOutcome::Cancelled);
        }
    }

    /// The one second sign-in, with the other scope set. It keeps the
    /// attempt's address and what the first token named; the answer must
    /// match both (`route`).
    fn second_sign_in(&mut self) {
        let Some(attempt) = self.oauth.as_ref() else {
            return;
        };
        let other = match attempt.provider.kind {
            ProviderKind::Microsoft => ProviderKind::MicrosoftGraph,
            ProviderKind::MicrosoftGraph => ProviderKind::Microsoft,
        };
        let email = attempt.email.clone();
        let Some(provider) = self.provider_of(other) else {
            return self.finish_oauth(OAuthOutcome::Failed(OAuthFailure::Unavailable));
        };
        let pkce = Pkce::new();
        let url = authorize_url(&provider, &pkce, &email);
        log::info!("starting the second sign-in for {}", mask::address(&email));
        if let Some(attempt) = self.oauth.as_mut() {
            attempt.provider = provider;
            attempt.pkce = pkce;
            attempt.ticket = next_ticket();
            attempt.deadline_ms = now_ms().saturating_add(timeout_ms(&email));
            attempt.phase = Phase::Waiting;
        }
        self.emit(Event::OAuthBrowser {
            email,
            url,
            user_code: None,
        });
    }

    pub(crate) fn cancel_oauth(&mut self) {
        self.finish_oauth(OAuthOutcome::Cancelled);
    }

    /// The sign-in is accepted: keep the account (new, or signed in again).
    fn go_on(&mut self, refresh: SecretString) {
        let Some(attempt) = self.oauth.as_ref() else {
            return;
        };
        let (email, account) = (attempt.email.clone(), attempt.account.clone());
        let outcome = match account {
            None => self.add_oauth_account(&email, refresh),
            Some(key) => self.sign_in_again(&key, refresh),
        };
        self.finish_oauth(outcome);
    }

    /// A token account this session saved but could not finish: its token
    /// was not written and taking it back out failed too. It is not live, so
    /// it is not a real account yet; after a restart it shows as "Sign in
    /// again".
    pub(super) fn stranded(&self, account: &envio_store::Account) -> bool {
        account.settings.is_oauth()
            && !self.live.contains_key(&account.key)
            && self
                .store
                .tokens
                .borrow()
                .get(&account.key, super::token::kind_of(&account.settings))
                .is_none()
    }

    fn email_taken(&self, email: &str) -> bool {
        self.store.accounts.list().iter().any(|a| {
            a.broken.is_none() && a.settings.email.eq_ignore_ascii_case(email) && !self.stranded(a)
        })
    }

    /// Removes the stranded entries for `email`, so a retry does not leave
    /// two. `false` if one could not be removed.
    fn drop_stranded(&mut self, email: &str) -> bool {
        let keys: Vec<String> = self
            .store
            .accounts
            .list()
            .iter()
            .filter(|a| a.settings.email.eq_ignore_ascii_case(email) && self.stranded(a))
            .map(|a| a.key.clone())
            .collect();
        keys.iter().all(|key| {
            self.store
                .accounts
                .remove(key)
                .map_err(|error| log::error!("removing an unfinished account: {error}"))
                .is_ok()
        })
    }

    fn add_oauth_account(&mut self, email: &str, refresh: SecretString) -> OAuthOutcome {
        let Some(provider) = self.oauth.as_ref().map(|a| a.provider.clone()) else {
            return OAuthOutcome::Failed(OAuthFailure::Other);
        };
        if self.email_taken(email) {
            return OAuthOutcome::Failed(OAuthFailure::Duplicate);
        }
        if !self.drop_stranded(email) {
            return OAuthOutcome::Failed(OAuthFailure::NotSaved);
        }
        let addr = |e: &envio_net::Endpoint| {
            store_server(
                &ServerAddr {
                    host: e.host.clone(),
                    port: e.port,
                    security: e.security,
                },
                None,
            )
        };
        // A Graph account has no mail servers: Graph is its only endpoint.
        let graph = provider.kind == ProviderKind::MicrosoftGraph;
        let settings = AccountSettings {
            name: email.rsplit_once('@').map_or("", |(l, _)| l).to_owned(),
            email: email.to_owned(),
            username: email.to_owned(),
            backend: if graph {
                BackendKind::Graph
            } else {
                BackendKind::Imap
            },
            imap: (!graph).then(|| addr(&provider.imap)),
            smtp: (!graph).then(|| addr(&provider.smtp)),
            auth: Some("microsoft".to_owned()),
            migration: None,
        };
        match self.save_oauth_account(settings, refresh) {
            Ok(account) => {
                let mut live = Live::new();
                live.wants.full = true;
                self.live.insert(account.key.clone(), live);
                self.emit(Event::AccountAdded { account });
                OAuthOutcome::Added
            }
            Err(_) => OAuthOutcome::Failed(OAuthFailure::NotSaved),
        }
    }

    pub(super) fn sign_in_again(&mut self, key: &str, refresh: SecretString) -> OAuthOutcome {
        let Some(kind) = self
            .store
            .accounts
            .get(key)
            .map(|a| super::token::kind_of(&a.settings))
        else {
            return OAuthOutcome::Failed(OAuthFailure::Other);
        };
        if !self.live.contains_key(key) {
            return OAuthOutcome::Failed(OAuthFailure::Other);
        }
        if let Err(error) = self.store.tokens.borrow_mut().set(key, kind, refresh) {
            log::error!("saving the new sign-in: {error}");
            return OAuthOutcome::Failed(OAuthFailure::NotSaved);
        }
        // The old access token and its worked-once flag belong to the old sign-in.
        self.store.token_cells.forget(key);
        self.resume(key);
        OAuthOutcome::Resumed
    }

    /// When the attempt, if any, must end.
    pub(super) fn oauth_deadline(&self) -> Option<i64> {
        self.oauth.as_ref().map(|a| a.deadline_ms)
    }

    pub(super) fn oauth_timer(&mut self, now: i64) {
        if self.oauth_deadline().is_some_and(|at| at <= now) {
            log::info!("the sign-in timed out");
            self.finish_oauth(OAuthOutcome::Failed(OAuthFailure::Timeout));
        }
    }
}

#[cfg(test)]
mod provider_tests;
#[cfg(test)]
mod tests;
