//! #384: a Detect whose MX records say Microsoft 365 ends in a card, not in
//! `Detected`; with sign-in off it ends in the refusal.

mod support;

use std::time::{Duration, Instant};

use envio_core::hooks::{SetupFake, fake_setup, setup_runs};
use envio_core::{AccountSetup, Command, ConnectRequest, Event, ServerAddr};
use envio_net::autoconfig::{Found, Hosting, Setting, Source};
use envio_net::oauth::Provider;
use envio_net::{Endpoint, Security};
use secrecy::SecretString;
use support::Harness;

fn found() -> Found {
    let setting = |host: &str| Setting {
        endpoint: Endpoint::new(host, 993, Security::Tls),
        username: "me".into(),
    };
    Found {
        imap: setting("imap.found.example"),
        smtp: setting("smtp.found.example"),
        source: Source::Ispdb,
        confirm: false,
    }
}

fn arm(email: &str, hosting: Hosting) {
    fake_setup(
        email,
        SetupFake {
            found: Some(found()),
            hosting: Some(hosting),
            ..Default::default()
        },
    );
}

fn with_microsoft() -> Harness {
    let mut h = Harness::unstarted();
    h.oauth = vec![Provider::microsoft_template().with_client_id("test-client-id")];
    h.start_core();
    h
}

/// Events for the next `span`, so a wrong event has time to show.
fn drain_for(h: &Harness, span: Duration) -> Vec<Event> {
    let mut seen = Vec::new();
    let until = Instant::now() + span;
    while let Some(left) = until.checked_duration_since(Instant::now()) {
        if let Ok(event) = h.events.recv_timeout(left) {
            seen.push(event);
        }
    }
    seen
}

fn drain(h: &Harness) -> Vec<Event> {
    drain_for(h, Duration::from_millis(600))
}

fn detect(h: &Harness, email: &str) {
    h.send(Command::Detect {
        email: email.into(),
    });
}

#[test]
fn a_microsoft_verdict_gives_the_card_and_no_detected() {
    let h = with_microsoft();
    let email = "Ann@Contoso.Example";
    arm(email, Hosting::Microsoft365);
    detect(&h, email);
    let (who, title, text) = h.wait(|e| match e {
        Event::MailOnMicrosoft { email, title, text } => {
            Some((email.clone(), title.clone(), text.clone()))
        }
        _ => None,
    });
    assert_eq!(who, email);
    assert_eq!(title, "This mail is on Microsoft");
    assert_eq!(
        text,
        "Mail for contoso.example is handled by Microsoft. Sign in with Microsoft to add it."
    );
    let rest = drain(&h);
    assert!(
        !rest.iter().any(|e| matches!(
            e,
            Event::Detected { .. } | Event::SetupRefused { .. } | Event::SetupProblem { .. }
        )),
        "{rest:?}"
    );
}

#[test]
fn a_microsoft_verdict_with_sign_in_off_is_refused_with_the_domain() {
    let h = Harness::start();
    let email = "ann@refused-mx.example";
    arm(email, Hosting::Microsoft365);
    detect(&h, email);
    let (who, title, text) = h.wait(|e| match e {
        Event::SetupRefused { email, title, text } => {
            Some((email.clone(), title.clone(), text.clone()))
        }
        _ => None,
    });
    assert_eq!(who, email);
    assert_eq!(title, "Microsoft accounts aren\u{2019}t supported yet");
    assert_eq!(
        text,
        "Mail for refused-mx.example is hosted by Microsoft 365, which Envio can\u{2019}t connect to for now. Try another address."
    );
    let rest = drain(&h);
    assert!(
        !rest
            .iter()
            .any(|e| matches!(e, Event::Detected { .. } | Event::MailOnMicrosoft { .. })),
        "{rest:?}"
    );
}

#[test]
fn an_unknown_verdict_is_detected_as_before() {
    let h = with_microsoft();
    let email = "ann@unknown-mx.example";
    arm(email, Hosting::Unknown);
    detect(&h, email);
    let setup = h.wait(|e| match e {
        Event::Detected { setup, .. } => Some(setup.clone()),
        _ => None,
    });
    assert_eq!(setup.imap.host, "imap.found.example");
    let rest = drain(&h);
    assert!(
        !rest.iter().any(|e| matches!(
            e,
            Event::MailOnMicrosoft { .. } | Event::SetupRefused { .. }
        )),
        "{rest:?}"
    );
}

#[test]
fn a_stale_microsoft_verdict_is_dropped() {
    let h = with_microsoft();
    let email = "ann@stale-mx.example";
    fake_setup(
        email,
        SetupFake {
            delay: Duration::from_millis(500),
            found: Some(found()),
            hosting: Some(Hosting::Microsoft365),
        },
    );
    for _ in 0..3 {
        detect(&h, email);
        std::thread::sleep(Duration::from_millis(50));
    }
    let seen = drain_for(&h, Duration::from_millis(2_000));
    // The first run's answer is stale; the replaced second never runs; the
    // third answers: one card, not two.
    let cards = seen
        .iter()
        .filter(|e| matches!(e, Event::MailOnMicrosoft { .. }))
        .count();
    assert_eq!(cards, 1, "{seen:?}");
    assert_eq!(setup_runs(email), 2);
}

#[test]
fn a_fake_with_only_a_verdict_looks_nothing_up_and_guesses() {
    let h = with_microsoft();
    let email = "ann@only-hosting.invalid";
    fake_setup(
        email,
        SetupFake {
            hosting: Some(Hosting::Unknown),
            ..Default::default()
        },
    );
    let started = Instant::now();
    detect(&h, email);
    let setup = h.wait(|e| match e {
        Event::Detected { setup, .. } => Some(setup.clone()),
        _ => None,
    });
    assert_eq!(setup.source, Some(Source::Guess));
    assert_eq!(setup.imap.host, "imap.only-hosting.invalid");
    assert!(started.elapsed() < Duration::from_secs(2));
}

/// Connect does not refuse a custom domain: only the address decides there.
#[test]
fn connecting_an_mx_hosted_domain_by_hand_is_not_refused() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let addr = ServerAddr {
        host: "127.0.0.1".into(),
        port,
        security: Security::Tls,
    };
    let h = Harness::start();
    let email = "ann@manual-mx.example";
    arm(email, Hosting::Microsoft365);
    h.send(Command::Connect(Box::new(ConnectRequest {
        setup: AccountSetup {
            name: "Ann".into(),
            email: email.into(),
            username: email.into(),
            imap: addr.clone(),
            smtp: addr,
            source: None,
            needs_confirm: false,
        },
        password: SecretString::from("hunter2".to_owned()),
        servers_confirmed: true,
        accepted: vec![],
    })));
    let rest = drain(&h);
    assert!(
        !rest.iter().any(|e| matches!(
            e,
            Event::SetupRefused { .. } | Event::MailOnMicrosoft { .. }
        )),
        "{rest:?}"
    );
    assert!(listener.accept().is_ok(), "the core never connected");
}
