//! The Outbox of a Microsoft (OAuth) account: a send whose token fails is
//! held, not lost, and never sent twice (#219 T6). Each test names its
//! invariant (a) to (h) from the plan.

mod support;

use std::time::Duration;

use envio_core::{AccountStatus, Command, OutboxDecision, OutboxStatus};
use envio_store::{Outbox, SendState};
use support::bearer::{Ca, Peers, imap_server, revoked_reply, smtp_server, tokens_reply};
use support::rig::*;

const BARE_535: &str = "535 5.7.3 Authentication unsuccessful";
const DISABLED_535: &str =
    "535 5.7.139 Authentication unsuccessful, SmtpClientAuthentication is disabled";

fn stored_state(r: &Rig, id: &str) -> SendState {
    let items = Outbox::open(&r.h.paths().outbox())
        .unwrap()
        .items()
        .unwrap();
    items.into_iter().find(|i| i.id == id).unwrap().meta.state
}

/// Every status the account was reported in since the last call.
fn statuses(r: &Rig) -> Vec<AccountStatus> {
    let mut seen = Vec::new();
    while let Ok(event) = r.h.events.try_recv() {
        if let Some((status, _)) = status_of(&event, &r.key) {
            seen.push(status);
        }
    }
    seen
}

fn sent(r: &Rig) -> usize {
    *r.peers.sent.lock().unwrap()
}

/// "Sign in again" completed. T7's redirect path is not there yet;
/// `UpdatePassword` is the one command that runs the worker's `resume`.
fn resume(r: &Rig) {
    r.h.send(Command::UpdatePassword {
        account: r.key.clone(),
        password: "unused".to_owned().into(),
    });
}

fn wait_paused(r: &Rig) {
    let (status, text) =
        r.h.wait(|e| status_of(e, &r.key).filter(|(s, _)| *s == AccountStatus::SignInFailed));
    assert_eq!(status, AccountStatus::SignInFailed);
    assert_eq!(text, "Sign in again");
}

#[test]
fn a_sign_in_refusal_before_any_imap_login_holds_the_item_and_pauses() {
    // (a) The token is fresh and no bearer IMAP login has worked: a plain
    // sign-in problem. Twice: SMTP and IMAP both refuse, and the refresh
    // itself is revoked.
    for revoked in [false, true] {
        let mut r = rig(&[], Duration::ZERO, move |_, _| {
            if revoked {
                revoked_reply()
            } else {
                tokens_reply("at-1", "rt-2")
            }
        });
        let item = queue(&r.h, &r.key);
        r.start();
        wait_paused(&r);
        settle();
        assert_eq!(stored_state(&r, &item), SendState::Queued, "{revoked}");
        assert_eq!(sent(&r), 0);
        // Still queued after a restart, and still not sent.
        r.h.restart();
        let rows = outbox_rows(&r.h);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, OutboxStatus::Queued, "{revoked}");
        assert_eq!(stored_state(&r, &item), SendState::Queued);
        assert_eq!(sent(&r), 0);
    }
}

#[test]
fn a_refresh_revoked_after_imap_worked_still_holds_the_item_and_pauses() {
    // (a) IMAP signs in with a token that is already expired; SMTP's
    // refresh is then revoked. The item is held, not failed, even though a
    // bearer IMAP login had worked.
    let mut r = rig(&["at-1"], Duration::ZERO, |n, _| {
        if n == 0 {
            (
                200,
                r#"{"access_token":"at-1","refresh_token":"rt-2","expires_in":0}"#.to_owned(),
            )
        } else {
            revoked_reply()
        }
    });
    let item = queue(&r.h, &r.key);
    r.start();
    wait_paused(&r);
    settle();
    assert_eq!(*r.peers.imap_tokens.lock().unwrap(), ["at-1"]);
    assert!(r.peers.smtp_tokens.lock().unwrap().is_empty());
    assert_eq!(stored_state(&r, &item), SendState::Queued);
    assert_eq!(sent(&r), 0);
    r.h.restart();
    assert_eq!(outbox_rows(&r.h)[0].status, OutboxStatus::Queued);
}

#[test]
fn a_token_endpoint_that_comes_back_sends_the_held_item() {
    // (b) The endpoint is down for the first two requests: Queued, the
    // account Offline with a retry time, then the normal backoff sends it.
    let mut r = rig(&["at-1"], Duration::ZERO, |n, _| {
        if n < 2 {
            (503, "{}".to_owned())
        } else {
            tokens_reply("at-1", "rt-2")
        }
    });
    let item = queue(&r.h, &r.key);
    r.start();
    let (status, _) = r
        .h
        .wait(|e| status_of(e, &r.key).filter(|(s, _)| matches!(s, AccountStatus::Offline { .. })));
    assert!(matches!(status, AccountStatus::Offline { .. }));
    assert_eq!(stored_state(&r, &item), SendState::Queued);
    assert_eq!(sent(&r), 0);
    wait_until("the held item is sent", || sent(&r) == 1);
    assert!(r.requests.lock().unwrap().len() >= 3);
    assert!(!statuses(&r).contains(&AccountStatus::SignInFailed));
}

#[test]
fn signing_in_again_sends_the_held_item() {
    // (c) The refresh is revoked once: paused with the item Queued and no
    // refresh while paused. After "Sign in again" it goes out.
    let mut r = rig(&["at-2"], Duration::ZERO, |n, _| {
        if n == 0 {
            revoked_reply()
        } else {
            tokens_reply("at-2", "rt-3")
        }
    });
    let item = queue(&r.h, &r.key);
    r.start();
    wait_paused(&r);
    settle();
    assert_eq!(stored_state(&r, &item), SendState::Queued);
    assert_eq!(sent(&r), 0);
    assert!(r.peers.smtp_tokens.lock().unwrap().is_empty());
    assert_eq!(
        r.requests.lock().unwrap().len(),
        1,
        "no refresh while paused"
    );
    resume(&r);
    wait_until("the held item is sent", || sent(&r) == 1);
    assert_eq!(*r.peers.smtp_tokens.lock().unwrap(), ["at-2"]);
    assert_eq!(r.requests.lock().unwrap().len(), 2);
}

#[test]
fn an_expired_access_token_that_refreshes_sends_on_the_first_run() {
    // (d) Every access token is already expired when it arrives
    // (`expires_in` 0), so IMAP and SMTP each refresh. Nothing the user
    // would see: no pause, no Offline, no Failed item.
    let mut r = rig(&["at-1", "at-2", "at-3", "at-4"], Duration::ZERO, |n, _| {
        (
            200,
            format!(
                r#"{{"access_token":"at-{}","refresh_token":"rt-{}","expires_in":0}}"#,
                n + 1,
                n + 2
            ),
        )
    });
    queue(&r.h, &r.key);
    r.start();
    wait_until("the message was sent", || sent(&r) == 1);
    settle();
    assert!(r.requests.lock().unwrap().len() >= 2, "it refreshed again");
    for status in statuses(&r) {
        assert!(
            !matches!(
                status,
                AccountStatus::SignInFailed | AccountStatus::Offline { .. }
            ),
            "{status:?}"
        );
    }
    let rows = outbox_rows(&r.h);
    assert!(
        rows.iter()
            .all(|row| !matches!(row.status, OutboxStatus::Failed { .. })),
        "{rows:?}"
    );
    assert_eq!(sent(&r), 1);
}

#[test]
fn a_maybe_sent_item_is_never_sent_again_by_a_token_run() {
    // (e) The item is in the state a crash after the accepted token leaves
    // it in. A run that refreshes and signs in fine does not touch it.
    let mut r = rig(&["at-1"], Duration::ZERO, |_, _| {
        tokens_reply("at-1", "rt-2")
    });
    let item = queue(&r.h, &r.key);
    Outbox::open(&r.h.paths().outbox())
        .unwrap()
        .set_state(&item, SendState::MayHaveBeenSent)
        .unwrap();
    r.start();
    wait_until("IMAP signed in", || {
        !r.peers.imap_tokens.lock().unwrap().is_empty()
    });
    settle();
    assert_eq!(sent(&r), 0);
    assert!(r.peers.smtp_tokens.lock().unwrap().is_empty());
    assert_eq!(stored_state(&r, &item), SendState::MayHaveBeenSent);
    r.h.restart();
    r.h.send(Command::AppActive(true));
    settle();
    assert_eq!(sent(&r), 0);
    let rows = outbox_rows(&r.h);
    // (Sent was searched and came up empty: the user may decide, nothing more.)
    assert!(
        matches!(rows[0].status, OutboxStatus::MayHaveBeenSent { .. }),
        "{rows:?}"
    );
}

/// (f) and (g): a refusal that fails the item with the hint, keeps the
/// account up, survives a restart, and lets the user send again.
fn send_refusal_fails_the_item(refusal: &str) {
    let mut r = rig(&["at-1"], Duration::ZERO, |_, _| {
        tokens_reply("at-1", "rt-2")
    });
    *r.peers.smtp_refusal.lock().unwrap() = refusal.to_owned();
    *r.peers.smtp_denied.lock().unwrap() = true;
    let item = queue(&r.h, &r.key);
    r.start();
    wait_until("the item failed", || {
        outbox_rows(&r.h)
            .iter()
            .any(|row| matches!(row.status, OutboxStatus::Failed { .. }))
    });
    let rows = outbox_rows(&r.h);
    let OutboxStatus::Failed { reason } = &rows[0].status else {
        panic!("{rows:?}");
    };
    assert!(!reason.is_empty());
    assert!(reason.contains(HINT), "{reason}");
    // The account stays up and mail keeps syncing.
    settle();
    assert!(!r.peers.imap_tokens.lock().unwrap().is_empty());
    assert!(
        !statuses(&r).contains(&AccountStatus::SignInFailed),
        "{refusal}"
    );
    assert_eq!(sent(&r), 0);
    // Still Failed after a restart.
    r.h.restart();
    let rows = outbox_rows(&r.h);
    assert!(
        matches!(&rows[0].status, OutboxStatus::Failed { reason } if reason.contains(HINT)),
        "{rows:?}"
    );
    // Once the server accepts, sending it again works.
    *r.peers.smtp_denied.lock().unwrap() = false;
    r.h.send(Command::AppActive(true));
    r.h.send(Command::DecideOutbox {
        item: item.clone(),
        decision: OutboxDecision::Retry,
    });
    wait_until("the message was sent", || sent(&r) == 1);
    assert_eq!(*r.peers.smtp_tokens.lock().unwrap().last().unwrap(), "at-1");
}

#[test]
fn a_disabled_smtp_mailbox_fails_the_item_and_keeps_the_account_up() {
    // (f)
    send_refusal_fails_the_item(DISABLED_535);
}

#[test]
fn a_bare_refusal_after_a_working_imap_login_fails_the_item_too() {
    // (g)
    send_refusal_fails_the_item(BARE_535);
}

#[test]
fn a_password_account_refused_by_smtp_is_still_a_sign_in_problem() {
    // (h) Regression: the same 535 5.7.3 for a password account pauses it
    // and keeps the item Queued, with no hint and no Failed item.
    let ca = Ca::new();
    let peers = Peers::new(&["at-1"]);
    *peers.smtp_denied.lock().unwrap() = true;
    let (imap, smtp) = (imap_server(&ca, &peers), smtp_server(&ca, &peers));
    let mut h = support::Harness::unstarted();
    h.roots = ca.roots();
    let key = h.seed_account_on(ME, Some("pw"), ("127.0.0.1", imap));
    // The SMTP server is on its own port: point the account's SMTP at it.
    let path = h.paths().accounts();
    let text = std::fs::read_to_string(&path).unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&text).unwrap();
    json["accounts"][0]["settings"]["smtp"]["port"] = smtp.into();
    std::fs::write(&path, json.to_string()).unwrap();
    let item = queue(&h, &key);
    h.start_core();
    h.send(Command::AppActive(true));
    let (status, _) =
        h.wait(|e| status_of(e, &key).filter(|(s, _)| *s == AccountStatus::SignInFailed));
    assert_eq!(status, AccountStatus::SignInFailed);
    settle();
    let items = Outbox::open(&h.paths().outbox()).unwrap().items().unwrap();
    assert_eq!(items[0].id, item);
    assert_eq!(items[0].meta.state, SendState::Queued);
    assert_eq!(*peers.sent.lock().unwrap(), 0);
}
