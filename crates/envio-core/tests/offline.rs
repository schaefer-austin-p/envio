//! An account that can't be reached, resume, pausing, and the background.

mod support;

use std::net::TcpListener;
use std::time::Duration;

use envio_core::{AccountStatus, Command, Event};
use support::{Harness, now_ms};

fn status_of(account: &str) -> impl Fn(&Event) -> Option<(AccountStatus, String)> + '_ {
    move |event| match event {
        Event::AccountStatus {
            account: a,
            status,
            text,
        } if a == account => Some((status.clone(), text.clone())),
        _ => None,
    }
}

fn offline(account: &str) -> impl Fn(&Event) -> Option<i64> + '_ {
    let status = status_of(account);
    move |event| match status(event) {
        Some((AccountStatus::Offline { retry_at_ms }, _)) => Some(retry_at_ms),
        _ => None,
    }
}

fn started_online(email: &str) -> (Harness, String) {
    let mut h = Harness::unstarted();
    h.foreground = true;
    let key = h.seed_account(email, Some("pw"));
    (h, key)
}

#[test]
fn an_unreachable_server_goes_offline_with_a_retry_time() {
    let (mut h, key) = started_online("me@example.org");
    let before = now_ms();
    h.start_core();
    let (status, text) =
        h.wait(|e| status_of(&key)(e).filter(|(s, _)| matches!(s, AccountStatus::Offline { .. })));
    let AccountStatus::Offline { retry_at_ms } = status else {
        unreachable!()
    };
    assert_eq!(text, "Offline, retrying");
    assert!(retry_at_ms >= before + 750, "{retry_at_ms} vs {before}");
    assert!(retry_at_ms <= now_ms() + 1_000);
}

#[test]
fn the_second_failure_waits_longer() {
    let (mut h, key) = started_online("me@example.org");
    h.start_core();
    let first = h.wait(offline(&key));
    let second = h.wait(offline(&key));
    // The second attempt came after the first retry time, and waits 1.5 to 2 s.
    assert!(second - first >= 1_500, "{} then {}", first, second);
}

#[test]
fn resume_retries_at_once() {
    let (mut h, key) = started_online("me@example.org");
    h.start_core();
    let first = h.wait(offline(&key));
    h.send(Command::AppActive(true));
    let second = h.wait(offline(&key));
    let seen = now_ms();
    // The retry came from the resume: we heard of it before the first retry
    // time, which is at least 750 ms after the first failure.
    assert!(seen < first, "heard at {seen}, first retry at {first}");
    assert!(second >= seen, "{second} vs {seen}");
}

#[test]
fn an_account_without_a_password_is_paused_for_sign_in() {
    let mut h = Harness::unstarted();
    h.foreground = true;
    let key = h.seed_account("me@example.org", None);
    h.start_core();
    let (status, text) =
        h.wait(|e| status_of(&key)(e).filter(|(s, _)| *s == AccountStatus::SignInFailed));
    assert_eq!(status, AccountStatus::SignInFailed);
    assert_eq!(text, "Sign-in failed, update password");
}

#[test]
fn background_makes_no_connection_attempts() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut h = Harness::unstarted();
    h.foreground = false;
    h.seed_account_at("me@example.org", Some("pw"), port);
    h.start_core();
    std::thread::sleep(Duration::from_millis(1_500));
    assert!(
        listener.accept().is_err(),
        "the core connected in the background"
    );
}
