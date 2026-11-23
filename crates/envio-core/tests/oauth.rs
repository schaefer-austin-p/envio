//! A Microsoft (OAuth) account through the whole core: the access token is
//! fetched on demand, shared by IMAP and SMTP, refreshed only when it must
//! be, and a refusal ends in the right words (#219 T5).

mod support;

use std::time::Duration;

use envio_core::{AccountStatus, Command, OutboxStatus};
use support::bearer::{refresh_token_of, revoked_reply, tokens_reply};
use support::rig::*;

#[test]
fn the_account_signs_in_with_a_token_made_from_its_refresh_token() {
    let mut r = rig(&["at-1"], Duration::ZERO, |_, _| {
        tokens_reply("at-1", "rt-2")
    });
    r.start();
    wait_until("IMAP signed in", || {
        !r.peers.imap_tokens.lock().unwrap().is_empty()
    });
    assert_eq!(*r.peers.imap_tokens.lock().unwrap(), ["at-1"]);
    let requests = r.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert_eq!(refresh_token_of(&requests[0]).as_deref(), Some("rt-1"));
    // The refresh token the server rotated to is kept, for the next run.
    wait_until("the rotated refresh token is stored", || {
        let tokens = envio_core::oauth_tokens::OAuthTokens::open(&r.h.data_dir()).unwrap();
        tokens
            .get(&r.key, envio_net::oauth::ProviderKind::Microsoft)
            .is_some_and(|t| secrecy::ExposeSecret::expose_secret(t) == "rt-2")
    });
}

#[test]
fn a_revoked_refresh_token_pauses_the_account_with_sign_in_again() {
    let mut r = rig(&["at-1"], Duration::ZERO, |_, _| revoked_reply());
    r.start();
    let (status, text) =
        r.h.wait(|e| status_of(e, &r.key).filter(|(s, _)| *s == AccountStatus::SignInFailed));
    assert_eq!(status, AccountStatus::SignInFailed);
    assert_eq!(text, "Sign in again");
    settle();
    assert!(
        r.peers.imap_tokens.lock().unwrap().is_empty(),
        "IMAP was tried"
    );
    // Paused: asking for a refresh neither refreshes nor connects.
    let before = r.requests.lock().unwrap().len();
    r.h.send(Command::Refresh {
        account: Some(r.key.clone()),
        ticket: 0,
    });
    settle();
    assert_eq!(r.requests.lock().unwrap().len(), before);
    assert!(r.peers.imap_tokens.lock().unwrap().is_empty());
}

#[test]
fn a_fresh_token_the_server_refuses_pauses_without_a_second_refresh() {
    let mut r = rig(&[], Duration::ZERO, |_, _| tokens_reply("at-1", "rt-2"));
    r.start();
    let (_, text) =
        r.h.wait(|e| status_of(e, &r.key).filter(|(s, _)| *s == AccountStatus::SignInFailed));
    assert_eq!(text, "Sign in again");
    settle();
    // A token that was just made is not made again: that would only loop.
    assert_eq!(r.requests.lock().unwrap().len(), 1);
    assert_eq!(*r.peers.imap_tokens.lock().unwrap(), ["at-1"]);
}

#[test]
fn a_token_endpoint_that_is_down_is_offline_not_sign_in_failed() {
    let mut r = rig(&["at-1"], Duration::ZERO, |_, _| (503, "{}".to_owned()));
    r.start();
    let (status, text) = r
        .h
        .wait(|e| status_of(e, &r.key).filter(|(s, _)| matches!(s, AccountStatus::Offline { .. })));
    assert!(matches!(status, AccountStatus::Offline { .. }));
    assert!(!text.is_empty());
    settle();
    assert_ne!(
        r.h.wait(|e| status_of(e, &r.key).map(|(s, _)| s)),
        AccountStatus::SignInFailed
    );
    assert!(r.peers.imap_tokens.lock().unwrap().is_empty());
}

#[test]
fn imap_and_smtp_share_one_token() {
    let mut r = rig(&["at-1"], Duration::ZERO, |_, _| {
        tokens_reply("at-1", "rt-2")
    });
    let item = queue(&r.h, &r.key);
    r.start();
    wait_until("the message was sent", || {
        *r.peers.sent.lock().unwrap() == 1
    });
    assert_eq!(*r.peers.smtp_tokens.lock().unwrap(), ["at-1"]);
    assert_eq!(
        *r.peers.imap_tokens.lock().unwrap().first().unwrap(),
        "at-1"
    );
    assert_eq!(r.requests.lock().unwrap().len(), 1, "one token served both");
    let _ = item;
}

#[test]
fn smtp_refusing_a_good_token_fails_the_item_and_keeps_the_account_up() {
    for refusal in [
        "535 5.7.3 Authentication unsuccessful",
        "535 5.7.139 Authentication unsuccessful, SmtpClientAuthentication is disabled",
    ] {
        let mut r = rig(&["at-1"], Duration::ZERO, |_, _| {
            tokens_reply("at-1", "rt-2")
        });
        *r.peers.smtp_refusal.lock().unwrap() = refusal.to_owned();
        *r.peers.smtp_denied.lock().unwrap() = true;
        let item = queue(&r.h, &r.key);
        r.start();
        wait_until("the item failed", || {
            let rows = outbox_rows(&r.h);
            rows.iter()
                .any(|row| row.id == item && matches!(row.status, OutboxStatus::Failed { .. }))
        });
        let rows = outbox_rows(&r.h);
        let OutboxStatus::Failed { reason } = &rows[0].status else {
            panic!("{rows:?}");
        };
        assert!(reason.contains(HINT), "{refusal}: {reason}");
        assert_eq!(*r.peers.sent.lock().unwrap(), 0);
        // IMAP kept working and the account was never paused.
        assert!(!r.peers.imap_tokens.lock().unwrap().is_empty());
        settle();
        while let Ok(event) = r.h.events.try_recv() {
            if let Some((status, _)) = status_of(&event, &r.key) {
                assert_ne!(status, AccountStatus::SignInFailed, "{refusal}");
            }
        }
    }
}
