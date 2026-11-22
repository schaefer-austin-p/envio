//! The Graph client's token source: the account's Graph token cell, with the
//! problems it reports as the net errors the client understands.

use std::cell::RefCell;

use envio_net::NetError;
use envio_net::graph::Bearer;
use futures::future::LocalBoxFuture;
use secrecy::SecretString;

use super::token::{Granted, TokenHandle};
use crate::problem::Problem;

pub(crate) struct CoreBearer {
    handle: TokenHandle,
    /// The token last handed out: a `force` call renews that one, so a
    /// refresh another connection already made is not repeated.
    last: RefCell<Option<Granted>>,
}

impl CoreBearer {
    pub(crate) fn new(handle: TokenHandle) -> Self {
        Self {
            handle,
            last: RefCell::new(None),
        }
    }
}

/// A temporary problem is `Busy` (the client retries later); every other
/// one is a pause, which asks the user to sign in again.
fn net_error(problem: &Problem) -> NetError {
    match problem {
        Problem::Temporary => NetError::Busy("no access token yet".into()),
        _ => NetError::SignIn("Outlook.com access token".into()),
    }
}

impl Bearer for CoreBearer {
    fn bearer(&self, force: bool) -> LocalBoxFuture<'_, envio_net::Result<SecretString>> {
        Box::pin(async move {
            let used = self.last.borrow().clone();
            let granted = match (force, used) {
                (true, Some(used)) => self.handle.renew(&used).await,
                _ => self.handle.access(force).await,
            }
            .map_err(|p| net_error(&p))?;
            *self.last.borrow_mut() = Some(granted.clone());
            Ok(granted.token)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use envio_net::oauth::{Provider, ProviderKind, TokenError, Tokens};
    use futures::FutureExt;
    use secrecy::ExposeSecret;

    use super::*;
    use crate::oauth_tokens::OAuthTokens;
    use crate::worker::token::{Refresher, TokenCells};

    struct Rig {
        cells: TokenCells,
        handle: TokenHandle,
        calls: Rc<Cell<u32>>,
        _dir: tempfile::TempDir,
    }

    /// A Graph token account whose nth refresh yields `acc-n`, or fails
    /// with `error` when it is set.
    fn rig(error: Option<TokenError>) -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let mut tokens = OAuthTokens::open(dir.path()).unwrap();
        tokens
            .set("a", ProviderKind::MicrosoftGraph, SecretString::from("rt"))
            .unwrap();
        let calls = Rc::new(Cell::new(0));
        let counted = Rc::clone(&calls);
        let refresh: Refresher = Rc::new(move |_, _| {
            counted.set(counted.get() + 1);
            let n = counted.get();
            let error = error.clone();
            async move {
                match error {
                    Some(e) => Err(e),
                    None => Ok(Tokens {
                        access: SecretString::from(format!("acc-{n}")),
                        refresh: None,
                        expires_in: 3600,
                        identity: None,
                    }),
                }
            }
            .boxed_local()
        });
        let cells = TokenCells::with(
            vec![Provider::microsoft_graph_template()],
            refresh,
            {
                // Each reading is a millisecond later, so a token has an age.
                let now = Cell::new(1_000_000);
                Rc::new(move || {
                    now.set(now.get() + 1);
                    now.get()
                })
            },
            Rc::new(RefCell::new(tokens)),
        );
        let handle = cells.handle("a", ProviderKind::MicrosoftGraph).unwrap();
        Rig {
            cells,
            handle,
            calls,
            _dir: dir,
        }
    }

    fn text(r: envio_net::Result<SecretString>) -> String {
        r.unwrap().expose_secret().to_owned()
    }

    #[tokio::test]
    async fn a_token_is_cached_and_a_forced_call_renews_the_one_refused() {
        let rig = rig(None);
        let bearer = CoreBearer::new(rig.handle.clone());
        assert_eq!(text(bearer.bearer(false).await), "acc-1");
        assert_eq!(text(bearer.bearer(false).await), "acc-1");
        assert_eq!(rig.calls.get(), 1);
        // The 401: a fresh token, not the cached one.
        assert_eq!(text(bearer.bearer(true).await), "acc-2");
        assert_eq!(rig.calls.get(), 2);
        // Another caller's refresh in between is reused, not repeated.
        let other = CoreBearer::new(rig.handle.clone());
        assert_eq!(text(other.bearer(false).await), "acc-2");
        assert_eq!(rig.calls.get(), 2);
        drop(rig.cells);
    }

    #[tokio::test]
    async fn a_token_already_renewed_by_another_caller_is_not_renewed_again() {
        let rig = rig(None);
        let first = CoreBearer::new(rig.handle.clone());
        let second = CoreBearer::new(rig.handle.clone());
        assert_eq!(text(first.bearer(false).await), "acc-1");
        assert_eq!(text(second.bearer(false).await), "acc-1");
        assert_eq!(text(second.bearer(true).await), "acc-2");
        // The first one's 401 was for acc-1, which is already replaced.
        assert_eq!(text(first.bearer(true).await), "acc-2");
        assert_eq!(rig.calls.get(), 2);
    }

    #[tokio::test]
    async fn a_refused_refresh_is_sign_in_and_a_failed_one_is_busy() {
        let revoked = rig(Some(TokenError::Revoked("invalid_grant".into())));
        let bearer = CoreBearer::new(revoked.handle.clone());
        let error = bearer.bearer(false).await.unwrap_err();
        assert!(matches!(error, NetError::SignIn(_)), "{error}");
        let offline = rig(Some(TokenError::Temporary));
        let bearer = CoreBearer::new(offline.handle.clone());
        let error = bearer.bearer(false).await.unwrap_err();
        assert!(matches!(error, NetError::Busy(_)), "{error}");
    }

    #[tokio::test]
    async fn a_pause_of_any_kind_is_sign_in() {
        let rig = rig(None);
        rig.cells.set_paused(
            "a",
            ProviderKind::MicrosoftGraph,
            Some(Problem::Certificate(None)),
        );
        let bearer = CoreBearer::new(rig.handle.clone());
        let error = bearer.bearer(false).await.unwrap_err();
        assert!(matches!(error, NetError::SignIn(_)), "{error}");
        assert_eq!(rig.calls.get(), 0);
    }
}
