//! No OAuth secret (access token, refresh token, new refresh token) reaches
//! the log, on a sign-in that works or one that is refused. The logger is
//! global, so this binary owns it.

mod support;

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use envio_core::{AccountStatus, logging};
use log::LevelFilter;
use support::bearer::{revoked_reply, tokens_reply};
use support::rig::*;

#[derive(Clone, Default)]
struct Shared(Arc<Mutex<Vec<u8>>>);

impl Write for Shared {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

const SECRETS: [&str; 3] = ["at-secret-7", "rt-secret-8", "rt-1"];

#[test]
fn no_token_reaches_the_log() {
    let shared = Shared::default();
    logging::init_with_writer(Box::new(shared.clone()), LevelFilter::Info).unwrap();
    // A refresh that works (and rotates the refresh token), IMAP and SMTP
    // signing in with the access token, and an SMTP that refuses it.
    let mut works = rig(&["at-secret-7"], Duration::ZERO, |_, _| {
        tokens_reply("at-secret-7", "rt-secret-8")
    });
    *works.peers.smtp_denied.lock().unwrap() = true;
    queue(&works.h, &works.key);
    works.start();
    wait_until("the item failed", || {
        outbox_rows(&works.h)
            .iter()
            .any(|row| matches!(row.status, envio_core::OutboxStatus::Failed { .. }))
    });
    drop(works.h.core.take());
    // A refresh token that is revoked.
    let mut revoked = rig(&["at-secret-7"], Duration::ZERO, |_, _| revoked_reply());
    revoked.start();
    revoked
        .h
        .wait(|e| status_of(e, &revoked.key).filter(|(s, _)| *s == AccountStatus::SignInFailed));
    drop(revoked.h.core.take());
    let out = String::from_utf8(shared.0.lock().unwrap().clone()).unwrap();
    for secret in SECRETS {
        assert!(!out.contains(secret), "{secret} reached the log:\n{out}");
    }
}
