//! The Outbox of a Microsoft (OAuth) account: a send whose token fails is
//! held, not lost, and never sent twice (#219 T6). Each test names its
//! invariant (a) to (h) from the plan.

mod support;

use std::time::{Duration, Instant};

use envio_core::{AccountStatus, Command, Event, OutboxDecision, OutboxStatus};
use envio_store::{FolderRole, Outbox, SendState};
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

fn sent(r: &Rig) -> usize {
    *r.peers.sent.lock().unwrap()
}

/// Waits (up to `secs`) for the first event `done` accepts, remembering
/// every account status on the way. `Harness::wait` drops what it skips,
/// which would hide a pause; this keeps it.
fn pump(r: &Rig, seen: &mut Vec<AccountStatus>, secs: u64, done: impl Fn(&Event) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let event =
            r.h.events
                .recv_timeout(left)
                .unwrap_or_else(|_| panic!("no matching event in {secs} s; statuses {seen:?}"));
        if let Some((status, _)) = status_of(&event, &r.key) {
            seen.push(status);
        }
        if done(&event) {
            return;
        }
    }
}

/// Records what is still in the channel.
fn drain_into(r: &Rig, seen: &mut Vec<AccountStatus>) {
    while let Ok(event) = r.h.events.try_recv() {
        if let Some((status, _)) = status_of(&event, &r.key) {
            seen.push(status);
        }
    }
}

fn drain(r: &Rig) {
    while r.h.events.try_recv().is_ok() {}
}

fn has_failed_row(event: &Event) -> bool {
    matches!(event, Event::Outbox { items }
        if items.iter().any(|row| matches!(row.status, OutboxStatus::Failed { .. })))
}

fn failed_reason(r: &Rig) -> String {
    let rows = outbox_rows(&r.h);
    let OutboxStatus::Failed { reason } = &rows[0].status else {
        panic!("{rows:?}");
    };
    reason.clone()
}

/// "Sign in again" completed.
///
/// TODO(#219 T7): replace this helper. T7 refuses `UpdatePassword` for an
/// OAuth account (a Notice) and adds the redirect path; until then
/// `UpdatePassword` is the one command that runs the worker's `resume`, and
/// (c) would hang, loudly, once T7 lands.
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

/// Restarts the core and syncs again: a pause is not kept across a start,
/// so the account tries again and ends the same way.
fn restart_and_sync(r: &mut Rig) {
    r.h.restart();
    drain(r);
    r.h.send(Command::AppActive(true));
}

#[test]
fn a_sign_in_refusal_before_any_imap_login_holds_the_item_and_pauses() {
    // (a) Nothing is accepted and nothing works: SMTP is never reached
    // (IMAP comes first), the account pauses, the item stays Queued, also
    // after a restart that syncs again. Twice: IMAP refuses a fresh token,
    // and the refresh is revoked.
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
        restart_and_sync(&mut r);
        wait_paused(&r);
        settle();
        assert_eq!(
            outbox_rows(&r.h)[0].status,
            OutboxStatus::Queued,
            "{revoked}"
        );
        assert_eq!(stored_state(&r, &item), SendState::Queued);
        assert_eq!(sent(&r), 0, "{revoked}");
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
    restart_and_sync(&mut r);
    wait_paused(&r);
    settle();
    assert_eq!(outbox_rows(&r.h)[0].status, OutboxStatus::Queued);
    assert_eq!(sent(&r), 0);
}

#[test]
fn smtp_is_only_reached_after_imap_signed_in_even_with_sent_cached() {
    // (a) The shape the plan fears, "SMTP before any bearer IMAP login", is
    // not reachable: even with the Sent folder cached (so a send needs no
    // folder list) the first run of a process is a full run that logs in to
    // IMAP first. `imap_ok` is therefore already set when SMTP is tried, and
    // the branch where it is not is pinned by `bearer_tests.rs`
    // (`a_fresh_token_smtp_refuses_before_imap_worked_is_a_sign_in_problem`).
    let mut r = rig(&["at-1"], Duration::ZERO, |_, _| {
        tokens_reply("at-1", "rt-2")
    });
    r.h.seed_folder_with(&r.key, "Sent", FolderRole::Sent, &[]);
    queue(&r.h, &r.key);
    r.start();
    wait_until("SMTP was tried", || {
        !r.peers.smtp_tokens.lock().unwrap().is_empty()
    });
    assert_eq!(
        *r.peers.imap_tokens.lock().unwrap().first().unwrap(),
        "at-1"
    );
    wait_until("the message was sent", || sent(&r) == 1);
}

#[test]
fn a_token_endpoint_that_comes_back_sends_the_held_item() {
    // (b) The endpoint is down for the first two requests: Queued, the
    // account Offline with a retry time, then the normal backoff (about 1 s
    // and 2 s) sends it. A long wait: that is two backoffs under load.
    let mut r = rig(&["at-1"], Duration::ZERO, |n, _| {
        if n < 2 {
            (503, "{}".to_owned())
        } else {
            tokens_reply("at-1", "rt-2")
        }
    });
    let item = queue(&r.h, &r.key);
    r.start();
    let mut seen = Vec::new();
    pump(&r, &mut seen, 5, |e| {
        matches!(
            status_of(e, &r.key),
            Some((AccountStatus::Offline { .. }, _))
        )
    });
    assert_eq!(stored_state(&r, &item), SendState::Queued);
    assert_eq!(sent(&r), 0);
    let deadline = Instant::now() + Duration::from_secs(30);
    while sent(&r) != 1 {
        assert!(Instant::now() < deadline, "the held item was never sent");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(r.requests.lock().unwrap().len() >= 3);
    drain_into(&r, &mut seen);
    assert!(!seen.contains(&AccountStatus::SignInFailed), "{seen:?}");
}

#[test]
fn signing_in_again_sends_the_held_item() {
    // (c) The refresh is revoked once: paused with the item Queued and no
    // refresh while paused. After "Sign in again" it goes out.
    //
    // The plan has the sign-in store its new refresh token with
    // `OAuthTokens::set` before `resume`. The worker keeps the tokens in
    // memory (read at start), so a file written from the test is not seen
    // by it: here the token endpoint answers differently the second time
    // instead, and the request is checked to carry the stored token.
    // TODO(#219 T7): store the new token through the redirect path and
    // assert the second request carries it.
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
    let requests = r.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        support::bearer::refresh_token_of(&requests[1]).as_deref(),
        Some("rt-1")
    );
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
    let mut seen = Vec::new();
    wait_until("the message was sent", || sent(&r) == 1);
    settle();
    assert!(r.requests.lock().unwrap().len() >= 2, "it refreshed again");
    drain_into(&r, &mut seen);
    for status in seen {
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

/// (f) and (g): a refusal that fails the item with the server's text and
/// the hint, keeps the account up, survives a restart (also when it is
/// sent again before the server relents), and lets the user send again.
fn send_refusal_fails_the_item(refusal: &str, server_text: Option<&str>) {
    let mut r = rig(&["at-1"], Duration::ZERO, |_, _| {
        tokens_reply("at-1", "rt-2")
    });
    *r.peers.smtp_refusal.lock().unwrap() = refusal.to_owned();
    *r.peers.smtp_denied.lock().unwrap() = true;
    let item = queue(&r.h, &r.key);
    r.start();
    let mut seen = Vec::new();
    pump(&r, &mut seen, 5, has_failed_row);
    settle();
    drain_into(&r, &mut seen);
    // Every status the account went through, none of them a pause.
    assert!(
        !seen.contains(&AccountStatus::SignInFailed),
        "{refusal}: {seen:?}"
    );
    let reason = failed_reason(&r);
    assert!(reason.contains(HINT), "{reason}");
    if let Some(text) = server_text {
        assert!(reason.contains(text), "{reason}");
    }
    assert_eq!(sent(&r), 0);
    // Still Failed after a restart.
    r.h.restart();
    drain(&r);
    assert!(
        matches!(&outbox_rows(&r.h)[0].status, OutboxStatus::Failed { reason } if reason.contains(HINT))
    );
    // After the restart IMAP logs in first, so the same refusal still fails
    // the item (not a pause) when it is sent again.
    r.h.send(Command::AppActive(true));
    r.h.send(Command::DecideOutbox {
        item: item.clone(),
        decision: OutboxDecision::Retry,
    });
    let mut again = Vec::new();
    pump(&r, &mut again, 5, has_failed_row);
    settle();
    drain_into(&r, &mut again);
    assert!(!again.contains(&AccountStatus::SignInFailed), "{again:?}");
    assert!(failed_reason(&r).contains(HINT));
    // Once the server accepts, sending it again works.
    *r.peers.smtp_denied.lock().unwrap() = false;
    r.h.send(Command::DecideOutbox {
        item,
        decision: OutboxDecision::Retry,
    });
    wait_until("the message was sent", || sent(&r) == 1);
    assert_eq!(*r.peers.smtp_tokens.lock().unwrap().last().unwrap(), "at-1");
}

#[test]
fn a_disabled_smtp_mailbox_fails_the_item_and_keeps_the_account_up() {
    // (f)
    send_refusal_fails_the_item(DISABLED_535, Some("SmtpClientAuthentication is disabled"));
}

#[test]
fn a_bare_refusal_after_a_working_imap_login_fails_the_item_too() {
    // (g) The reason carries the client's wording for the 334 challenge the
    // fake (and Microsoft) send before a bare 535, not the server's text,
    // so only the hint is asserted. See the hand-back note on #326.
    send_refusal_fails_the_item(BARE_535, None);
}

#[test]
fn a_password_account_refused_by_smtp_is_still_a_sign_in_problem() {
    // (h) Regression: the same 535 5.7.3 for a password account pauses it
    // and keeps the item Queued, with no hint and no Failed item. The fake
    // SMTP offers PLAIN and LOGIN, so it really receives the password AUTH
    // and answers 535.
    let ca = Ca::new();
    let peers = Peers::new(&["at-1"]);
    *peers.smtp_passwords.lock().unwrap() = true;
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
    // The server saw a password AUTH and refused it with the 535.
    assert!(
        !peers.smtp_auths.lock().unwrap().is_empty(),
        "no password AUTH reached SMTP"
    );
    assert!(peers.smtp_tokens.lock().unwrap().is_empty());
    let items = Outbox::open(&h.paths().outbox()).unwrap().items().unwrap();
    assert_eq!(items[0].id, item);
    assert_eq!(items[0].meta.state, SendState::Queued);
    assert_eq!(*peers.sent.lock().unwrap(), 0);
}
