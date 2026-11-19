use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use envio_net::NetError;
use envio_net::oauth::{Provider, ProviderKind, TokenError, Tokens};
use futures::FutureExt;
use secrecy::{ExposeSecret, SecretString};

use super::*;

type Reply = Result<(&'static str, Option<&'static str>, u64), TokenError>;

/// A token endpoint that counts requests and answers from a queue (the
/// last answer repeats). It yields once before answering, so a second
/// caller can arrive while the first request is in flight.
struct Fake {
    calls: Cell<u32>,
    seen: RefCell<Vec<String>>,
    replies: RefCell<VecDeque<Reply>>,
    /// Runs while a request is in flight (the clock moving, say).
    during: RefCell<Option<Box<dyn Fn()>>>,
}

impl Fake {
    fn new(replies: Vec<Reply>) -> Rc<Self> {
        Rc::new(Self {
            calls: Cell::new(0),
            seen: RefCell::new(Vec::new()),
            replies: RefCell::new(replies.into()),
            during: RefCell::new(None),
        })
    }

    fn refresher(self: &Rc<Self>) -> Refresher {
        let fake = Rc::clone(self);
        Rc::new(move |_, token| {
            fake.calls.set(fake.calls.get() + 1);
            fake.seen
                .borrow_mut()
                .push(token.expose_secret().to_owned());
            let fake = Rc::clone(&fake);
            async move {
                tokio::task::yield_now().await;
                tokio::task::yield_now().await;
                if let Some(f) = &*fake.during.borrow() {
                    f();
                }
                let mut queue = fake.replies.borrow_mut();
                let reply = if queue.len() > 1 {
                    queue.pop_front().unwrap()
                } else {
                    queue.front().cloned().unwrap()
                };
                reply.map(|(access, refresh, expires_in)| Tokens {
                    access: SecretString::from(access),
                    refresh: refresh.map(SecretString::from),
                    expires_in,
                    identity: None,
                })
            }
            .boxed_local()
        })
    }
}

struct Setup {
    cells: TokenCells,
    fake: Rc<Fake>,
    now: Rc<Cell<i64>>,
    dir: tempfile::TempDir,
    tokens: Rc<RefCell<OAuthTokens>>,
}

fn setup(replies: Vec<Reply>) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let mut tokens = OAuthTokens::open(dir.path()).unwrap();
    tokens.set("a", SecretString::from("stored-1")).unwrap();
    tokens.set("b", SecretString::from("stored-b")).unwrap();
    let tokens = Rc::new(RefCell::new(tokens));
    let fake = Fake::new(replies);
    let now = Rc::new(Cell::new(1_000_000));
    let clock = Rc::clone(&now);
    let cells = TokenCells::with(
        vec![Provider::microsoft_template()],
        fake.refresher(),
        Rc::new(move || clock.get()),
        Rc::clone(&tokens),
    );
    Setup {
        cells,
        fake,
        now,
        dir,
        tokens,
    }
}

fn handle(s: &Setup, key: &str) -> TokenHandle {
    s.cells.handle(key, ProviderKind::Microsoft).unwrap()
}

fn text(r: Result<SecretString, Problem>) -> String {
    r.unwrap().expose_secret().to_owned()
}

#[tokio::test]
async fn cached_token_is_reused_for_ten_connects() {
    let s = setup(vec![Ok(("acc-1", None, 3600))]);
    let h = handle(&s, "a");
    for _ in 0..10 {
        assert_eq!(text(h.access(false).await), "acc-1");
    }
    assert_eq!(s.fake.calls.get(), 1);
}

#[tokio::test]
async fn the_cache_and_the_imap_flag_outlive_every_handle() {
    let s = setup(vec![Ok(("acc-1", None, 3600)), Ok(("acc-2", None, 3600))]);
    let first = handle(&s, "a");
    assert_eq!(text(first.access(false).await), "acc-1");
    first.set_imap_ok();
    drop(first);
    // A later connect asks for the handle again: same token, same flag.
    let again = handle(&s, "a");
    assert_eq!(text(again.access(false).await), "acc-1");
    assert!(again.imap_ok());
    assert_eq!(s.fake.calls.get(), 1);
    // A removed account's token does not come back with a new one.
    drop(again);
    s.cells.forget("a");
    let reborn = handle(&s, "a");
    assert!(!reborn.imap_ok());
    assert_eq!(text(reborn.access(false).await), "acc-2");
}

#[tokio::test]
async fn token_inside_the_margin_refreshes_and_outside_it_does_not() {
    let s = setup(vec![Ok(("acc-1", None, 600)), Ok(("acc-2", None, 600))]);
    let h = handle(&s, "a");
    assert_eq!(text(h.access(false).await), "acc-1");
    // 300_001 ms left: more than the margin.
    s.now.set(1_000_000 + 299_999);
    assert_eq!(text(h.access(false).await), "acc-1");
    assert_eq!(s.fake.calls.get(), 1);
    // Exactly the margin left: refreshed.
    s.now.set(1_000_000 + 300_000);
    assert_eq!(text(h.access(false).await), "acc-2");
    assert_eq!(s.fake.calls.get(), 2);
}

#[tokio::test]
async fn token_valid_for_less_than_the_margin_refreshes_every_time() {
    let s = setup(vec![Ok(("acc", None, 200))]);
    let h = handle(&s, "a");
    h.access(false).await.unwrap();
    h.access(false).await.unwrap();
    assert_eq!(s.fake.calls.get(), 2);
}

#[tokio::test]
async fn expiry_counts_from_the_start_of_the_request() {
    // The clock moves while the request runs; the lifetime counts from
    // its start, so a slow reply never makes a token look younger.
    let s = setup(vec![Ok(("acc", None, 400))]);
    let h = handle(&s, "a");
    h.access(false).await.unwrap();
    s.now.set(1_000_000 + 100_001);
    h.access(false).await.unwrap();
    assert_eq!(s.fake.calls.get(), 2);
}

#[tokio::test]
async fn a_slow_reply_does_not_make_the_token_look_younger() {
    let s = setup(vec![Ok(("acc", None, 400))]);
    let now = Rc::clone(&s.now);
    *s.fake.during.borrow_mut() = Some(Box::new(move || now.set(now.get() + 150_000)));
    let h = handle(&s, "a");
    h.access(false).await.unwrap();
    // Valid until 1_400_000 (counted from the request's start); the clock
    // is now at 1_150_000, so 250_000 ms are left: inside the margin.
    assert_eq!(h.age_ms(), Some(150_000));
    h.access(false).await.unwrap();
    assert_eq!(s.fake.calls.get(), 2);
}

#[tokio::test]
async fn a_late_waiter_does_not_clear_a_newer_flight() {
    let s = setup(vec![Ok(("acc", None, 0))]);
    let h = handle(&s, "a");
    let mut a = Box::pin(h.access(false));
    let mut b = Box::pin(h.access(true));
    assert!(futures::poll!(a.as_mut()).is_pending());
    assert!(futures::poll!(b.as_mut()).is_pending());
    // The first flight ends in a's poll; b has not looked yet.
    assert!(futures::poll!(a.as_mut()).is_ready());
    // A new flight starts, then b finishes the old one.
    let mut c = Box::pin(h.access(true));
    assert!(futures::poll!(c.as_mut()).is_pending());
    assert!(futures::poll!(b.as_mut()).is_ready());
    // d must join c's flight, not start a third.
    let mut d = Box::pin(h.access(false));
    assert!(futures::poll!(d.as_mut()).is_pending());
    let (c, d) = futures::join!(c, d);
    c.unwrap();
    d.unwrap();
    assert_eq!(s.fake.calls.get(), 2);
}

#[tokio::test]
async fn a_missing_lifetime_means_refresh_next_time() {
    let s = setup(vec![Ok(("acc", None, 0))]);
    let h = handle(&s, "a");
    h.access(false).await.unwrap();
    h.access(false).await.unwrap();
    assert_eq!(s.fake.calls.get(), 2);
}

#[tokio::test]
async fn huge_lifetime_does_not_overflow() {
    let s = setup(vec![Ok(("acc", None, u64::MAX))]);
    let h = handle(&s, "a");
    h.access(false).await.unwrap();
    h.access(false).await.unwrap();
    assert_eq!(s.fake.calls.get(), 1);
}

#[tokio::test]
async fn force_refreshes_a_good_token() {
    let s = setup(vec![Ok(("acc-1", None, 3600)), Ok(("acc-2", None, 3600))]);
    let h = handle(&s, "a");
    assert_eq!(text(h.access(false).await), "acc-1");
    assert_eq!(text(h.access(true).await), "acc-2");
    assert_eq!(text(h.access(false).await), "acc-2");
    assert_eq!(s.fake.calls.get(), 2);
}

#[tokio::test]
async fn two_concurrent_callers_make_one_request() {
    let s = setup(vec![Ok(("acc-1", None, 3600)), Ok(("other", None, 3600))]);
    let h = handle(&s, "a");
    let (x, y) = futures::join!(h.access(false), h.access(false));
    assert_eq!(text(x), "acc-1");
    assert_eq!(text(y), "acc-1");
    assert_eq!(s.fake.calls.get(), 1);
}

#[tokio::test]
async fn forced_caller_during_a_refresh_joins_it() {
    let s = setup(vec![Ok(("acc-1", None, 3600)), Ok(("other", None, 3600))]);
    let h = handle(&s, "a");
    let (x, y) = futures::join!(h.access(false), h.access(true));
    assert_eq!(text(x), "acc-1");
    assert_eq!(text(y), "acc-1");
    assert_eq!(s.fake.calls.get(), 1);
}

#[tokio::test]
async fn two_handles_of_one_account_share_the_flight() {
    let s = setup(vec![Ok(("acc-1", None, 3600)), Ok(("other", None, 3600))]);
    let (h1, h2) = (handle(&s, "a"), handle(&s, "a"));
    let (x, y) = futures::join!(h1.access(false), h2.access(false));
    assert_eq!(text(x), text(y));
    assert_eq!(s.fake.calls.get(), 1);
}

#[tokio::test]
async fn different_accounts_refresh_independently() {
    let s = setup(vec![Ok(("acc", None, 3600))]);
    let (a, b) = (handle(&s, "a"), handle(&s, "b"));
    let (x, y) = futures::join!(a.access(false), b.access(false));
    x.unwrap();
    y.unwrap();
    assert_eq!(s.fake.calls.get(), 2);
    let mut seen = s.fake.seen.borrow().clone();
    seen.sort();
    assert_eq!(seen, ["stored-1", "stored-b"]);
}

#[tokio::test]
async fn concurrent_callers_share_a_failure_and_the_next_call_tries_again() {
    let s = setup(vec![Err(TokenError::Temporary), Ok(("acc-2", None, 3600))]);
    let h = handle(&s, "a");
    let (x, y) = futures::join!(h.access(false), h.access(false));
    assert_eq!(x.unwrap_err(), Problem::Temporary);
    assert_eq!(y.unwrap_err(), Problem::Temporary);
    assert_eq!(s.fake.calls.get(), 1);
    assert_eq!(text(h.access(false).await), "acc-2");
    assert_eq!(s.fake.calls.get(), 2);
}

#[tokio::test]
async fn a_caller_that_gives_up_does_not_stop_the_refresh_for_the_other() {
    let s = setup(vec![Ok(("acc-1", None, 3600))]);
    let h = handle(&s, "a");
    // The first caller starts the request and is dropped (the background
    // open is aborted when a step needs the session).
    assert!(h.access(false).now_or_never().is_none());
    assert_eq!(text(h.access(false).await), "acc-1");
    assert_eq!(s.fake.calls.get(), 1);
}

#[tokio::test]
async fn new_refresh_token_is_stored_before_the_access_token_is_returned() {
    let s = setup(vec![Ok(("acc-1", Some("stored-2"), 3600))]);
    let h = handle(&s, "a");
    h.access(false).await.unwrap();
    let on_disk = OAuthTokens::open(s.dir.path()).unwrap();
    assert_eq!(on_disk.get("a").unwrap().expose_secret(), "stored-2");
    assert_eq!(on_disk.get("b").unwrap().expose_secret(), "stored-b");
}

#[tokio::test]
async fn the_next_refresh_uses_the_new_refresh_token() {
    let s = setup(vec![
        Ok(("acc-1", Some("stored-2"), 0)),
        Ok(("acc-2", None, 0)),
    ]);
    let h = handle(&s, "a");
    h.access(false).await.unwrap();
    h.access(false).await.unwrap();
    assert_eq!(*s.fake.seen.borrow(), ["stored-1", "stored-2"]);
}

#[tokio::test]
async fn a_reply_without_a_refresh_token_keeps_the_stored_one() {
    let s = setup(vec![Ok(("acc-1", None, 3600))]);
    handle(&s, "a").access(false).await.unwrap();
    let on_disk = OAuthTokens::open(s.dir.path()).unwrap();
    assert_eq!(on_disk.get("a").unwrap().expose_secret(), "stored-1");
}

#[tokio::test]
async fn a_failed_write_keeps_the_new_access_token_and_the_old_refresh_token() {
    let s = setup(vec![Ok(("acc-1", Some("stored-2"), 3600))]);
    // The atomic rename onto a non-empty directory fails.
    let target = s.dir.path().join(crate::oauth_tokens::FILE_NAME);
    std::fs::remove_file(&target).unwrap();
    std::fs::create_dir_all(target.join("x")).unwrap();
    let h = handle(&s, "a");
    assert_eq!(text(h.access(false).await), "acc-1");
    assert_eq!(
        s.tokens.borrow().get("a").unwrap().expose_secret(),
        "stored-1"
    );
    // Cached: no second request.
    assert_eq!(text(h.access(false).await), "acc-1");
    assert_eq!(s.fake.calls.get(), 1);
}

#[tokio::test]
async fn revoked_is_sign_in_and_forgets_the_token_and_the_imap_login() {
    let s = setup(vec![Ok(("acc-1", None, 3600)), Err(TokenError::Revoked)]);
    let h = handle(&s, "a");
    h.access(false).await.unwrap();
    h.set_imap_ok();
    assert!(h.imap_ok());
    assert_eq!(h.access(true).await.unwrap_err(), Problem::SignIn);
    assert!(!h.imap_ok());
    assert_eq!(h.age_ms(), None);
    // Nothing is served from before the revocation.
    assert_eq!(h.access(false).await.unwrap_err(), Problem::SignIn);
    assert_eq!(s.fake.calls.get(), 3);
}

#[tokio::test]
async fn a_temporary_failure_keeps_the_good_token_and_the_imap_login() {
    let s = setup(vec![Ok(("acc-1", None, 3600)), Err(TokenError::Temporary)]);
    let h = handle(&s, "a");
    h.access(false).await.unwrap();
    h.set_imap_ok();
    assert_eq!(h.access(true).await.unwrap_err(), Problem::Temporary);
    assert!(h.imap_ok());
    assert_eq!(text(h.access(false).await), "acc-1");
}

#[tokio::test]
async fn a_failed_first_refresh_caches_nothing() {
    let s = setup(vec![Err(TokenError::Temporary)]);
    let h = handle(&s, "a");
    assert!(h.access(false).await.is_err());
    assert_eq!(h.age_ms(), None);
    assert!(h.access(false).await.is_err());
    assert_eq!(s.fake.calls.get(), 2);
}

#[tokio::test]
async fn no_stored_refresh_token_is_sign_in_without_a_request() {
    let s = setup(vec![Ok(("acc", None, 3600))]);
    let h = handle(&s, "nobody");
    assert_eq!(h.access(false).await.unwrap_err(), Problem::SignIn);
    assert_eq!(s.fake.calls.get(), 0);
}

#[tokio::test]
async fn age_counts_from_when_the_token_was_obtained() {
    let s = setup(vec![Ok(("acc", None, 3600))]);
    let h = handle(&s, "a");
    assert_eq!(h.age_ms(), None);
    h.access(false).await.unwrap();
    s.now.set(1_000_000 + 42_000);
    assert_eq!(h.age_ms(), Some(42_000));
}

#[tokio::test]
async fn imap_ok_starts_false() {
    let s = setup(vec![Ok(("acc", None, 3600))]);
    assert!(!handle(&s, "a").imap_ok());
}

#[tokio::test]
async fn the_handle_keeps_its_state_while_another_clone_lives() {
    let s = setup(vec![Ok(("acc", None, 3600))]);
    let h = handle(&s, "a");
    h.access(false).await.unwrap();
    h.set_imap_ok();
    let again = handle(&s, "a");
    assert!(again.imap_ok());
    again.access(false).await.unwrap();
    assert_eq!(s.fake.calls.get(), 1);
}

#[test]
fn no_handle_without_a_provider() {
    let dir = tempfile::tempdir().unwrap();
    let tokens = Rc::new(RefCell::new(OAuthTokens::open(dir.path()).unwrap()));
    let cells = TokenCells::with(
        Vec::new(),
        Fake::new(vec![]).refresher(),
        Rc::new(|| 0),
        tokens,
    );
    assert!(cells.handle("a", ProviderKind::Microsoft).is_none());
}

#[test]
fn token_problem_table() {
    assert_eq!(token_problem(&TokenError::Revoked), Problem::SignIn);
    assert_eq!(token_problem(&TokenError::Temporary), Problem::Temporary);
    assert_eq!(
        token_problem(&TokenError::Refused("invalid_client".into())),
        Problem::SignIn
    );
}

fn refused_text(e: NetError) -> Option<String> {
    match e {
        NetError::Refused(text) => Some(text),
        _ => None,
    }
}

#[test]
fn bearer_send_error_table() {
    let hint = crate::text::outbox::smtp_disabled_hint();
    let disabled = || NetError::SignIn("535 5.7.139 SmtpClientAuthentication is disabled".into());
    let bare = || NetError::SignIn("535 5.7.3 Authentication unsuccessful".into());

    // 5.7.139 is a send refusal whatever the token's age or the IMAP login.
    for (fresh, imap_ok) in [(false, false), (true, false), (false, true), (true, true)] {
        let text = refused_text(bearer_send_error(disabled(), fresh, imap_ok)).unwrap();
        assert!(text.contains("5.7.139"), "the server's text stays");
        assert!(text.ends_with(&hint), "the hint follows it");
    }
    // A bare refusal with a fresh token after a good IMAP login is one too.
    let text = refused_text(bearer_send_error(bare(), true, true)).unwrap();
    assert!(text.contains("5.7.3") && text.ends_with(&hint));
    // Anything else stays a sign-in refusal.
    for (fresh, imap_ok) in [(true, false), (false, true), (false, false)] {
        assert!(matches!(
            bearer_send_error(bare(), fresh, imap_ok),
            NetError::SignIn(_)
        ));
    }
}

#[test]
fn bearer_send_error_leaves_other_errors_alone() {
    let busy = bearer_send_error(NetError::Busy("451 try later".into()), true, true);
    assert!(matches!(busy, NetError::Busy(_)));
    let timeout = bearer_send_error(NetError::Timeout, true, true);
    assert!(matches!(timeout, NetError::Timeout));
    // A plain refusal without the code is not given the hint.
    let plain = bearer_send_error(NetError::Refused("550 no".into()), true, true);
    assert_eq!(refused_text(plain).as_deref(), Some("550 no"));
}

#[test]
fn constants_match_the_plan() {
    assert_eq!(REFRESH_MARGIN_MS, 300_000);
    assert_eq!(FRESH_MS, 60_000);
}
