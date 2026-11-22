//! The OAuth access token of an account: cached, refreshed at most once at
//! a time per account, never written to disk. The refresh token is read
//! from `OAuthTokens` only, and a new one is stored there before the access
//! token is handed out.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use envio_net::NetError;
use envio_net::Roots;
use envio_net::oauth::{Provider, ProviderKind, TokenError, Tokens};
use futures::FutureExt;
use futures::future::{LocalBoxFuture, Shared};
use secrecy::SecretString;

use crate::oauth_tokens::OAuthTokens;
use crate::problem::Problem;

/// A cached token with less than this left is refreshed first.
pub(crate) const REFRESH_MARGIN_MS: i64 = 5 * 60 * 1000;

/// A token younger than this was good a moment ago: a sign-in refusal that
/// arrives with it is not an expired token.
pub(crate) const FRESH_MS: i64 = 60_000;

/// An access token and when it was obtained: a refusal is judged by the age
/// of the token the attempt used, not of whatever is cached by then.
#[derive(Clone, Debug)]
pub(crate) struct Granted {
    pub(crate) token: SecretString,
    pub(crate) at_ms: i64,
}

type Reply = Result<Granted, TokenError>;
type Flight = Shared<LocalBoxFuture<'static, Reply>>;
/// Trades a refresh token for new tokens (the network call, replaceable in tests).
pub(crate) type Refresher =
    Rc<dyn Fn(&Provider, &SecretString) -> LocalBoxFuture<'static, Result<Tokens, TokenError>>>;
pub(crate) type Clock = Rc<dyn Fn() -> i64>;

struct Access {
    token: SecretString,
    at_ms: i64,
    expires_ms: i64,
}

#[derive(Default)]
struct State {
    access: Option<Access>,
    flight: Option<(u64, Flight)>,
    next_flight: u64,
    imap_ok: bool,
    /// Why the account is paused: no refresh is made for it.
    paused: Option<Problem>,
}

struct Cell {
    key: String,
    kind: ProviderKind,
    provider: Provider,
    tokens: Rc<RefCell<OAuthTokens>>,
    refresh: Refresher,
    clock: Clock,
    state: RefCell<State>,
}

/// A handle on one account's token: clone-cheap, owns no `&Worker` borrow.
#[derive(Clone)]
pub(crate) struct TokenHandle(Rc<Cell>);

impl TokenHandle {
    /// A usable access token: the cached one if it has more than
    /// `REFRESH_MARGIN_MS` left (and `force` is not set), else one refresh.
    /// A second caller during a refresh awaits the first one's result.
    pub(crate) async fn access(&self, force: bool) -> Result<Granted, Problem> {
        if !force && let Some(granted) = self.0.cached() {
            return Ok(granted);
        }
        // A paused account makes no request, whoever asks (an attempt that
        // began before the pause included).
        if let Some(problem) = self.0.state.borrow().paused.clone() {
            return Err(problem);
        }
        let (id, flight) = self.0.join_or_start();
        let reply = flight.await;
        self.0.finish(id);
        reply.map_err(|e| token_problem(&e))
    }

    /// A token after `used` was refused: the cached one if it is newer than
    /// `used` (another connection already refreshed), else a forced refresh.
    pub(crate) async fn renew(&self, used: &Granted) -> Result<Granted, Problem> {
        if let Some(granted) = self.0.cached()
            && granted.at_ms > used.at_ms
        {
            return Ok(granted);
        }
        self.access(true).await
    }

    /// Milliseconds since a token obtained at `at_ms`.
    pub(crate) fn age_of(&self, at_ms: i64) -> i64 {
        (self.0.clock)().saturating_sub(at_ms)
    }

    /// Milliseconds since the cached access token was obtained.
    #[cfg(test)]
    pub(crate) fn age_ms(&self) -> Option<i64> {
        let at = self.0.state.borrow().access.as_ref()?.at_ms;
        Some(self.age_of(at))
    }

    /// A bearer IMAP login succeeded since the last sign-in.
    pub(crate) fn imap_ok(&self) -> bool {
        self.0.state.borrow().imap_ok
    }

    pub(crate) fn set_imap_ok(&self) {
        self.0.state.borrow_mut().imap_ok = true;
    }
}

impl Cell {
    fn cached(&self) -> Option<Granted> {
        let state = self.state.borrow();
        let access = state.access.as_ref()?;
        let left = access.expires_ms.saturating_sub((self.clock)());
        (left > REFRESH_MARGIN_MS).then(|| Granted {
            token: access.token.clone(),
            at_ms: access.at_ms,
        })
    }

    fn join_or_start(self: &Rc<Self>) -> (u64, Flight) {
        let mut state = self.state.borrow_mut();
        if let Some((id, flight)) = &state.flight {
            return (*id, flight.clone());
        }
        let id = state.next_flight;
        state.next_flight += 1;
        // Weak: the flight lives in the cell, so it must not own it.
        let weak = Rc::downgrade(self);
        let flight = async move {
            match weak.upgrade() {
                Some(cell) => cell.run().await,
                None => Err(TokenError::Temporary),
            }
        }
        .boxed_local()
        .shared();
        state.flight = Some((id, flight.clone()));
        (id, flight)
    }

    /// Whoever finishes first clears the flight; a newer one stays.
    fn finish(&self, id: u64) {
        let mut state = self.state.borrow_mut();
        if state.flight.as_ref().is_some_and(|(held, _)| *held == id) {
            state.flight = None;
        }
    }

    /// One refresh; its failure is logged here, once per flight, however
    /// many callers share it.
    async fn run(&self) -> Reply {
        let reply = self.refresh_once().await;
        if let Err(error) = &reply {
            crate::cause::log_token(error);
        }
        reply
    }

    async fn refresh_once(&self) -> Reply {
        let Some(stored) = self.tokens.borrow().get(&self.key, self.kind).cloned() else {
            return Err(TokenError::Revoked(
                crate::cause::NO_STORED_TOKEN.to_owned(),
            ));
        };
        let at = (self.clock)();
        let result = (self.refresh)(&self.provider, &stored).await;
        let tokens = match result {
            Ok(tokens) => tokens,
            Err(error) => {
                if matches!(error, TokenError::Revoked(_)) {
                    let mut state = self.state.borrow_mut();
                    state.access = None;
                    state.imap_ok = false;
                }
                return Err(error);
            }
        };
        if let Some(next) = tokens.refresh.clone() {
            // A failed write keeps the new access token in memory; the next
            // start refreshes from the stored token, which stays valid for a
            // while.
            if let Err(error) = self.tokens.borrow_mut().set(&self.key, self.kind, next) {
                log::error!("could not store the new OAuth refresh token: {error}");
            }
        }
        let ttl = i64::try_from(tokens.expires_in)
            .unwrap_or(i64::MAX)
            .saturating_mul(1000);
        self.state.borrow_mut().access = Some(Access {
            token: tokens.access.clone(),
            at_ms: at,
            expires_ms: at.saturating_add(ttl),
        });
        Ok(Granted {
            token: tokens.access,
            at_ms: at,
        })
    }
}

/// The handles of all token accounts, one per account key.
pub(crate) struct TokenCells {
    providers: Vec<Provider>,
    refresh: Refresher,
    clock: Clock,
    tokens: Rc<RefCell<OAuthTokens>>,
    cells: RefCell<BTreeMap<(String, ProviderKind), Rc<Cell>>>,
}

impl TokenCells {
    pub(crate) fn new(
        providers: Vec<Provider>,
        roots: Roots,
        tokens: Rc<RefCell<OAuthTokens>>,
    ) -> Self {
        let refresh: Refresher = Rc::new(move |provider, token| {
            let (provider, token, roots) = (provider.clone(), token.clone(), roots.clone());
            async move { envio_net::oauth::refresh(&provider, &roots, &token).await }.boxed_local()
        });
        Self::with(providers, refresh, Rc::new(crate::clock::now_ms), tokens)
    }

    pub(crate) fn with(
        providers: Vec<Provider>,
        refresh: Refresher,
        clock: Clock,
        tokens: Rc<RefCell<OAuthTokens>>,
    ) -> Self {
        Self {
            providers,
            refresh,
            clock,
            tokens,
            cells: RefCell::new(BTreeMap::new()),
        }
    }

    /// Sets or clears why the account's token of this kind is paused; while
    /// set, no refresh is made. The account's other kind is not touched.
    pub(crate) fn set_paused(&self, account: &str, kind: ProviderKind, problem: Option<Problem>) {
        if let Some(cell) = self.cells.borrow().get(&(account.to_owned(), kind)) {
            cell.state.borrow_mut().paused = problem;
        }
    }

    /// The account's handle: always the same cell, so its cached access
    /// token and its IMAP-worked flag outlive any one connect; `None` while no provider of that kind is configured.
    pub(crate) fn handle(&self, account: &str, kind: ProviderKind) -> Option<TokenHandle> {
        let provider = self.providers.iter().find(|p| p.kind == kind)?;
        let mut cells = self.cells.borrow_mut();
        let slot = (account.to_owned(), kind);
        if let Some(cell) = cells.get(&slot) {
            return Some(TokenHandle(Rc::clone(cell)));
        }
        let cell = Rc::new(Cell {
            key: account.to_owned(),
            kind,
            provider: provider.clone(),
            tokens: Rc::clone(&self.tokens),
            refresh: Rc::clone(&self.refresh),
            clock: Rc::clone(&self.clock),
            state: RefCell::new(State::default()),
        });
        cells.insert(slot, Rc::clone(&cell));
        Some(TokenHandle(cell))
    }

    /// The account is gone: its cached tokens, of both kinds, go with it.
    pub(crate) fn forget(&self, account: &str) {
        self.cells.borrow_mut().retain(|(key, _), _| key != account);
    }
}

/// Which token an account's backend uses: Graph accounts the Graph one,
/// every other account the IMAP one.
pub(crate) fn kind_of(settings: &envio_store::AccountSettings) -> ProviderKind {
    match settings.backend {
        envio_store::BackendKind::Graph => ProviderKind::MicrosoftGraph,
        envio_store::BackendKind::Imap => ProviderKind::Microsoft,
    }
}

/// Revoked -> SignIn; Temporary -> Temporary; Refused -> SignIn. A refused
/// refresh (blocked client, policy, bad request) cannot be fixed by
/// retrying, and `Problem::Refused` would not pause the account, so it
/// pauses with "Sign in again", the one action that can help.
pub(crate) fn token_problem(error: &TokenError) -> Problem {
    match error {
        TokenError::Temporary => Problem::Temporary,
        TokenError::Revoked(_) | TokenError::Refused(_) => Problem::SignIn,
    }
}

/// SMTP over a bearer token. `5.7.139` in the text, or a sign-in refusal
/// with a fresh token while a bearer IMAP login has worked (the token is
/// good, SMTP is the problem), becomes a refusal with a hint; everything
/// else is unchanged.
pub(crate) fn bearer_send_error(error: NetError, token_fresh: bool, imap_ok: bool) -> NetError {
    let text = match &error {
        NetError::SignIn(text) | NetError::Refused(text) => text,
        _ => return error,
    };
    let disabled = text.contains("5.7.139");
    let sign_in_with_good_token = matches!(error, NetError::SignIn(_)) && token_fresh && imap_ok;
    if disabled || sign_in_with_good_token {
        let hint = crate::text::outbox::smtp_disabled_hint();
        return NetError::Refused(format!("{text} {hint}"));
    }
    error
}

#[cfg(test)]
mod tests;
