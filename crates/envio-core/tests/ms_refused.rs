//! #381: a Microsoft address is refused before anything is looked up or
//! offered, while Microsoft sign-in is off (until #382).

mod support;

use std::time::{Duration, Instant};

use envio_core::hooks::{SetupFake, fake_setup, setup_runs};
use envio_core::{Command, Event};
use envio_net::autoconfig::{Found, Setting, Source};
use envio_net::{Endpoint, Security};
use support::Harness;

const MICROSOFT: &[&str] = &[
    "ms-refused-1@outlook.com",
    "ms-refused-2@hotmail.co.uk",
    "ms-refused-3@live.de",
    "ms-refused-4@msn.com",
    "ms-refused-5@contoso.onmicrosoft.com",
    "ms-refused-6@OUTLOOK.com",
];

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

fn arm(email: &str) {
    fake_setup(
        email,
        SetupFake {
            delay: Duration::ZERO,
            found: Some(found()),
        },
    );
}

/// Events for the next 600 ms, so a wrong `Detected` has time to show.
fn drain(h: &Harness) -> Vec<Event> {
    let mut seen = Vec::new();
    let until = Instant::now() + Duration::from_millis(600);
    while let Some(left) = until.checked_duration_since(Instant::now()) {
        if let Ok(event) = h.events.recv_timeout(left) {
            seen.push(event);
        }
    }
    seen
}

#[test]
fn a_microsoft_address_is_refused_and_nothing_is_looked_up() {
    let h = Harness::start();
    for email in MICROSOFT {
        arm(email);
        h.send(Command::Detect {
            email: (*email).into(),
        });
        let (who, title, text) = h.wait(|e| match e {
            Event::SetupRefused { email, title, text } => {
                Some((email.clone(), title.clone(), text.clone()))
            }
            _ => None,
        });
        assert_eq!(who, *email);
        assert_eq!(title, "Microsoft accounts aren\u{2019}t supported yet");
        assert_eq!(
            text,
            "Envio can\u{2019}t connect to Outlook, Hotmail or Microsoft 365 mail for now. Try another address."
        );
        assert_eq!(setup_runs(email), 0, "{email} was looked up");
    }
    let rest = drain(&h);
    assert!(
        !rest
            .iter()
            .any(|e| matches!(e, Event::Detected { .. } | Event::SetupProblem { .. })),
        "{rest:?}"
    );
}

#[test]
fn a_microsoft_address_gets_no_sign_in_offer() {
    let h = Harness::start();
    let email = "ms-refused-7@live.com";
    arm(email);
    h.send(Command::Detect {
        email: email.into(),
    });
    let rest = drain(&h);
    assert!(rest.iter().any(|e| matches!(e, Event::SetupRefused { .. })));
    assert!(
        !rest
            .iter()
            .any(|e| matches!(e, Event::Detected { oauth: Some(_), .. })),
        "{rest:?}"
    );
}

#[test]
fn another_address_is_detected_as_before() {
    let h = Harness::start();
    let email = "ms-refused-ok@example.org";
    arm(email);
    h.send(Command::Detect {
        email: email.into(),
    });
    let (setup, oauth) = h.wait(|e| match e {
        Event::Detected { setup, oauth, .. } => Some((setup.clone(), oauth.clone())),
        _ => None,
    });
    assert_eq!(oauth, None);
    assert_eq!(setup.imap.host, "imap.found.example");
    assert_eq!(setup_runs(email), 1);
    let rest = drain(&h);
    assert!(
        !rest.iter().any(|e| matches!(e, Event::SetupRefused { .. })),
        "{rest:?}"
    );
}

#[test]
fn look_alike_domains_are_not_refused() {
    let h = Harness::start();
    // Not Microsoft's: only the explicit table counts (live.org, hotmail.biz).
    let email = "ms-refused-alike@live.org";
    arm(email);
    h.send(Command::Detect {
        email: email.into(),
    });
    h.wait(|e| matches!(e, Event::Detected { .. }).then_some(()));
    assert_eq!(setup_runs(email), 1);
}
