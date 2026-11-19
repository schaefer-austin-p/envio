//! `connect_imap` and `connect_smtp` with a token: the refuse-then-refresh
//! rule (an old token refused is refreshed once; a fresh one is not), against
//! fake IMAP and SMTP servers and a clock the test moves.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use envio_net::oauth::{Provider, ProviderKind, Tokens};
use envio_store::{Security, Server};
use futures::FutureExt;
use secrecy::SecretString;

use super::{Login, LoginSource, SmtpConnectError, connect_imap, connect_smtp};
use crate::oauth_tokens::OAuthTokens;
use crate::problem::Problem;
use crate::servers::endpoint;
use crate::worker::token::{Refresher, TokenCells, TokenHandle};

#[allow(dead_code)]
#[path = "../../../tests/support/bearer.rs"]
mod bearer;
use bearer::{Ca, Peers, imap_server, smtp_server};

const T0: i64 = 1_000_000;

struct Rig {
    peers: Arc<Peers>,
    ca: Ca,
    imap: u16,
    smtp: u16,
    handle: TokenHandle,
    cells: TokenCells,
    /// Refreshes made so far.
    calls: Rc<Cell<u32>>,
    now: Rc<Cell<i64>>,
    _dir: tempfile::TempDir,
}

/// A token account whose nth refresh yields `acc-n`; the servers accept the
/// tokens in `valid`. The first token is already fetched, at `T0`.
async fn rig(valid: &[&str]) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let mut tokens = OAuthTokens::open(dir.path()).unwrap();
    tokens.set("a", SecretString::from("rt")).unwrap();
    let calls = Rc::new(Cell::new(0));
    let counted = Rc::clone(&calls);
    let refresh: Refresher = Rc::new(move |_, _| {
        counted.set(counted.get() + 1);
        let n = counted.get();
        async move {
            Ok(Tokens {
                access: SecretString::from(format!("acc-{n}")),
                refresh: None,
                expires_in: 3600,
                identity: None,
            })
        }
        .boxed_local()
    });
    let now = Rc::new(Cell::new(T0));
    let clock = Rc::clone(&now);
    let cells = TokenCells::with(
        vec![Provider::microsoft_template()],
        refresh,
        Rc::new(move || clock.get()),
        Rc::new(RefCell::new(tokens)),
    );
    let handle = cells.handle("a", ProviderKind::Microsoft).unwrap();
    handle.access(false).await.unwrap();
    let ca = Ca::new();
    let peers = Peers::new(valid);
    let (imap, smtp) = (imap_server(&ca, &peers), smtp_server(&ca, &peers));
    Rig {
        peers,
        ca,
        imap,
        smtp,
        handle,
        cells,
        calls,
        now,
        _dir: dir,
    }
}

impl Rig {
    fn login(&self, port: u16) -> Login {
        let server = Server {
            host: "127.0.0.1".into(),
            port,
            security: Security::Tls,
            pin: None,
        };
        Login {
            endpoint: endpoint(&server).unwrap(),
            username: "me@outlook.com".into(),
            source: LoginSource::Token(self.handle.clone()),
        }
    }

    /// The first token is now older than a fresh one may be.
    fn age(&self) {
        self.now.set(T0 + 120_000);
    }

    async fn imap(&self) -> Result<(), Problem> {
        connect_imap(&self.login(self.imap), &self.ca.roots())
            .await
            .map(drop)
    }

    async fn smtp(&self) -> Result<(), SmtpConnectError> {
        connect_smtp(&self.login(self.smtp), &self.ca.roots())
            .await
            .map(drop)
    }

    fn imap_seen(&self) -> Vec<String> {
        self.peers.imap_tokens.lock().unwrap().clone()
    }

    fn smtp_seen(&self) -> Vec<String> {
        self.peers.smtp_tokens.lock().unwrap().clone()
    }
}

#[tokio::test]
async fn an_old_token_the_imap_server_refuses_is_refreshed_once_and_retried() {
    let r = rig(&["acc-2"]).await;
    r.age();
    r.imap().await.unwrap();
    assert_eq!(r.imap_seen(), ["acc-1", "acc-2"]);
    assert_eq!(r.calls.get(), 2);
    assert!(r.handle.imap_ok());
}

#[tokio::test]
async fn a_fresh_token_the_imap_server_refuses_is_not_refreshed() {
    let r = rig(&[]).await;
    assert!(matches!(r.imap().await, Err(Problem::SignIn)));
    assert_eq!(r.imap_seen(), ["acc-1"]);
    assert_eq!(r.calls.get(), 1);
    assert!(!r.handle.imap_ok());
}

#[tokio::test]
async fn imap_refusing_the_refreshed_token_too_stops_after_one_retry() {
    let r = rig(&[]).await;
    r.age();
    assert!(matches!(r.imap().await, Err(Problem::SignIn)));
    assert_eq!(r.imap_seen(), ["acc-1", "acc-2"]);
    assert_eq!(r.calls.get(), 2);
}

#[tokio::test]
async fn an_old_token_the_smtp_server_refuses_is_refreshed_once_and_retried() {
    let r = rig(&["acc-2"]).await;
    r.age();
    assert!(r.smtp().await.is_ok());
    assert_eq!(r.smtp_seen(), ["acc-1", "acc-2"]);
    assert_eq!(r.calls.get(), 2);
}

#[tokio::test]
async fn a_fresh_token_smtp_refuses_after_imap_worked_is_a_send_refusal() {
    let r = rig(&["acc-1"]).await;
    *r.peers.smtp_denied.lock().unwrap() = true;
    r.handle.set_imap_ok();
    let hint = crate::text::outbox::smtp_disabled_hint();
    match r.smtp().await {
        Err(SmtpConnectError::SendRefused(text)) => assert!(text.ends_with(&hint), "{text}"),
        _ => panic!("expected a send refusal"),
    }
    assert_eq!(r.smtp_seen(), ["acc-1"]);
    assert_eq!(r.calls.get(), 1);
}

#[tokio::test]
async fn a_fresh_token_smtp_refuses_before_imap_worked_is_a_sign_in_problem() {
    let r = rig(&["acc-1"]).await;
    *r.peers.smtp_denied.lock().unwrap() = true;
    assert!(matches!(
        r.smtp().await,
        Err(SmtpConnectError::Problem(Problem::SignIn))
    ));
    assert_eq!(r.smtp_seen(), ["acc-1"]);
}

#[tokio::test]
async fn an_old_token_smtp_keeps_refusing_is_refreshed_once_then_a_send_refusal() {
    let r = rig(&["acc-1", "acc-2"]).await;
    *r.peers.smtp_denied.lock().unwrap() = true;
    r.handle.set_imap_ok();
    r.age();
    assert!(matches!(
        r.smtp().await,
        Err(SmtpConnectError::SendRefused(_))
    ));
    assert_eq!(r.smtp_seen(), ["acc-1", "acc-2"]);
    assert_eq!(r.calls.get(), 2);
}

#[tokio::test]
async fn smtp_disabled_by_the_mailbox_is_refused_at_once_even_with_an_old_token() {
    let r = rig(&["acc-1"]).await;
    *r.peers.smtp_denied.lock().unwrap() = true;
    *r.peers.smtp_refusal.lock().unwrap() =
        "535 5.7.139 Authentication unsuccessful, SmtpClientAuthentication is disabled".into();
    r.age();
    assert!(matches!(
        r.smtp().await,
        Err(SmtpConnectError::SendRefused(_))
    ));
    assert_eq!(r.smtp_seen(), ["acc-1"]);
    assert_eq!(r.calls.get(), 1, "no refresh for a disabled mailbox");
}

#[tokio::test]
async fn an_imap_refusal_after_smtp_refreshed_retries_once_with_the_new_token() {
    // Both used the old token; SMTP is refused first and refreshes, so by the
    // time IMAP's refusal arrives the cached token is young. IMAP's token was
    // old, so it still gets its one retry (with the token SMTP fetched).
    let r = rig(&["acc-2"]).await;
    *r.peers.imap_delay_ms.lock().unwrap() = 400;
    r.age();
    let (imap, smtp) = futures::join!(r.imap(), r.smtp());
    assert!(imap.is_ok(), "{imap:?}");
    assert!(smtp.is_ok());
    assert_eq!(r.imap_seen(), ["acc-1", "acc-2"]);
    assert_eq!(r.smtp_seen(), ["acc-1", "acc-2"]);
    assert_eq!(r.calls.get(), 2, "one refresh served both");
}

#[tokio::test]
async fn an_smtp_refusal_after_imap_refreshed_retries_once_with_the_new_token() {
    let r = rig(&["acc-2"]).await;
    *r.peers.smtp_delay_ms.lock().unwrap() = 400;
    r.age();
    let (imap, smtp) = futures::join!(r.imap(), r.smtp());
    assert!(imap.is_ok());
    assert!(smtp.is_ok(), "{:?}", smtp.err().map(|_| "smtp failed"));
    assert_eq!(r.smtp_seen(), ["acc-1", "acc-2"]);
    assert_eq!(r.calls.get(), 2, "one refresh served both");
}

#[tokio::test]
async fn a_pause_that_lands_mid_connect_stops_the_forced_refresh() {
    let r = rig(&["acc-2"]).await;
    *r.peers.imap_delay_ms.lock().unwrap() = 400;
    r.age();
    let pause = async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        r.cells.set_paused("a", Some(Problem::Certificate(None)));
    };
    let (imap, ()) = futures::join!(r.imap(), pause);
    assert!(matches!(imap, Err(Problem::Certificate(None))), "{imap:?}");
    assert_eq!(r.calls.get(), 1, "no refresh for a paused account");
}
