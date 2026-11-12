//! Account setup runs on its own thread: the worker keeps answering.

mod support;

use std::time::{Duration, Instant};

use envio_core::hooks::{SetupFake, fake_setup, setup_runs};
use envio_core::{AccountSetup, Command, ConnectRequest, Event, ServerAddr};
use envio_net::autoconfig::{Found, Setting, Source};
use envio_net::{Endpoint, Security};
use secrecy::SecretString;
use support::Harness;

fn setting(host: &str) -> Setting {
    Setting {
        endpoint: Endpoint {
            host: host.into(),
            port: 993,
            security: Security::Tls,
            pin: None,
        },
        username: "me".into(),
    }
}

fn found(confirm: bool) -> Found {
    Found {
        imap: setting("imap.example.org"),
        smtp: setting("smtp.example.org"),
        source: if confirm { Source::Srv } else { Source::Ispdb },
        confirm,
    }
}

fn slow(email: &str, confirm: bool) {
    fake_setup(
        email,
        SetupFake {
            delay: Duration::from_secs(2),
            found: Some(found(confirm)),
        },
    );
}

fn folders_answer(h: &Harness, key: &str) -> Duration {
    let began = Instant::now();
    h.send(Command::ListFolders {
        account: key.into(),
    });
    h.wait(|e| matches!(e, Event::Folders { .. }).then_some(()));
    began.elapsed()
}

fn started(h: &Harness) {
    h.wait(|e| matches!(e, Event::Started { .. }).then_some(()));
}

#[test]
fn commands_are_handled_while_detect_runs() {
    let mut h = Harness::unstarted();
    let key = h.seed_account("seed@example.org", Some("pw"));
    h.start_core();
    started(&h);
    slow("slow-detect@example.org", false);
    h.send(Command::Detect {
        email: "slow-detect@example.org".into(),
    });
    std::thread::sleep(Duration::from_millis(100));
    let took = folders_answer(&h, &key);
    // Detect stalls for 2 s; the answer comes well before it ends.
    assert!(took < Duration::from_millis(1_500), "{took:?}");
    // The Detect still ends.
    h.wait(|e| matches!(e, Event::Detected { .. }).then_some(()));
}

#[test]
fn commands_are_handled_while_connect_probes() {
    let mut h = Harness::unstarted();
    let key = h.seed_account("seed@example.org", Some("pw"));
    h.start_core();
    started(&h);
    // A server that accepts the connection and never answers.
    let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = silent.local_addr().unwrap().port();
    let addr = ServerAddr {
        host: "127.0.0.1".into(),
        port,
        security: Security::Tls,
    };
    h.send(Command::Connect(Box::new(ConnectRequest {
        setup: AccountSetup {
            name: "Slow".into(),
            email: "slow-probe@example.org".into(),
            username: "slow".into(),
            imap: addr.clone(),
            smtp: addr,
            source: None,
            needs_confirm: false,
        },
        password: SecretString::from("pw".to_owned()),
        servers_confirmed: false,
        accepted: vec![],
    })));
    std::thread::sleep(Duration::from_millis(200));
    let took = folders_answer(&h, &key);
    // The probe never gets an answer; the folders answer must not wait on it.
    assert!(took < Duration::from_millis(1_500), "{took:?}");
}

#[test]
fn a_second_detect_replaces_the_queued_one() {
    let h = Harness::start();
    started(&h);
    let email = "replace@example.org";
    fake_setup(
        email,
        SetupFake {
            delay: Duration::from_millis(800),
            found: Some(found(false)),
        },
    );
    for _ in 0..3 {
        h.send(Command::Detect {
            email: email.into(),
        });
        std::thread::sleep(Duration::from_millis(50));
    }
    // The first runs (its answer is stale); the second was replaced by the
    // third, which answers.
    h.wait(|e| matches!(e, Event::Detected { .. }).then_some(()));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(setup_runs(email), 2);
}

#[test]
fn setup_results_keep_the_srv_confirm_rule() {
    let h = Harness::start();
    started(&h);
    let email = "srv@example.org";
    fake_setup(
        email,
        SetupFake {
            delay: Duration::ZERO,
            found: Some(found(true)),
        },
    );
    h.send(Command::Detect {
        email: email.into(),
    });
    let (setup, text) = h.wait(|e| match e {
        Event::Detected { setup, text, .. } => Some((setup.clone(), text.clone())),
        _ => None,
    });
    assert!(setup.needs_confirm);
    assert!(text.contains("imap.example.org"), "{text}");
    // An app that clears the flag still gets refused.
    h.send(Command::Connect(Box::new(ConnectRequest {
        setup: AccountSetup {
            needs_confirm: false,
            ..setup
        },
        password: SecretString::from("pw".to_owned()),
        servers_confirmed: false,
        accepted: vec![],
    })));
    let text = h.wait(|e| match e {
        Event::SetupProblem { text, .. } => Some(text.clone()),
        _ => None,
    });
    assert_eq!(text, "Confirm the servers before signing in");
}
