//! Signing in with Microsoft through the whole core: the commands, the
//! events and what ends up on disk (#219 T7). The token endpoint, IMAP and
//! SMTP are fakes; no Microsoft address and no client ID is involved.

mod support;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use envio_core::hooks::{SetupFake, fake_setup, set_oauth_timeout};
use envio_core::oauth_tokens::OAuthTokens;
use envio_core::secrets::Secrets;
use envio_core::{AccountStatus, Command, Event, OAuthFailure, OAuthOutcome, OutboxStatus};
use envio_net::oauth::Provider;
use envio_net::{Endpoint, Security};
use envio_store::Accounts;
use secrecy::ExposeSecret;
use support::Harness;
use support::bearer::{
    Ca, Peers, Reply, imap_server, refresh_token_of, smtp_server, token_endpoint, tokens_reply,
    tokens_reply_for,
};
use support::rig::*;

type Requests = Arc<Mutex<Vec<String>>>;

struct Flow {
    h: Harness,
    peers: Arc<Peers>,
    requests: Requests,
    /// Every event seen so far, as text.
    tape: Vec<String>,
}

/// A core with the fake Microsoft provider and no account yet.
fn flow(
    valid: &[&str],
    delay: Duration,
    reply: impl Fn(usize, &str) -> Reply + Send + Sync + 'static,
) -> Flow {
    let ca = Ca::new();
    let peers = Peers::new(valid);
    let (token_port, requests) = token_endpoint(&ca, delay, reply);
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
    Flow {
        h,
        peers,
        requests,
        tape: Vec::new(),
    }
}

/// A token endpoint that answers every code with `at-1` and `rt-new`, naming `email`.
fn plain(email: &'static str) -> impl Fn(usize, &str) -> Reply + Send + Sync + 'static {
    move |_, _| tokens_reply_for("at-1", "rt-new", Some(email), None)
}

impl Flow {
    fn wait<T>(&mut self, pick: impl Fn(&Event) -> Option<T>) -> T {
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

    /// Starts a sign-in; returns the `state` the browser URL carries.
    fn begin(&mut self, email: &str) -> String {
        self.h.send(Command::StartOAuth {
            email: email.into(),
            account: None,
        });
        let url = self.browser();
        state_of(&url)
    }

    fn browser(&mut self) -> String {
        self.wait(|e| match e {
            Event::OAuthBrowser { url, .. } => Some(url.clone()),
            _ => None,
        })
    }

    fn redirect(&self, query: &str) {
        self.h.send(Command::OAuthRedirect {
            url: secrecy::SecretString::from(format!("enviooauth://callback?{query}")),
        });
    }

    fn outcome(&mut self) -> OAuthOutcome {
        self.wait(|e| match e {
            Event::OAuthDone { outcome, .. } => Some(*outcome),
            _ => None,
        })
    }

    fn notice(&mut self) -> String {
        self.wait(|e| match e {
            Event::Notice { text } => Some(text.clone()),
            _ => None,
        })
    }

    fn account_added(&mut self) -> String {
        self.wait(|e| match e {
            Event::AccountAdded { account } => Some(account.key.clone()),
            _ => None,
        })
    }

    /// Whatever else arrives within a moment.
    fn drain(&mut self) {
        settle();
        while let Ok(event) = self.h.events.try_recv() {
            self.tape.push(format!("{event:?}"));
        }
    }

    fn count(&self, word: &str) -> usize {
        self.tape.iter().filter(|e| e.contains(word)).count()
    }

    fn saved(&self) -> Vec<envio_store::Account> {
        Accounts::open(&self.h.paths().accounts())
            .unwrap()
            .list()
            .to_vec()
    }
}

fn state_of(url: &str) -> String {
    url.split(['?', '&'])
        .find_map(|part| part.strip_prefix("state="))
        .unwrap()
        .to_owned()
}

fn refresh_of(h: &Harness, key: &str) -> Option<String> {
    OAuthTokens::open(&h.data_dir())
        .unwrap()
        .get(key, envio_net::oauth::ProviderKind::Microsoft)
        .map(|t| t.expose_secret().to_owned())
}

fn password_of(h: &Harness, key: &str) -> bool {
    Secrets::open(&h.data_dir()).unwrap().get(key).is_some()
}

// ---- Detect ----

#[test]
fn detect_offers_the_sign_in_and_the_providers_servers() {
    let mut f = flow(&[], Duration::ZERO, plain(ME));
    f.h.send(Command::Detect { email: ME.into() });
    let (setup, offer, text) = f.wait(|e| match e {
        Event::Detected {
            setup, oauth, text, ..
        } => Some((setup.clone(), oauth.clone(), text.clone())),
        _ => None,
    });
    let offer = offer.expect("an offer");
    assert_eq!(offer.provider, "Microsoft");
    assert_eq!(offer.name_hint, "me");
    assert_eq!(setup.imap.host, "127.0.0.1");
    assert_eq!(setup.source, Some(envio_net::autoconfig::Source::Provider));
    assert!(!setup.needs_confirm);
    assert!(text.contains("Sign in with Microsoft"), "{text}");
}

#[test]
fn detect_without_a_provider_is_what_it_always_was() {
    use envio_net::autoconfig::{Found, Setting, Source};
    let h = Harness::start();
    let setting = |host: &str| Setting {
        endpoint: Endpoint::new(host, 993, Security::Tls),
        username: "ada@example.org".into(),
    };
    fake_setup(
        "ada@example.org",
        SetupFake {
            delay: Duration::ZERO,
            found: Some(Found {
                imap: setting("imap.found.example"),
                smtp: setting("smtp.found.example"),
                source: Source::Ispdb,
                confirm: false,
            }),
        },
    );
    h.send(Command::Detect {
        email: "ada@example.org".into(),
    });
    let (setup, oauth, text) = h.wait(|e| match e {
        Event::Detected {
            setup, oauth, text, ..
        } => Some((setup.clone(), oauth.clone(), text.clone())),
        _ => None,
    });
    assert_eq!(oauth, None);
    assert_eq!(setup.imap.host, "imap.found.example");
    assert_eq!(setup.source, Some(Source::Ispdb));
    assert_eq!(
        text,
        "Settings found in the Thunderbird configuration database"
    );
}

#[test]
fn starting_without_a_provider_is_refused_and_opens_no_browser() {
    let mut h = Harness::start();
    h.send(Command::StartOAuth {
        email: ME.into(),
        account: None,
    });
    let mut refused = 0;
    let deadline = Instant::now() + Duration::from_millis(800);
    while Instant::now() < deadline {
        match h.events.recv_timeout(Duration::from_millis(50)) {
            Ok(Event::OAuthDone { outcome, .. }) => {
                assert_eq!(outcome, OAuthOutcome::Failed(OAuthFailure::Unavailable));
                refused += 1;
            }
            Ok(Event::OAuthBrowser { .. }) => panic!("a sign-in began"),
            _ => {}
        }
    }
    assert_eq!(refused, 1);
    drop(h.core.take());
}

#[test]
fn starting_is_refused_for_other_domains_duplicates_and_password_accounts() {
    let ca = Ca::new();
    let mut h = Harness::unstarted();
    let mut provider = Provider::microsoft_template().with_client_id("test-client-id");
    provider.token_url = "https://127.0.0.1:1/token".into();
    h.oauth = vec![provider];
    h.roots = ca.roots();
    let oauth_key = h.seed_oauth_account(ME, (1, 1), "rt-1");
    let password_key = h.seed_account("pw@example.org", Some("pw"));
    h.start_core();
    let mut f = Flow {
        h,
        peers: Peers::new(&[]),
        requests: Arc::default(),
        tape: Vec::new(),
    };
    let cases = [
        (ME, None, OAuthFailure::Duplicate),
        ("ada@example.org", None, OAuthFailure::Unavailable),
        (ME, Some(password_key), OAuthFailure::NotOAuth),
        (ME, Some("nobody".to_owned()), OAuthFailure::NotOAuth),
    ];
    for (email, account, why) in cases {
        f.h.send(Command::StartOAuth {
            email: email.into(),
            account,
        });
        // Every refused start is answered, so the screen does not wait.
        assert_eq!(f.outcome(), OAuthOutcome::Failed(why), "{email}");
    }
    assert_eq!(f.count("OAuthBrowser"), 0);
    // "Sign in again" for the OAuth account itself begins.
    f.h.send(Command::StartOAuth {
        email: String::new(),
        account: Some(oauth_key),
    });
    let url = f.browser();
    assert!(url.contains("login_hint=me%40outlook.com"), "{url}");
}

// ---- The flow ----

#[test]
fn a_sign_in_adds_the_account_and_keeps_the_token_out_of_everything_else() {
    let mut f = flow(&["at-1"], Duration::ZERO, plain(ME));
    let state = f.begin(ME);
    f.redirect(&format!("code=code-xyz&state={state}"));
    let key = f.account_added();
    assert_eq!(f.outcome(), OAuthOutcome::Added);
    // The account first, then the end of the sign-in.
    let added = f.tape.iter().position(|e| e.contains("AccountAdded"));
    let done = f.tape.iter().position(|e| e.contains("OAuthDone"));
    assert!(added < done, "{:#?}", f.tape);

    let accounts = f.saved();
    assert_eq!(accounts.len(), 1);
    let settings = &accounts[0].settings;
    assert_eq!(settings.auth.as_deref(), Some("microsoft"));
    assert_eq!(
        (settings.email.as_str(), settings.username.as_str()),
        (ME, ME)
    );
    assert_eq!(settings.imap.as_ref().unwrap().host, "127.0.0.1");
    assert_eq!(refresh_of(&f.h, &key).as_deref(), Some("rt-new"));
    assert!(!password_of(&f.h, &key), "a password entry was made");
    // The first sync signs in with the token and the typed address.
    wait_until("IMAP signed in", || {
        f.peers
            .imap_tokens
            .lock()
            .unwrap()
            .contains(&"at-1".to_owned())
    });
    let request = f.requests.lock().unwrap()[0].clone();
    assert!(request.contains("code=code-xyz"), "{request}");
    let verifier = request
        .split('&')
        .find_map(|p| p.strip_prefix("code_verifier="))
        .expect("a verifier was sent")
        .to_owned();
    f.drain();
    for secret in ["code-xyz", "rt-new", "at-1", verifier.as_str()] {
        assert!(
            f.tape.iter().all(|e| !e.contains(secret)),
            "{secret} is in an event: {:#?}",
            f.tape
        );
    }
}

#[test]
fn a_link_with_the_wrong_state_is_ignored_and_the_attempt_stays() {
    let mut f = flow(&["at-1"], Duration::ZERO, plain(ME));
    let state = f.begin(ME);
    f.redirect("code=code-xyz&state=not-the-state");
    let text = f.notice();
    assert!(text.contains("no longer valid"), "{text}");
    f.drain();
    assert_eq!(f.requests.lock().unwrap().len(), 0, "a code was traded");
    assert_eq!(f.count("OAuthDone"), 0);
    // The real link still works.
    f.redirect(&format!("code=code-xyz&state={state}"));
    f.account_added();
    assert_eq!(f.outcome(), OAuthOutcome::Added);
}

#[test]
fn a_link_with_no_sign_in_waiting_is_a_notice_and_nothing_else() {
    let mut f = flow(&[], Duration::ZERO, plain(ME));
    f.redirect("code=c&state=s");
    assert!(f.notice().contains("no longer valid"));
    f.drain();
    assert_eq!(f.requests.lock().unwrap().len(), 0);
    assert!(f.saved().is_empty());
}

#[test]
fn access_denied_is_cancelled_and_ends_the_attempt() {
    let mut f = flow(&[], Duration::ZERO, plain(ME));
    let state = f.begin(ME);
    // A forged denial without the state changes nothing.
    f.redirect("error=access_denied&state=forged");
    let _ = f.notice();
    f.redirect(&format!("error=access_denied&state={state}"));
    assert_eq!(f.outcome(), OAuthOutcome::Cancelled);
    f.redirect(&format!("code=c&state={state}"));
    assert!(f.notice().contains("no longer valid"));
    assert!(f.saved().is_empty());
}

// ---- Identity (decision 9) ----

#[test]
fn the_typed_address_matching_either_claim_saves_at_once() {
    for (email, upn) in [
        (Some(ME), None),
        (Some("other@outlook.example"), Some(ME)),
        (None, Some("ME@Outlook.com")),
    ] {
        let mut f = flow(&["at-1"], Duration::ZERO, move |_, _| {
            tokens_reply_for("at-1", "rt-new", email, upn)
        });
        let state = f.begin(ME);
        f.redirect(&format!("code=c&state={state}"));
        f.account_added();
        assert_eq!(f.outcome(), OAuthOutcome::Added, "{email:?} {upn:?}");
        assert_eq!(f.count("OAuthConfirm"), 0);
    }
}

#[test]
fn a_different_address_asks_and_stores_nothing_until_accepted() {
    let mut f = flow(&["at-1"], Duration::ZERO, |_, _| {
        tokens_reply_for("at-1", "rt-new", Some("someone@outlook.example"), None)
    });
    let state = f.begin(ME);
    f.redirect(&format!("code=c&state={state}"));
    let (who, text) = f.wait(|e| match e {
        Event::OAuthConfirm {
            signed_in_as, text, ..
        } => Some((signed_in_as.clone(), text.clone())),
        _ => None,
    });
    assert_eq!(who.as_deref(), Some("someone@outlook.example"));
    assert_eq!(
        text,
        format!("You signed in as someone@outlook.example. Use this account for {ME}?")
    );
    f.drain();
    assert!(f.saved().is_empty(), "saved before the answer");
    assert_eq!(refresh_of(&f.h, "any"), None);
    assert!(!f.h.data_dir().join("oauth.json").exists());
    f.h.send(Command::ConfirmOAuth { accept: true });
    let key = f.account_added();
    assert_eq!(f.outcome(), OAuthOutcome::Added);
    let accounts = f.saved();
    assert_eq!(accounts[0].settings.email, ME);
    assert_eq!(accounts[0].settings.username, ME);
    assert_eq!(refresh_of(&f.h, &key).as_deref(), Some("rt-new"));
    // XOAUTH2 `user=` is the typed address: the fake server only accepts that.
    wait_until("IMAP signed in as the typed address", || {
        f.peers
            .imap_tokens
            .lock()
            .unwrap()
            .contains(&"at-1".to_owned())
    });
}

#[test]
fn a_token_without_identity_asks_with_the_plain_text() {
    let mut f = flow(&[], Duration::ZERO, |_, _| tokens_reply("at-1", "rt-new"));
    let state = f.begin(ME);
    f.redirect(&format!("code=c&state={state}"));
    let (who, text) = f.wait(|e| match e {
        Event::OAuthConfirm {
            signed_in_as, text, ..
        } => Some((signed_in_as.clone(), text.clone())),
        _ => None,
    });
    assert_eq!(who, None);
    assert_eq!(
        text,
        format!("You signed in to a Microsoft account. Use it for {ME}?")
    );
}

#[test]
fn declining_the_question_stores_nothing_and_drops_the_tokens() {
    let mut f = flow(&["at-1"], Duration::ZERO, |_, _| {
        tokens_reply_for("at-1", "rt-new", Some("someone@outlook.example"), None)
    });
    let state = f.begin(ME);
    f.redirect(&format!("code=c&state={state}"));
    f.wait(|e| matches!(e, Event::OAuthConfirm { .. }).then_some(()));
    f.h.send(Command::ConfirmOAuth { accept: false });
    assert_eq!(f.outcome(), OAuthOutcome::Cancelled);
    f.drain();
    assert!(f.saved().is_empty());
    assert!(!f.h.data_dir().join("oauth.json").exists());
    // The answer was the last use of the tokens: asking again has nothing to accept.
    f.h.send(Command::ConfirmOAuth { accept: true });
    assert!(f.notice().contains("no sign-in waiting"));
    f.drain();
    assert!(f.saved().is_empty());
}

#[test]
fn an_answer_with_no_attempt_is_a_notice() {
    let mut f = flow(&[], Duration::ZERO, plain(ME));
    f.h.send(Command::ConfirmOAuth { accept: true });
    assert!(f.notice().contains("no sign-in waiting"));
    f.drain();
    assert!(f.saved().is_empty());
}

// ---- Saving ----

#[test]
fn a_token_that_cannot_be_written_takes_the_account_back_out() {
    let mut f = flow(&[], Duration::ZERO, plain(ME));
    // Wait until the store is open before breaking the file.
    let _ = outbox_rows(&f.h);
    // The token file cannot be replaced: a directory sits where it goes.
    let blocker = f.h.data_dir().join("oauth.json");
    std::fs::create_dir_all(&blocker).unwrap();
    std::fs::write(blocker.join("keep"), "x").unwrap();
    let state = f.begin(ME);
    f.redirect(&format!("code=c&state={state}"));
    assert_eq!(f.outcome(), OAuthOutcome::Failed(OAuthFailure::NotSaved));
    f.drain();
    assert_eq!(f.count("AccountAdded"), 0);
    assert!(f.saved().is_empty(), "the account stayed");
    let passwords = f.h.data_dir().join("passwords.json");
    assert!(!passwords.exists(), "a password file appeared");
    assert!(blocker.join("keep").exists());
}

// ---- Failure and timing ----

#[test]
fn a_token_endpoint_that_is_down_is_offline_and_the_user_can_start_again() {
    let mut f = flow(&["at-1"], Duration::ZERO, |n, _| {
        if n == 0 {
            (503, "{}".to_owned())
        } else {
            tokens_reply_for("at-1", "rt-new", Some(ME), None)
        }
    });
    let state = f.begin(ME);
    f.redirect(&format!("code=c&state={state}"));
    assert_eq!(f.outcome(), OAuthOutcome::Failed(OAuthFailure::Offline));
    assert!(f.saved().is_empty());
    let state = f.begin(ME);
    f.redirect(&format!("code=c&state={state}"));
    f.account_added();
    assert_eq!(f.outcome(), OAuthOutcome::Added);
}

#[test]
fn a_refusal_of_the_code_is_blocked_or_other() {
    let mut f = flow(&[], Duration::ZERO, |n, _| match n {
        0 => (
            400,
            r#"{"error":"unauthorized_client","error_description":"secret words"}"#.into(),
        ),
        _ => (400, r#"{"error":"invalid_grant"}"#.into()),
    });
    for want in [OAuthFailure::Blocked, OAuthFailure::Other] {
        let state = f.begin(ME);
        f.redirect(&format!("code=c&state={state}"));
        assert_eq!(f.outcome(), OAuthOutcome::Failed(want));
    }
    f.drain();
    assert_eq!(
        f.count("secret words"),
        0,
        "Microsoft's text reached an event"
    );
}

#[test]
fn the_sign_in_stops_by_itself() {
    const WHO: &str = "timeout@outlook.com";
    set_oauth_timeout(WHO, 300);
    let mut f = flow(&[], Duration::ZERO, plain(WHO));
    let state = f.begin(WHO);
    assert_eq!(f.outcome(), OAuthOutcome::Failed(OAuthFailure::Timeout));
    f.redirect(&format!("code=c&state={state}"));
    assert!(f.notice().contains("no longer valid"));
    f.drain();
    assert_eq!(f.requests.lock().unwrap().len(), 0);
}

#[test]
fn a_new_sign_in_replaces_the_one_before_and_a_late_answer_is_dropped() {
    // Each code trade takes 600 ms; the first one's answer arrives while the
    // second sign-in waits for its own.
    let mut f = flow(&["at-a", "at-b"], Duration::from_millis(600), |n, _| {
        if n == 0 {
            tokens_reply_for("at-a", "rt-a", Some(ME), None)
        } else {
            tokens_reply_for("at-b", "rt-b", Some(ME), None)
        }
    });
    let first = f.begin(ME);
    f.redirect(&format!("code=c1&state={first}"));
    wait_until("the first trade started", || {
        !f.requests.lock().unwrap().is_empty()
    });
    let second = f.begin(ME);
    assert_ne!(first, second);
    // The first link no longer belongs to any sign-in.
    f.redirect(&format!("code=c1&state={first}"));
    assert!(f.notice().contains("no longer valid"));
    f.redirect(&format!("code=c2&state={second}"));
    let key = f.account_added();
    assert_eq!(f.outcome(), OAuthOutcome::Added);
    f.drain();
    assert_eq!(f.count("AccountAdded"), 1);
    assert_eq!(refresh_of(&f.h, &key).as_deref(), Some("rt-b"));
    assert_eq!(f.saved().len(), 1);
}

// ---- Accounts that exist ----

#[test]
fn a_password_cannot_be_set_on_a_token_account() {
    let mut r = rig(&["at-1"], Duration::ZERO, |_, _| {
        tokens_reply("at-1", "rt-2")
    });
    r.start();
    r.h.send(Command::UpdatePassword {
        account: r.key.clone(),
        password: secrecy::SecretString::from("pw".to_owned()),
    });
    let text = r.h.wait(|e| match e {
        Event::Notice { text } => Some(text.clone()),
        _ => None,
    });
    assert!(text.contains("no password"), "{text}");
    settle();
    assert!(!password_of(&r.h, &r.key));
    assert!(!r.h.data_dir().join("passwords.json").exists());
}

#[test]
fn signing_in_again_resumes_the_account_and_the_outbox_drains() {
    let mut r = rig(&["at-2"], Duration::ZERO, |_, form| {
        if form.contains("grant_type=authorization_code") {
            tokens_reply_for("at-2", "rt-3", Some(ME), None)
        } else if refresh_token_of(form).as_deref() == Some("rt-1") {
            support::bearer::revoked_reply()
        } else {
            tokens_reply("at-2", "rt-3")
        }
    });
    queue(&r.h, &r.key);
    r.start();
    let (_, text) =
        r.h.wait(|e| status_of(e, &r.key).filter(|(s, _)| *s == AccountStatus::SignInFailed));
    assert_eq!(text, "Sign in again");
    assert_eq!(*r.peers.sent.lock().unwrap(), 0);

    r.h.send(Command::StartOAuth {
        email: String::new(),
        account: Some(r.key.clone()),
    });
    let url = r.h.wait(|e| match e {
        Event::OAuthBrowser { url, .. } => Some(url.clone()),
        _ => None,
    });
    r.h.send(Command::OAuthRedirect {
        url: secrecy::SecretString::from(format!(
            "enviooauth://callback?code=c&state={}",
            state_of(&url)
        )),
    });
    let outcome = r.h.wait(|e| match e {
        Event::OAuthDone { outcome, .. } => Some(*outcome),
        _ => None,
    });
    assert_eq!(outcome, OAuthOutcome::Resumed);
    wait_until("the held message went out", || {
        *r.peers.sent.lock().unwrap() == 1
    });
    assert_eq!(refresh_of(&r.h, &r.key).as_deref(), Some("rt-3"));
    let rows = outbox_rows(&r.h);
    assert!(
        rows.iter()
            .all(|row| !matches!(row.status, OutboxStatus::Failed { .. })),
        "{rows:?}"
    );
    assert_eq!(
        r.h.events
            .try_iter()
            .filter(|e| matches!(e, Event::AccountAdded { .. }))
            .count(),
        0
    );
}

#[test]
fn removing_the_account_deletes_its_token() {
    let mut f = flow(&["at-1"], Duration::ZERO, plain(ME));
    let state = f.begin(ME);
    f.redirect(&format!("code=c&state={state}"));
    let key = f.account_added();
    assert_eq!(refresh_of(&f.h, &key).as_deref(), Some("rt-new"));
    f.h.send(Command::RemoveAccount {
        account: key.clone(),
        confirmed: true,
    });
    f.wait(|e| matches!(e, Event::AccountRemoved { .. }).then_some(()));
    assert_eq!(refresh_of(&f.h, &key), None);
}

// ---- Review 1218 ----

#[test]
fn the_redirect_command_does_not_print_its_code() {
    let command = Command::OAuthRedirect {
        url: secrecy::SecretString::from("enviooauth://callback?code=code-dbg&state=s".to_owned()),
    };
    let shown = format!("{command:?}");
    assert!(!shown.contains("code-dbg"), "{shown}");
    assert!(!shown.contains("callback"), "{shown}");
}

#[test]
fn an_account_that_could_not_be_taken_back_out_does_not_block_a_retry() {
    const WHO: &str = "stranded@outlook.com";
    let mut f = flow(&["at-1"], Duration::ZERO, plain(WHO));
    let _ = outbox_rows(&f.h);
    // The token cannot be written and the cleanup fails as well.
    let blocker = f.h.data_dir().join("oauth.json");
    std::fs::create_dir_all(&blocker).unwrap();
    envio_core::hooks::fail_next_account_removal(WHO);
    let state = f.begin(WHO);
    f.redirect(&format!("code=c&state={state}"));
    assert_eq!(f.outcome(), OAuthOutcome::Failed(OAuthFailure::NotSaved));
    assert_eq!(f.saved().len(), 1, "the entry is on disk without a token");
    assert_eq!(f.count("AccountAdded"), 0);
    // The disk is fine again: the same address can sign in, with no
    // "already added" and no second entry.
    std::fs::remove_dir_all(&blocker).unwrap();
    let state = f.begin(WHO);
    f.redirect(&format!("code=c&state={state}"));
    let key = f.account_added();
    assert_eq!(f.outcome(), OAuthOutcome::Added);
    let accounts = f.saved();
    assert_eq!(accounts.len(), 1, "{accounts:?}");
    assert_eq!(accounts[0].key, key);
    assert_eq!(refresh_of(&f.h, &key).as_deref(), Some("rt-new"));
}

#[test]
fn the_sign_in_times_out_while_the_code_is_being_traded() {
    const WHO: &str = "slow-trade@outlook.com";
    set_oauth_timeout(WHO, 300);
    let mut f = flow(&["at-1"], Duration::from_millis(900), plain(WHO));
    let state = f.begin(WHO);
    f.redirect(&format!("code=c&state={state}"));
    assert_eq!(f.outcome(), OAuthOutcome::Failed(OAuthFailure::Timeout));
    // The trade finishes afterwards; its answer is dropped.
    std::thread::sleep(Duration::from_millis(1200));
    f.drain();
    assert_eq!(f.count("AccountAdded"), 0);
    assert!(f.saved().is_empty());
    assert_eq!(f.count("OAuthDone"), 1);
}

#[test]
fn the_sign_in_times_out_while_the_question_is_open() {
    const WHO: &str = "slow-answer@outlook.com";
    set_oauth_timeout(WHO, 600);
    let mut f = flow(&["at-1"], Duration::ZERO, |_, _| {
        tokens_reply_for("at-1", "rt-new", Some("someone@outlook.example"), None)
    });
    let state = f.begin(WHO);
    f.redirect(&format!("code=c&state={state}"));
    f.wait(|e| matches!(e, Event::OAuthConfirm { .. }).then_some(()));
    assert_eq!(f.outcome(), OAuthOutcome::Failed(OAuthFailure::Timeout));
    // A late "yes" has nothing to accept, and nothing was stored.
    f.h.send(Command::ConfirmOAuth { accept: true });
    assert!(f.notice().contains("no sign-in waiting"));
    f.drain();
    assert!(f.saved().is_empty());
    assert!(!f.h.data_dir().join("oauth.json").exists());
}

#[test]
fn a_token_write_failure_on_sign_in_again_keeps_the_account_paused() {
    let mut r = rig(&["at-2"], Duration::ZERO, |_, form| {
        if form.contains("grant_type=authorization_code") {
            tokens_reply_for("at-2", "rt-3", Some(ME), None)
        } else if refresh_token_of(form).as_deref() == Some("rt-1") {
            support::bearer::revoked_reply()
        } else {
            tokens_reply("at-2", "rt-3")
        }
    });
    queue(&r.h, &r.key);
    r.start();
    r.h.wait(|e| status_of(e, &r.key).filter(|(s, _)| *s == AccountStatus::SignInFailed));
    // The token file cannot be replaced: a directory sits where it is.
    let file = r.h.data_dir().join("oauth.json");
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir_all(&file).unwrap();
    let (outcome, mut events) = sign_in_again(&r);
    assert_eq!(outcome, OAuthOutcome::Failed(OAuthFailure::NotSaved));
    settle();
    events.extend(r.h.events.try_iter());
    assert!(
        events
            .iter()
            .filter_map(|e| status_of(e, &r.key))
            .all(|(s, _)| s == AccountStatus::SignInFailed),
        "the account was resumed: {events:?}"
    );
    assert_eq!(*r.peers.sent.lock().unwrap(), 0);
    // Once the file can be written again, signing in again works.
    std::fs::remove_dir_all(&file).unwrap();
    assert_eq!(sign_in_again(&r).0, OAuthOutcome::Resumed);
    wait_until("the held message went out", || {
        *r.peers.sent.lock().unwrap() == 1
    });
}

/// Signs in again and returns the outcome with the events seen on the way.
fn sign_in_again(r: &Rig) -> (OAuthOutcome, Vec<Event>) {
    r.h.send(Command::StartOAuth {
        email: String::new(),
        account: Some(r.key.clone()),
    });
    let url = r.h.wait(|e| match e {
        Event::OAuthBrowser { url, .. } => Some(url.clone()),
        _ => None,
    });
    r.h.send(Command::OAuthRedirect {
        url: secrecy::SecretString::from(format!(
            "enviooauth://callback?code=c&state={}",
            state_of(&url)
        )),
    });
    let seen = std::cell::RefCell::new(Vec::new());
    let outcome = r.h.wait(|e| {
        seen.borrow_mut().push(e.clone());
        match e {
            Event::OAuthDone { outcome, .. } => Some(*outcome),
            _ => None,
        }
    });
    (outcome, seen.into_inner())
}
