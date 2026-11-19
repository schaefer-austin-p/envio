//! A Microsoft (OAuth) world for the end-to-end tests: one injected
//! `Provider` whose token endpoint, IMAP and SMTP are the fakes envio-core's
//! own tests use (included by path, so there is one set of fakes).

#[path = "../../../envio-core/tests/support/mod.rs"]
pub mod core_support;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use envio_core::{Command, Event, OAuthOutcome};
use envio_net::oauth::Provider;
use envio_net::{Endpoint, Security};

pub use core_support::Harness;
pub use core_support::bearer::{Peers, Reply, id_token, revoked_reply};
pub use core_support::rig::{ME, settle, status_of, wait_until};

use core_support::bearer::{Ca, imap_server, smtp_server, token_endpoint};

pub struct World {
    pub h: Harness,
    pub peers: Arc<Peers>,
    /// The form body of every token request.
    pub requests: Arc<Mutex<Vec<String>>>,
    /// Every event seen so far, as text.
    pub tape: Vec<String>,
}

/// A started core with the fake provider and no account yet. The mail
/// servers accept the access tokens in `valid`; the token endpoint answers
/// the nth request with `reply(n, form)`.
pub fn world(
    valid: &[&str],
    reply: impl Fn(usize, &str) -> Reply + Send + Sync + 'static,
) -> World {
    let ca = Ca::new();
    let peers = Peers::new(valid);
    let (token_port, requests) = token_endpoint(&ca, Duration::ZERO, reply);
    let (imap, smtp) = (imap_server(&ca, &peers), smtp_server(&ca, &peers));
    let mut h = Harness::unstarted();
    h.roots = ca.roots();
    let mut provider = Provider::microsoft_template().with_client_id("test-client-id");
    provider.token_url = format!("https://127.0.0.1:{token_port}/token");
    provider.imap = Endpoint::new("127.0.0.1", imap, Security::Tls);
    provider.smtp = Endpoint::new("127.0.0.1", smtp, Security::Tls);
    h.oauth = vec![provider];
    h.start_core();
    h.send(Command::AppActive(true));
    World {
        h,
        peers,
        requests,
        tape: Vec::new(),
    }
}

impl World {
    pub fn wait<T>(&mut self, pick: impl Fn(&Event) -> Option<T>) -> T {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let event = self
                .h
                .events
                .recv_timeout(left)
                .unwrap_or_else(|_| panic!("no matching event; saw {:#?}", self.tape));
            self.tape.push(format!("{event:?}"));
            if let Some(found) = pick(&event) {
                return found;
            }
        }
    }

    /// Whatever else arrives within a moment.
    pub fn drain(&mut self) {
        settle();
        while let Ok(event) = self.h.events.try_recv() {
            self.tape.push(format!("{event:?}"));
        }
    }

    /// Starts a sign-in (`account` is `Some(key)` for "Sign in again") and
    /// returns the `state` of the browser link.
    pub fn begin(&mut self, email: &str, account: Option<String>) -> String {
        self.h.send(Command::StartOAuth {
            email: email.into(),
            account,
        });
        let url = self.wait(|e| match e {
            Event::OAuthBrowser { url, .. } => Some(url.clone()),
            _ => None,
        });
        url.split(['?', '&'])
            .find_map(|part| part.strip_prefix("state="))
            .expect("a state in the sign-in link")
            .to_owned()
    }

    /// The redirect of the browser, as the URL dispatcher hands it over.
    pub fn redirect(&self, state: &str) {
        self.h.send(Command::OAuthRedirect {
            url: format!("enviooauth://callback?code=code-1&state={state}").into(),
        });
    }

    pub fn outcome(&mut self) -> OAuthOutcome {
        self.wait(|e| match e {
            Event::OAuthDone { outcome, .. } => Some(*outcome),
            _ => None,
        })
    }

    /// A whole sign-in of a new account; returns its key.
    pub fn sign_in(&mut self, email: &str) -> String {
        let state = self.begin(email, None);
        self.redirect(&state);
        let key = self.wait(|e| match e {
            Event::AccountAdded { account } => Some(account.key.clone()),
            _ => None,
        });
        assert_eq!(self.outcome(), OAuthOutcome::Added);
        key
    }

    /// Writes a message in compose and sends it (queues it in the Outbox).
    pub fn send_mail(&self, account: &str) {
        self.h.send(Command::Send {
            compose: envio_core::ComposeId(1),
            draft: envio_core::DraftInput {
                account: account.to_owned(),
                to: vec!["ada@example.org".into()],
                subject: "Plans".into(),
                body: "See you".into(),
                ..envio_core::DraftInput::default()
            },
        });
    }

    pub fn sent(&self) -> usize {
        *self.peers.sent.lock().unwrap()
    }

    pub fn count(&self, word: &str) -> usize {
        self.tape.iter().filter(|e| e.contains(word)).count()
    }
}
