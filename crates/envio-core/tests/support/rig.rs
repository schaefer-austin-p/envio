//! A Microsoft (OAuth) account rig for the core's integration tests: fake
//! token endpoint, IMAP and SMTP, and an unstarted harness pointed at them.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use envio_core::{AccountStatus, Command, Event};
use envio_net::oauth::Provider;
use envio_store::{NewOutboxItem, Outbox};

use super::Harness;
use super::bearer::{Ca, Peers, Reply, imap_server, smtp_server, token_endpoint};

pub const ME: &str = "me@outlook.com";
pub const HINT: &str = "Ask your administrator to allow Authenticated SMTP for this mailbox";

pub const EML: &[u8] = b"From: me@outlook.com\r\nTo: Ada <ada@example.org>\r\n\
Subject: Plans\r\nMessage-ID: <q1@example.org>\r\nMIME-Version: 1.0\r\n\
Content-Type: text/plain; charset=utf-8\r\n\r\nSee you\r\n";

pub struct Rig {
    pub h: Harness,
    pub key: String,
    pub peers: Arc<Peers>,
    /// The form body of every token request.
    pub requests: Arc<Mutex<Vec<String>>>,
}

/// An unstarted OAuth account (refresh token `rt-1`) whose token endpoint
/// answers with `reply`, and mail servers that accept the tokens `valid`.
pub fn rig(
    valid: &[&str],
    delay: Duration,
    reply: impl Fn(usize, &str) -> Reply + Send + Sync + 'static,
) -> Rig {
    let ca = Ca::new();
    let peers = Peers::new(valid);
    let (token_port, requests) = token_endpoint(&ca, delay, reply);
    let imap = imap_server(&ca, &peers);
    let smtp = smtp_server(&ca, &peers);
    let mut h = Harness::unstarted();
    h.roots = ca.roots();
    let mut provider = Provider::microsoft_template().with_client_id("test-client-id");
    provider.token_url = format!("https://127.0.0.1:{token_port}/token");
    h.oauth = vec![provider];
    let key = h.seed_oauth_account(ME, (imap, smtp), "rt-1");
    Rig {
        h,
        key,
        peers,
        requests,
    }
}

impl Rig {
    /// Starts the core in the foreground, so it syncs the account.
    pub fn start(&mut self) {
        self.h.start_core();
        self.h.send(Command::AppActive(true));
    }
}

pub fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "timed out: {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Lets anything still in flight happen.
pub fn settle() {
    std::thread::sleep(Duration::from_millis(400));
}

pub fn queue(h: &Harness, account: &str) -> String {
    let outbox = Outbox::open(&h.paths().outbox()).unwrap();
    outbox
        .enqueue(
            NewOutboxItem {
                send_after_ms: 0,
                account: account.to_owned(),
                message_id: "<q1@example.org>".into(),
                queued_at_ms: 5_000,
                from: ME.into(),
                recipients: vec!["ada@example.org".into()],
            },
            EML,
        )
        .unwrap()
        .id
}

pub fn status_of(e: &Event, account: &str) -> Option<(AccountStatus, String)> {
    match e {
        Event::AccountStatus {
            account: a,
            status,
            text,
        } if a == account => Some((status.clone(), text.clone())),
        _ => None,
    }
}

pub fn outbox_rows(h: &Harness) -> Vec<envio_core::OutboxRow> {
    h.send(Command::ListOutbox);
    h.wait(|e| match e {
        Event::Outbox { items } => Some(items.clone()),
        _ => None,
    })
}
