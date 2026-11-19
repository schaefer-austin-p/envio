//! Signing in with Microsoft, end to end, with no Microsoft (#219 T9): one
//! injected `Provider` whose token endpoint, IMAP and SMTP are the fakes of
//! envio-core's tests. Detect, sign in, sync and send over XOAUTH2, refresh,
//! revoke, "Sign in again", a mailbox that refuses SMTP, the real
//! Microsoft provider's Detect, and the empty provider list.

#[path = "support/oauth.rs"]
mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use envio_core::hooks::{SetupFake, fake_setup};
use envio_core::{AccountStatus, Command, Event, OAuthFailure, OAuthOutcome, OutboxStatus};
use envio_net::oauth::Provider;
use envio_net::{Endpoint, Security};
use support::*;

const HINT: &str = "Ask your administrator to allow Authenticated SMTP for this mailbox";

/// Every access token expires within the 5 minutes the core refreshes
/// ahead of, so each connect after the sign-in asks for a new one: the
/// tests' stand-in for time passing.
fn short(access: &str, refresh: &str) -> Reply {
    (
        200,
        format!(
            r#"{{"access_token":"{access}","refresh_token":"{refresh}","expires_in":60,"id_token":"{}"}}"#,
            id_token(Some(ME), None)
        ),
    )
}

fn has_pause(events: &[String]) -> bool {
    events
        .iter()
        .any(|e| e.contains("SignInFailed") || e.contains("Offline"))
}

#[test]
fn a_microsoft_account_signs_in_syncs_sends_and_refreshes_without_a_word() {
    // The code exchange gives at-1, which the servers do not accept: IMAP
    // only gets in if the core refreshes ahead (at-1 has 60 s left), and
    // SMTP then refreshes again.
    let mut w = world(&["at-2", "at-3"], |n, form| {
        if form.contains("grant_type=authorization_code") {
            short("at-1", "rt-1")
        } else {
            short(&format!("at-{}", n + 1), &format!("rt-{}", n + 1))
        }
    });
    // Detect offers the sign-in and the provider's servers.
    w.h.send(Command::Detect { email: ME.into() });
    let offer = w.wait(|e| match e {
        Event::Detected { oauth, setup, .. } => Some((oauth.clone(), setup.imap.host.clone())),
        _ => None,
    });
    assert_eq!(offer.0.expect("an offer").provider, "Microsoft");
    assert_eq!(offer.1, "127.0.0.1");

    let key = w.sign_in(ME);
    // The first sync signs in with the token of the code exchange.
    wait_until("IMAP signed in", || {
        !w.peers.imap_tokens.lock().unwrap().is_empty()
    });
    w.drain();

    // A message goes through SMTP XOAUTH2, with a refreshed token.
    w.send_mail(&key);
    wait_until("the message was sent", || w.sent() == 1);
    w.drain();
    assert!(!w.peers.smtp_tokens.lock().unwrap().is_empty());
    let requests = w.requests.lock().unwrap().clone();
    assert!(requests.len() >= 2, "{requests:?}");
    assert!(requests[0].contains("grant_type=authorization_code"));
    assert!(
        requests[1..]
            .iter()
            .all(|r| r.contains("grant_type=refresh_token")),
        "{requests:?}"
    );
    // The refresh happened with no event for the user.
    assert!(!has_pause(&w.tape), "{:#?}", w.tape);
    assert_eq!(w.count("Notice"), 0, "{:#?}", w.tape);
    // The token of the code exchange is already due for a refresh, so
    // IMAP and SMTP each signed in with a token of their own.
    let imap = w.peers.imap_tokens.lock().unwrap().clone();
    let smtp = w.peers.smtp_tokens.lock().unwrap().clone();
    assert_eq!(imap[0], "at-2", "IMAP did not get a refreshed token");
    assert_ne!(imap[0], smtp[0], "SMTP reused IMAP's token");
}

#[test]
fn a_revoked_token_pauses_the_account_and_signing_in_again_delivers_the_held_message() {
    let revoked = Arc::new(AtomicBool::new(false));
    let flag = revoked.clone();
    let mut w = world(
        &[
            "at-2", "at-3", "at-4", "at-5", "at-6", "at-7", "at-8", "at-9",
        ],
        move |n, form| {
            if form.contains("grant_type=authorization_code") {
                short(&format!("at-{}", n + 1), "rt-1")
            } else if flag.load(Ordering::SeqCst) {
                revoked_reply()
            } else {
                short(&format!("at-{}", n + 1), "rt-1")
            }
        },
    );
    // The code exchange's token is one the servers do not know, so the
    // first sync refreshes (the endpoint is not revoked yet) and signs in.
    let key = w.sign_in(ME);
    wait_until("IMAP signed in", || {
        !w.peers.imap_tokens.lock().unwrap().is_empty()
    });
    w.drain();

    // Microsoft revokes the refresh token; a message is sent.
    revoked.store(true, Ordering::SeqCst);
    w.send_mail(&key);
    let (status, text) =
        w.wait(|e| status_of(e, &key).filter(|(s, _)| *s == AccountStatus::SignInFailed));
    assert_eq!(status, AccountStatus::SignInFailed);
    assert_eq!(text, "Sign in again");
    settle();
    assert_eq!(w.sent(), 0);
    let rows = outbox(&mut w);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, OutboxStatus::Queued);

    // Sign in again: the redirect resumes the account, the message goes.
    revoked.store(false, Ordering::SeqCst);
    let state = w.begin("", Some(key.clone()));
    w.redirect(&state);
    assert_eq!(w.outcome(), OAuthOutcome::Resumed);
    wait_until("the held message is sent", || w.sent() == 1);
    // Delivered (the fake IMAP files nothing into Sent). The worker marks
    // the item after SMTP accepts it, so wait for that, not for one read.
    let mut rows = Vec::new();
    wait_until("the item is marked sent", || {
        rows = outbox(&mut w);
        rows.first()
            .is_some_and(|r| r.status == OutboxStatus::SentNotFiled)
    });
    assert_eq!(rows.len(), 1, "{rows:?}");
}

fn outbox(w: &mut World) -> Vec<envio_core::OutboxRow> {
    w.h.send(Command::ListOutbox);
    w.wait(|e| match e {
        Event::Outbox { items } => Some(items.clone()),
        _ => None,
    })
}

#[test]
fn a_mailbox_with_smtp_auth_disabled_fails_the_message_with_the_hint_and_stays_up() {
    let mut w = world(&["at-1", "at-2", "at-3"], |n, form| {
        if form.contains("grant_type=authorization_code") {
            short("at-1", "rt-1")
        } else {
            short(&format!("at-{}", n + 1), "rt-1")
        }
    });
    *w.peers.smtp_refusal.lock().unwrap() =
        "535 5.7.139 Authentication unsuccessful, SmtpClientAuthentication is disabled".into();
    *w.peers.smtp_denied.lock().unwrap() = true;
    let key = w.sign_in(ME);
    wait_until("IMAP signed in", || {
        !w.peers.imap_tokens.lock().unwrap().is_empty()
    });
    w.drain();
    w.send_mail(&key);
    w.wait(|e| match e {
        Event::Outbox { items }
            if items
                .iter()
                .any(|r| matches!(r.status, OutboxStatus::Failed { .. })) =>
        {
            Some(())
        }
        _ => None,
    });
    w.drain();
    let rows = outbox(&mut w);
    let OutboxStatus::Failed { reason } = &rows[0].status else {
        panic!("{rows:?}");
    };
    assert!(reason.contains(HINT), "{reason}");
    assert!(
        reason.contains("SmtpClientAuthentication is disabled"),
        "{reason}"
    );
    assert_eq!(w.sent(), 0);
    // The account was never paused: mail keeps syncing.
    assert!(!has_pause(&w.tape), "{:#?}", w.tape);
}

#[test]
fn detect_offers_the_real_microsoft_provider_with_its_hosts() {
    // The real `Provider::microsoft()` list, as the app's `Config` takes it:
    // the other tests inject a provider with fake hosts.
    assert!(Provider::microsoft().is_some());
    let mut h = Harness::unstarted();
    h.oauth = envio_core::default_oauth_providers();
    assert_eq!(h.oauth.len(), 1);
    h.start_core();
    h.send(Command::Detect {
        email: "x@outlook.com".into(),
    });
    let (setup, oauth) = h.wait(|e| match e {
        Event::Detected { setup, oauth, .. } => Some((setup.clone(), oauth.clone())),
        _ => None,
    });
    assert_eq!(oauth.expect("an offer").provider, "Microsoft");
    assert_eq!(setup.imap.host, "outlook.office365.com");
    assert_eq!(setup.smtp.host, "smtp.office365.com");
}

#[test]
fn without_a_provider_the_wizard_path_is_what_it_always_was() {
    // An app built with no OAuth provider (an empty list) keeps today's path.
    let mut h = Harness::unstarted();
    h.oauth = Vec::new();
    h.start_core();
    let setting = |host: &str| envio_net::autoconfig::Setting {
        endpoint: Endpoint::new(host, 993, Security::Tls),
        username: "x@outlook.com".into(),
    };
    fake_setup(
        "x@outlook.com",
        SetupFake {
            delay: Duration::ZERO,
            found: Some(envio_net::autoconfig::Found {
                imap: setting("imap.found.example"),
                smtp: setting("smtp.found.example"),
                source: envio_net::autoconfig::Source::Ispdb,
                confirm: false,
            }),
        },
    );
    h.send(Command::Detect {
        email: "x@outlook.com".into(),
    });
    let (setup, oauth, text) = h.wait(|e| match e {
        Event::Detected {
            setup, oauth, text, ..
        } => Some((setup.clone(), oauth.clone(), text.clone())),
        _ => None,
    });
    // No offer, the found servers, today's wording.
    assert_eq!(oauth, None);
    assert_eq!(setup.imap.host, "imap.found.example");
    assert_eq!(
        text,
        "Settings found in the Thunderbird configuration database"
    );
    // And a sign-in cannot be started: refused, no browser.
    h.send(Command::StartOAuth {
        email: "x@outlook.com".into(),
        account: None,
    });
    let outcome = h.wait(|e| match e {
        Event::OAuthBrowser { .. } => panic!("a sign-in began"),
        Event::OAuthDone { outcome, .. } => Some(*outcome),
        _ => None,
    });
    assert_eq!(outcome, OAuthOutcome::Failed(OAuthFailure::Unavailable));
}
