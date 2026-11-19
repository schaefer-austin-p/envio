//! Accounts stopped by a problem, and the Outbox entry stuck behind one.

mod support;

use envio_core::{AccountStatus, CertDetails, Event, OutboxRow, OutboxStatus, Service};
use envio_net::Fingerprint;
use envio_ui::{Model, Op, Row, Ui};
use support::*;

fn pin(service: Service) -> AccountStatus {
    AccountStatus::Certificate(Some(CertDetails {
        service,
        pinned: Some(Fingerprint::from_bytes([1; 32])),
        presented: Fingerprint::from_bytes([2; 32]),
        reason: None,
        host: "mail.envio.test".into(),
    }))
}

fn status(ui: &mut Ui, account: &str, s: AccountStatus, text: &str) -> Vec<envio_ui::Output> {
    feed(
        ui,
        Event::AccountStatus {
            account: account.into(),
            status: s,
            text: text.into(),
        },
    )
}

fn started() -> Ui {
    let mut ui = Ui::new();
    feed(
        &mut ui,
        Event::Started {
            accounts: vec![info("grace"), info("heidi")],
            cache: envio_core::CacheState::Existing,
        },
    );
    ui
}

fn attention_of(ui: &Ui, at: usize) -> bool {
    let Row::Account(a) = &ui.rows(Model::Accounts)[at] else {
        panic!()
    };
    a.attention
}

fn queued(id: &str, account: &str, status: OutboxStatus) -> OutboxRow {
    OutboxRow {
        id: id.into(),
        account: account.into(),
        subject: "Invoice".into(),
        recipients: vec!["ivan@envio.test".into()],
        queued_at_ms: 0,
        status,
        text: "Waiting to be sent".into(),
    }
}

fn reasons(ui: &Ui) -> Vec<String> {
    ui.rows(Model::Outbox)
        .iter()
        .map(|r| match r {
            Row::Outbox(o) => o.reason.clone(),
            _ => panic!(),
        })
        .collect()
}

#[test]
fn only_sign_in_and_certificate_problems_need_attention() {
    let mut ui = started();
    for (s, want) in [
        (AccountStatus::SignInFailed, true),
        (pin(Service::Imap), true),
        (pin(Service::Smtp), true),
        (AccountStatus::Offline { retry_at_ms: 1 }, false),
        (AccountStatus::Syncing, false),
        (AccountStatus::Ready, false),
    ] {
        status(&mut ui, "heidi", s.clone(), "x");
        assert_eq!(attention_of(&ui, 1), want, "{s:?}");
        assert!(!attention_of(&ui, 0), "the other account is untouched");
    }
}

#[test]
fn a_broken_account_has_no_signal() {
    let mut ui = Ui::new();
    let mut broken = info("grace");
    broken.broken = true;
    feed(
        &mut ui,
        Event::Started {
            accounts: vec![broken],
            cache: envio_core::CacheState::Existing,
        },
    );
    assert!(!attention_of(&ui, 0));
}

#[test]
fn a_waiting_message_says_why_when_the_outgoing_certificate_changed() {
    let mut ui = started();
    feed(
        &mut ui,
        Event::Outbox {
            items: vec![
                queued("1", "grace", OutboxStatus::Queued),
                queued("2", "heidi", OutboxStatus::Queued),
            ],
        },
    );
    assert_eq!(reasons(&ui), ["", ""]);

    let outs = status(
        &mut ui,
        "grace",
        pin(Service::Smtp),
        "The certificate of the outgoing mail server changed.",
    );
    assert!(matches!(changes(&outs, Model::Outbox)[..], [Op::Reset(_)]));
    assert_eq!(
        reasons(&ui),
        ["The certificate of the outgoing mail server changed.", ""]
    );

    let outs = status(&mut ui, "grace", AccountStatus::Ready, "Up to date");
    assert!(!changes(&outs, Model::Outbox).is_empty());
    assert_eq!(reasons(&ui), ["", ""]);
}

#[test]
fn the_reason_also_shows_when_the_list_arrives_after_the_problem() {
    let mut ui = started();
    status(&mut ui, "grace", pin(Service::Smtp), "Outgoing changed");
    feed(
        &mut ui,
        Event::Outbox {
            items: vec![queued("1", "grace", OutboxStatus::Queued)],
        },
    );
    assert_eq!(reasons(&ui), ["Outgoing changed"]);
}

#[test]
fn an_incoming_problem_or_another_status_gives_no_reason() {
    let mut ui = started();
    feed(
        &mut ui,
        Event::Outbox {
            items: vec![
                queued("1", "grace", OutboxStatus::Queued),
                queued("2", "heidi", OutboxStatus::Failed { reason: "x".into() }),
            ],
        },
    );
    status(&mut ui, "grace", pin(Service::Imap), "Incoming changed");
    status(&mut ui, "heidi", pin(Service::Smtp), "Outgoing changed");
    assert_eq!(reasons(&ui), ["", ""]);
}

#[test]
fn a_status_change_that_leaves_the_reasons_alone_sends_no_outbox_change() {
    let mut ui = started();
    feed(
        &mut ui,
        Event::Outbox {
            items: vec![queued("1", "grace", OutboxStatus::Queued)],
        },
    );
    let outs = status(&mut ui, "grace", AccountStatus::Syncing, "Syncing");
    assert!(changes(&outs, Model::Outbox).is_empty());
}

fn actions(ui: &Ui) -> Vec<String> {
    ui.rows(Model::Outbox)
        .iter()
        .map(|r| match r {
            Row::Outbox(o) => o.reason_action.clone(),
            _ => panic!(),
        })
        .collect()
}

#[test]
fn a_failed_sign_in_holds_a_waiting_message_and_says_so() {
    let mut ui = started();
    feed(
        &mut ui,
        Event::Outbox {
            items: vec![
                queued("1", "grace", OutboxStatus::Queued),
                queued("2", "heidi", OutboxStatus::Queued),
            ],
        },
    );
    status(
        &mut ui,
        "grace",
        AccountStatus::SignInFailed,
        "Sign-in failed: check the password.",
    );
    assert_eq!(reasons(&ui), ["Sign-in failed: check the password.", ""]);
    assert_eq!(actions(&ui), ["Open accounts", ""]);
    status(&mut ui, "grace", AccountStatus::Ready, "Up to date");
    assert_eq!(reasons(&ui), ["", ""]);
}

#[test]
fn the_certificate_case_keeps_its_own_button_wording() {
    let mut ui = started();
    status(&mut ui, "grace", pin(Service::Smtp), "Outgoing changed");
    feed(
        &mut ui,
        Event::Outbox {
            items: vec![queued("1", "grace", OutboxStatus::Queued)],
        },
    );
    assert_eq!(actions(&ui), ["Check certificate"]);
}

fn signin_again(ui: &Ui) -> Vec<bool> {
    ui.rows(Model::Outbox)
        .iter()
        .map(|r| match r {
            Row::Outbox(o) => o.signin_again,
            _ => panic!(),
        })
        .collect()
}

#[test]
fn only_an_oauth_account_that_failed_to_sign_in_is_marked_for_sign_in_again() {
    let mut ui = Ui::new();
    let mut ms = info("grace");
    ms.oauth = true;
    feed(
        &mut ui,
        Event::Started {
            accounts: vec![ms, info("heidi")],
            cache: envio_core::CacheState::Existing,
        },
    );
    let Row::Account(a) = &ui.rows(Model::Accounts)[0] else {
        panic!()
    };
    assert!(a.oauth);
    feed(
        &mut ui,
        Event::Outbox {
            items: vec![
                queued("1", "grace", OutboxStatus::Queued),
                queued("2", "heidi", OutboxStatus::Queued),
            ],
        },
    );
    assert_eq!(signin_again(&ui), [false, false]);
    status(
        &mut ui,
        "grace",
        AccountStatus::SignInFailed,
        "Sign in again",
    );
    status(
        &mut ui,
        "heidi",
        AccountStatus::SignInFailed,
        "Sign-in failed",
    );
    assert_eq!(signin_again(&ui), [true, false]);
}
