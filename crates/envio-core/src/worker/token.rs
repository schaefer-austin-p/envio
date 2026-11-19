//! The OAuth access token of an account: cached, refreshed at most once at
//! a time per account, never written to disk. The refresh token is read
//! from `OAuthTokens` only, and a new one is stored there before the access
//! token is handed out.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

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

type Reply = Result<SecretString, TokenError>;
type Flight = Shared<LocalBoxFuture<'static, Reply>>;
/// Trades a refresh token for new tokens (the network call, replaceable in tests).
pub(crate) type Refresher =
    Rc<dyn Fn(&SecretString) -> LocalBoxFuture<'static, Result<Tokens, TokenError>>>;
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
}

struct Cell {
    key: String,
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
    pub(crate) async fn access(&self, force: bool) -> Result<SecretString, Problem> {
        if !force && let Some(token) = self.0.cached() {
            return Ok(token);
        }
        let (id, flight) = self.0.join_or_start();
        let reply = flight.await;
        self.0.finish(id);
        reply.map_err(|e| token_problem(&e))
    }

    /// Milliseconds since the cached access token was obtained.
    pub(crate) fn age_ms(&self) -> Option<i64> {
        let state = self.0.state.borrow();
        let at = state.access.as_ref()?.at_ms;
        Some((self.0.clock)().saturating_sub(at))
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
    fn cached(&self) -> Option<SecretString> {
        let state = self.state.borrow();
        let access = state.access.as_ref()?;
        let left = access.expires_ms.saturating_sub((self.clock)());
        (left > REFRESH_MARGIN_MS).then(|| access.token.clone())
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

    async fn run(&self) -> Reply {
        let Some(stored) = self.tokens.borrow().get(&self.key).cloned() else {
            return Err(TokenError::Revoked);
        };
        let at = (self.clock)();
        let result = (self.refresh)(&stored).await;
        let tokens = match result {
            Ok(tokens) => tokens,
            Err(error) => {
                if error == TokenError::Revoked {
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
            if let Err(error) = self.tokens.borrow_mut().set(&self.key, next) {
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
        Ok(tokens.access)
    }
}

/// The handles of all token accounts, one per account key.
pub(crate) struct TokenCells {
    providers: Vec<Provider>,
    refresh: Refresher,
    clock: Clock,
    tokens: Rc<RefCell<OAuthTokens>>,
    cells: RefCell<BTreeMap<String, Weak<Cell>>>,
}

impl TokenCells {
    pub(crate) fn new(providers: Vec<Provider>, roots: Roots, tokens: OAuthTokens) -> Self {
        let first = providers.first().cloned();
        let refresh: Refresher = Rc::new(move |token| {
            let token = token.clone();
            let roots = roots.clone();
            let provider = first.clone();
            async move {
                match provider {
                    Some(p) => envio_net::oauth::refresh(&p, &roots, &token).await,
                    None => Err(TokenError::Refused("no_provider".to_owned())),
                }
            }
            .boxed_local()
        });
        Self::with(
            providers,
            refresh,
            Rc::new(crate::clock::now_ms),
            Rc::new(RefCell::new(tokens)),
        )
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

    /// The account's handle, the same one for every caller while one is
    /// alive; `None` while no provider of that kind is configured.
    pub(crate) fn handle(&self, account: &str, kind: ProviderKind) -> Option<TokenHandle> {
        self.providers.iter().find(|p| p.kind == kind)?;
        let mut cells = self.cells.borrow_mut();
        if let Some(cell) = cells.get(account).and_then(Weak::upgrade) {
            return Some(TokenHandle(cell));
        }
        let cell = Rc::new(Cell {
            key: account.to_owned(),
            tokens: Rc::clone(&self.tokens),
            refresh: Rc::clone(&self.refresh),
            clock: Rc::clone(&self.clock),
            state: RefCell::new(State::default()),
        });
        cells.insert(account.to_owned(), Rc::downgrade(&cell));
        Some(TokenHandle(cell))
    }
}

/// Revoked -> SignIn; Temporary -> Temporary; Refused -> SignIn. A refused
/// refresh (blocked client, policy, bad request) cannot be fixed by
/// retrying, and `Problem::Refused` would not pause the account, so it
/// pauses with "Sign in again", the one action that can help.
pub(crate) fn token_problem(error: &TokenError) -> Problem {
    match error {
        TokenError::Temporary => Problem::Temporary,
        TokenError::Revoked | TokenError::Refused(_) => Problem::SignIn,
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
