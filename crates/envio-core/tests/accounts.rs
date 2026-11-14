//! Adding accounts (servers, trust, password) and removing them.

mod support;

use std::net::TcpListener;
use std::time::{Duration, Instant};

use envio_core::secrets::Secrets;
use envio_core::{
    AccountSetup, AccountStatus, Command, ConnectRequest, Event, ServerAddr, Service,
};
use envio_net::Security;
use envio_store::{Accounts, ActionKind, Draft, Journal, MessageRef, NewOutboxItem, Outbox};
use secrecy::{ExposeSecret, SecretString};
use support::Harness;
use support::fake::{Kind, serve};

const EMAIL: &str = "ada@example.org";

fn addr(port: u16) -> ServerAddr {
    ServerAddr {
        host: "127.0.0.1".into(),
        port,
        security: Security::Tls,
    }
}

fn request(imap: u16, smtp: u16, accepted: Vec<(Service, String)>) -> Command {
    Command::Connect(Box::new(ConnectRequest {
        setup: AccountSetup {
            name: "Ada".into(),
            email: EMAIL.into(),
            username: EMAIL.into(),
            imap: addr(imap),
            smtp: addr(smtp),
            source: None,
            needs_confirm: false,
        },
        password: SecretString::from("hunter2".to_owned()),
        servers_confirmed: false,
        accepted,
    }))
}

fn problem(event: &Event) -> Option<String> {
    match event {
        Event::SetupProblem { text, .. } => Some(text.clone()),
        _ => None,
    }
}

fn saved_accounts(h: &Harness) -> Vec<envio_store::Account> {
    Accounts::open(&h.paths().accounts())
        .unwrap()
        .list()
        .to_vec()
}

fn started(h: &Harness) {
    h.wait(|e| matches!(e, Event::Started { .. }).then_some(()));
}

fn status_of(account: &str) -> impl Fn(&Event) -> Option<AccountStatus> + '_ {
    move |event| match event {
        Event::AccountStatus {
            account: a, status, ..
        } if a == account => Some(status.clone()),
        _ => None,
    }
}

fn queue_mail(h: &Harness, account: &str) {
    let outbox = Outbox::open(&h.paths().outbox()).unwrap();
    let new = NewOutboxItem {
        account: account.into(),
        message_id: "<m1@example.org>".into(),
        queued_at_ms: 1_000,
        from: EMAIL.into(),
        recipients: vec!["bob@example.org".into()],
    };
    outbox
        .enqueue(new, b"Subject: hi\r\n\r\nhello\r\n")
        .unwrap();
}

#[test]
fn connect_refuses_unconfirmed_srv_servers_before_any_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let h = Harness::start();
    let Command::Connect(mut connect) = request(port, port, vec![]) else {
        unreachable!()
    };
    connect.setup.needs_confirm = true;
    h.send(Command::Connect(connect));
    let text = h.wait(problem);
    assert_eq!(text, "Confirm the servers before signing in");
    std::thread::sleep(Duration::from_millis(300));
    assert!(listener.accept().is_err(), "the core connected anyway");
    assert!(saved_accounts(&h).is_empty());
}

#[test]
fn connect_reports_an_unreachable_server() {
    let h = Harness::start();
    h.send(request(1, 1, vec![]));
    let text = h.wait(problem);
    assert!(text.starts_with("Could not reach the server"), "{text}");
    assert!(saved_accounts(&h).is_empty());
    assert!(Secrets::open(&h.data_dir()).unwrap().accounts().is_empty());
}

#[test]
fn connect_asks_before_trusting_an_unknown_certificate() {
    let imap = serve(Kind::Handshake);
    let h = Harness::start();
    h.send(request(imap.port, 1, vec![]));
    let (service, host, hex, text) = h.wait(|e| match e {
        Event::TrustPrompt {
            service,
            host,
            hex,
            fingerprint,
            text,
            ..
        } => {
            assert_eq!(fingerprint.len(), 95, "colon separated pairs");
            Some((*service, host.clone(), hex.clone(), text.clone()))
        }
        _ => None,
    });
    assert_eq!(service, Service::Imap);
    assert_eq!(host, "127.0.0.1");
    assert_eq!(hex, imap.hex);
    assert!(text.contains("not trusted"), "{text}");
    assert!(text.contains("incoming mail server"), "{text}");
    assert!(saved_accounts(&h).is_empty());
}

#[test]
fn connect_asks_again_for_the_smtp_certificate_and_saves_nothing_yet() {
    let imap = serve(Kind::Imap);
    let smtp = serve(Kind::Handshake);
    let h = Harness::start();
    h.send(request(
        imap.port,
        smtp.port,
        vec![(Service::Imap, imap.hex.clone())],
    ));
    let (service, hex) = h.wait(|e| match e {
        Event::TrustPrompt { service, hex, .. } => Some((*service, hex.clone())),
        _ => None,
    });
    assert_eq!((service, hex), (Service::Smtp, smtp.hex));
    assert!(saved_accounts(&h).is_empty());
    assert!(Secrets::open(&h.data_dir()).unwrap().accounts().is_empty());
}

#[test]
fn connect_adds_the_account_with_only_the_accepted_pins() {
    let imap = serve(Kind::Imap);
    let smtp = serve(Kind::Smtp);
    let h = Harness::start();
    let accepted = vec![
        (Service::Imap, imap.hex.clone()),
        (Service::Smtp, smtp.hex.clone()),
    ];
    h.send(request(imap.port, smtp.port, accepted));
    let info = h.wait(|e| match e {
        Event::AccountAdded { account } => Some(account.clone()),
        _ => None,
    });
    assert_eq!((info.email.as_str(), info.name.as_str()), (EMAIL, "Ada"));
    let saved = saved_accounts(&h);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].key, info.key);
    assert_eq!(
        saved[0].settings.imap.pin.as_deref(),
        Some(imap.hex.as_str())
    );
    assert_eq!(
        saved[0].settings.smtp.pin.as_deref(),
        Some(smtp.hex.as_str())
    );
    let secrets = Secrets::open(&h.data_dir()).unwrap();
    assert!(secrets.get(&info.key).is_some());
    // A second add of the same address is refused.
    h.send(request(imap.port, smtp.port, vec![]));
    let text = h.wait(problem);
    assert_eq!(text, "This account is already added");
}

#[test]
fn remove_asks_first_when_work_is_unsent() {
    let mut h = Harness::unstarted();
    let key = h.seed_account(EMAIL, Some("pw"));
    queue_mail(&h, &key);
    h.start_core();
    started(&h);
    h.send(Command::RemoveAccount {
        account: key.clone(),
        confirmed: false,
    });
    let (account, unsent, text) = h.wait(|e| match e {
        Event::ConfirmRemove {
            account,
            unsent,
            text,
        } => Some((account.clone(), *unsent, text.clone())),
        _ => None,
    });
    assert_eq!(account, key);
    assert_eq!((unsent.outbox, unsent.drafts, unsent.actions), (1, 0, 0));
    assert!(text.contains("1 message in the Outbox"), "{text}");
    assert_eq!(saved_accounts(&h).len(), 1, "nothing is removed yet");
}

#[test]
fn removing_an_account_clears_the_inline_images() {
    let mut h = Harness::unstarted();
    let key = h.seed_account(EMAIL, Some("pw"));
    h.start_core();
    started(&h);
    let folder = h.paths().inline().join("3");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("0.png"), b"x").unwrap();
    h.send(Command::RemoveAccount {
        account: key.clone(),
        confirmed: true,
    });
    h.wait(|e| matches!(e, Event::AccountRemoved { account } if *account == key).then_some(()));
    assert!(!h.paths().inline().exists());
}

#[test]
fn remove_confirmed_deletes_outbox_drafts_journal_and_password() {
    let mut h = Harness::unstarted();
    let key = h.seed_account(EMAIL, Some("pw"));
    let other = h.seed_account("bob@example.org", Some("pw2"));
    queue_mail(&h, &key);
    let drafts = envio_store::Drafts::open(&h.paths().drafts()).unwrap();
    for account in [&key, &other] {
        let draft = Draft {
            account: account.clone(),
            subject: "d".into(),
            ..Draft::default()
        };
        drafts.create(&draft).unwrap();
    }
    let mut journal = Journal::open(&h.paths().journal()).unwrap();
    let target = |account: &str| MessageRef {
        account: account.into(),
        folder: "INBOX".into(),
        uidvalidity: 1,
        uid: 7,
        message_id: None,
    };
    journal.push(1, target(&key), ActionKind::Delete).unwrap();
    journal.push(2, target(&other), ActionKind::Delete).unwrap();
    drop(journal);
    h.start_core();
    started(&h);
    h.send(Command::RemoveAccount {
        account: key.clone(),
        confirmed: true,
    });
    h.wait(|e| (status_of(&key)(e) == Some(AccountStatus::Removing)).then_some(()));
    h.wait(|e| matches!(e, Event::AccountRemoved { account } if *account == key).then_some(()));
    drop(h.core.take());

    let paths = h.paths();
    let left: Vec<String> = saved_accounts(&h).into_iter().map(|a| a.key).collect();
    assert_eq!(left, vec![other.clone()]);
    assert!(
        Outbox::open(&paths.outbox())
            .unwrap()
            .items()
            .unwrap()
            .is_empty()
    );
    let drafts = envio_store::Drafts::open(&paths.drafts()).unwrap();
    let kept: Vec<String> = drafts
        .list()
        .unwrap()
        .into_iter()
        .map(|d| d.1.account)
        .collect();
    assert_eq!(kept, vec![other.clone()]);
    let journal = Journal::open(&paths.journal()).unwrap();
    let pending: Vec<String> = journal
        .pending()
        .map(|a| a.target.account.clone())
        .collect();
    assert_eq!(pending, vec![other.clone()]);
    let secrets = Secrets::open(&h.data_dir()).unwrap();
    assert!(secrets.get(&key).is_none());
    assert!(secrets.get(&other).is_some());
    let marker = std::fs::read_to_string(h.data_dir().join("removing.json")).unwrap_or_default();
    assert!(!marker.contains(&key), "{marker}");
}

#[test]
fn remove_without_unsent_work_needs_no_confirmation() {
    let mut h = Harness::unstarted();
    let key = h.seed_account(EMAIL, Some("pw"));
    h.start_core();
    started(&h);
    h.send(Command::RemoveAccount {
        account: key.clone(),
        confirmed: false,
    });
    let removed = h.wait(|e| match e {
        Event::AccountRemoved { account } => Some(*account == key),
        Event::ConfirmRemove { .. } => Some(false),
        _ => None,
    });
    assert!(removed, "asked although nothing was unsent");
    assert!(saved_accounts(&h).is_empty());
}

#[test]
fn an_interrupted_removal_is_finished_at_start() {
    let mut h = Harness::unstarted();
    let key = h.seed_account(EMAIL, Some("pw"));
    queue_mail(&h, &key);
    // The cut came after the marker was written and before anything else.
    std::fs::write(h.data_dir().join("removing.json"), format!("[\"{key}\"]")).unwrap();
    h.start_core();
    let accounts = h.wait(|e| match e {
        Event::Started { accounts, .. } => Some(accounts.clone()),
        _ => None,
    });
    assert!(accounts.is_empty(), "{accounts:?}");
    drop(h.core.take());
    assert!(saved_accounts(&h).is_empty());
    let outbox = Outbox::open(&h.paths().outbox()).unwrap();
    assert!(outbox.items().unwrap().is_empty());
    assert!(Secrets::open(&h.data_dir()).unwrap().get(&key).is_none());
}

#[test]
fn a_damaged_removal_marker_is_kept_aside_and_the_store_opens() {
    let mut h = Harness::unstarted();
    let key = h.seed_account(EMAIL, Some("pw"));
    std::fs::write(h.data_dir().join("removing.json"), "not json {").unwrap();
    h.start_core();
    let accounts = h.wait(|e| match e {
        Event::Started { accounts, .. } => Some(accounts.clone()),
        _ => None,
    });
    assert_eq!(accounts.len(), 1, "{accounts:?}");
    drop(h.core.take());
    let aside = h.data_dir().join("removing.json.damaged");
    assert_eq!(std::fs::read_to_string(aside).unwrap(), "not json {");
    assert!(!h.data_dir().join("removing.json").exists());
    assert_eq!(saved_accounts(&h)[0].key, key);
}

#[test]
fn a_failed_resume_does_not_stop_the_store_opening() {
    let mut h = Harness::unstarted();
    h.seed_account(EMAIL, Some("pw"));
    // Reading it fails (a directory), which is not a damaged file.
    let marker = h.data_dir().join("removing.json");
    std::fs::create_dir(&marker).unwrap();
    h.start_core();
    let accounts = h.wait(|e| match e {
        Event::Started { accounts, .. } => Some(accounts.clone()),
        _ => None,
    });
    assert_eq!(accounts.len(), 1, "{accounts:?}");
    drop(h.core.take());
    assert!(marker.is_dir(), "the marker was not kept");
}

#[test]
fn update_password_resumes_a_paused_account() {
    let mut h = Harness::unstarted();
    h.foreground = true;
    let key = h.seed_account(EMAIL, None);
    h.start_core();
    h.wait(|e| (status_of(&key)(e) == Some(AccountStatus::SignInFailed)).then_some(()));
    h.send(Command::UpdatePassword {
        account: key.clone(),
        password: SecretString::from("new-pw".to_owned()),
    });
    // Resumed: it tries the (unreachable) server and goes offline.
    h.wait(|e| matches!(status_of(&key)(e), Some(AccountStatus::Offline { .. })).then_some(()));
    let secrets = Secrets::open(&h.data_dir()).unwrap();
    assert!(secrets.get(&key).is_some());
}

#[test]
fn accept_certificate_needs_the_presented_fingerprint() {
    let server = serve(Kind::Handshake);
    let mut h = Harness::unstarted();
    h.foreground = true;
    let key = h.seed_account_at(EMAIL, Some("pw"), server.port);
    h.start_core();
    let presented = h.wait(|e| match status_of(&key)(e) {
        Some(AccountStatus::Certificate(Some(details))) => Some(details.presented.to_hex()),
        _ => None,
    });
    assert_eq!(presented, server.hex);
    let pin = |h: &Harness| saved_accounts(h)[0].settings.imap.pin.clone();

    let wrong = "00".repeat(32);
    h.send(Command::AcceptCertificate {
        account: key.clone(),
        service: Service::Imap,
        fingerprint: wrong,
    });
    h.wait(|e| matches!(e, Event::Notice { .. }).then_some(()));
    assert_eq!(pin(&h), None);

    h.send(Command::AcceptCertificate {
        account: key.clone(),
        service: Service::Smtp,
        fingerprint: presented.clone(),
    });
    h.wait(|e| matches!(e, Event::Notice { .. }).then_some(()));
    assert_eq!(pin(&h), None, "the SMTP server was not the one asking");

    h.send(Command::AcceptCertificate {
        account: key.clone(),
        service: Service::Imap,
        fingerprint: presented.clone(),
    });
    h.wait(|e| {
        let moved = status_of(&key)(e).is_some_and(|s| !matches!(s, AccountStatus::Certificate(_)));
        moved.then_some(())
    });
    assert_eq!(pin(&h), Some(presented));
}

#[test]
fn a_broken_account_is_listed_and_not_connected() {
    let mut h = Harness::unstarted();
    h.foreground = true;
    let broken = h.seed_account("broken@example.org", Some("pw"));
    let good = h.seed_account("good@example.org", Some("pw"));
    let path = h.paths().accounts();
    let text = std::fs::read_to_string(&path).unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&text).unwrap();
    for entry in json["accounts"].as_array_mut().unwrap() {
        if entry["key"] == broken.as_str() {
            entry["settings"]["imap"]["pin"] = "not-a-pin".into();
        }
    }
    std::fs::write(&path, json.to_string()).unwrap();
    h.start_core();
    let accounts = h.wait(|e| match e {
        Event::Started { accounts, .. } => Some(accounts.clone()),
        _ => None,
    });
    let info = accounts.iter().find(|a| a.key == broken).unwrap();
    assert!(info.broken);
    assert!(matches!(info.status, AccountStatus::Broken(_)));
    assert!(!accounts.iter().find(|a| a.key == good).unwrap().broken);
    // The healthy one still syncs (tries to connect and goes offline).
    // A wrongly run broken account shows itself as a status other than Broken.
    let moved = std::cell::Cell::new(false);
    let watch = |e: &Event| {
        if status_of(&broken)(e).is_some_and(|s| !matches!(s, AccountStatus::Broken(_))) {
            moved.set(true);
        }
    };
    h.wait(|e| {
        watch(e);
        status_of(&good)(e).filter(|s| matches!(s, AccountStatus::Offline { .. }))
    });
    let quiet = Instant::now() + Duration::from_millis(700);
    while let Some(left) = quiet.checked_duration_since(Instant::now()) {
        if let Ok(event) = h.events.recv_timeout(left) {
            watch(&event);
        }
    }
    assert!(!moved.get(), "a broken account changed status: it ran");
}

#[test]
fn a_pin_change_during_connect_is_refused() {
    let imap = serve(Kind::Imap);
    let smtp = serve(Kind::Smtp);
    let h = Harness::start();
    let other = "00".repeat(32);
    let accepted = vec![(Service::Imap, other), (Service::Smtp, smtp.hex.clone())];
    h.send(request(imap.port, smtp.port, accepted));
    let hex = h.wait(|e| match e {
        Event::TrustPrompt { hex, .. } => Some(hex.clone()),
        _ => None,
    });
    assert_eq!(hex, imap.hex);
    assert!(saved_accounts(&h).is_empty());
}

#[test]
fn a_failed_password_write_rolls_the_account_back() {
    let imap = serve(Kind::Imap);
    let smtp = serve(Kind::Smtp);
    let h = Harness::start();
    started(&h);
    // The password file cannot be written: a directory sits in its place.
    std::fs::create_dir_all(h.data_dir().join(envio_core::secrets::FILE_NAME)).unwrap();
    let accepted = vec![
        (Service::Imap, imap.hex.clone()),
        (Service::Smtp, smtp.hex.clone()),
    ];
    h.send(request(imap.port, smtp.port, accepted));
    let text = h.wait(problem);
    assert!(
        text.starts_with("Could not save the account settings"),
        "{text}"
    );
    assert!(saved_accounts(&h).is_empty());
}

#[test]
fn an_srv_host_needs_confirmation_end_to_end() {
    use envio_core::hooks::{SetupFake, fake_setup};
    use envio_net::autoconfig::{Found, Setting, Source};
    let h = Harness::start();
    started(&h);
    let setting = |host: &str| Setting {
        endpoint: envio_net::Endpoint {
            host: host.into(),
            port: 993,
            security: Security::Tls,
            pin: None,
        },
        username: "ada".into(),
    };
    fake_setup(
        "ada@srv-end.example",
        SetupFake {
            delay: Duration::ZERO,
            found: Some(Found {
                imap: setting("mail.elsewhere.example"),
                smtp: setting("smtp.elsewhere.example"),
                source: Source::Srv,
                confirm: true,
            }),
        },
    );
    h.send(Command::Detect {
        email: "ada@srv-end.example".into(),
    });
    let setup = h.wait(|e| match e {
        Event::Detected { setup, .. } => Some(setup.clone()),
        _ => None,
    });
    h.send(Command::Connect(Box::new(ConnectRequest {
        setup: AccountSetup {
            needs_confirm: false,
            ..setup
        },
        password: SecretString::from("pw".to_owned()),
        servers_confirmed: false,
        accepted: vec![],
    })));
    assert_eq!(h.wait(problem), "Confirm the servers before signing in");
}

#[test]
fn empty_roots_never_prompt_trust() {
    let imap = serve(Kind::Handshake);
    let mut h = Harness::unstarted();
    h.roots = envio_net::Roots::empty();
    h.start_core();
    h.send(request(imap.port, 1, vec![]));
    let event = h.wait(|e| match e {
        Event::TrustPrompt { .. } => Some(None),
        Event::SetupProblem { text, .. } => Some(Some(text.clone())),
        _ => None,
    });
    let text = event.expect("a trust prompt with no roots");
    assert!(
        text.contains("no trusted certificate authorities"),
        "{text}"
    );
}

#[test]
fn a_password_for_a_broken_account_is_refused_and_not_written() {
    let mut h = Harness::unstarted();
    let broken = h.seed_account("broken@example.org", Some("old-pw"));
    let path = h.paths().accounts();
    let text = std::fs::read_to_string(&path).unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&text).unwrap();
    json["accounts"][0]["settings"]["imap"]["pin"] = "not-a-pin".into();
    std::fs::write(&path, json.to_string()).unwrap();
    h.start_core();
    h.send(Command::UpdatePassword {
        account: broken.clone(),
        password: SecretString::from("new-pw".to_owned()),
    });
    let notice = h.wait(|e| match e {
        Event::Notice { text } => Some(text.clone()),
        _ => None,
    });
    assert!(notice.contains("Remove it and add it again"), "{notice}");
    let secrets = Secrets::open(&h.data_dir()).unwrap();
    assert_eq!(secrets.get(&broken).unwrap().expose_secret(), "old-pw");
}
