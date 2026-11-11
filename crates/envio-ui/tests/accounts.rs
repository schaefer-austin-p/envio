mod support;

use envio_core::{AccountStatus, CertDetails, Event, Service};
use envio_net::Fingerprint;
use envio_ui::{Model, Op, Row, Signal, Ui};
use support::*;

fn start(ui: &mut Ui) {
    let accounts = vec![info("a"), info("b")];
    feed(
        ui,
        Event::Started {
            accounts,
            cache: envio_core::CacheState::Existing,
        },
    );
}

fn keys(op: &Op) -> Vec<String> {
    match op {
        Op::Reset(rows) => rows
            .iter()
            .map(|r| match r {
                Row::Account(a) => a.key.clone(),
                _ => panic!(),
            })
            .collect(),
        _ => panic!("{op:?}"),
    }
}

#[test]
fn started_resets_accounts() {
    let mut ui = Ui::new();
    let mut broken = info("b");
    broken.broken = true;
    let outs = feed(
        &mut ui,
        Event::Started {
            accounts: vec![info("a"), broken],
            cache: envio_core::CacheState::Fresh,
        },
    );
    let ops = changes(&outs, Model::Accounts);
    assert_eq!(keys(ops[0]), ["a", "b"]);
    let Op::Reset(rows) = ops[0] else { panic!() };
    let Row::Account(b) = &rows[1] else { panic!() };
    assert_eq!(b.state, "broken");
}

#[test]
fn status_updates_one_row() {
    let mut ui = Ui::new();
    start(&mut ui);
    let outs = feed(
        &mut ui,
        Event::AccountStatus {
            account: "b".into(),
            status: AccountStatus::SignInFailed,
            text: "Sign-in failed".into(),
        },
    );
    let ops = changes(&outs, Model::Accounts);
    let [Op::Update { at: 1, row }] = ops[..] else {
        panic!("{ops:?}")
    };
    let Row::Account(b) = row else { panic!() };
    assert_eq!(
        (b.status.as_str(), b.state.as_str()),
        ("Sign-in failed", "auth")
    );
    assert!(matches!(signals(&outs)[..], [Signal::Status { .. }]));
}

#[test]
fn removed_account_leaves_the_model_and_its_rows() {
    let mut ui = Ui::new();
    start(&mut ui);
    opened(
        &mut ui,
        F1,
        vec![
            msg_in(3, 30, 1, "a"),
            msg_in(2, 20, 1, "b"),
            msg_in(1, 10, 1, "b"),
        ],
        true,
    );
    let outs = feed(
        &mut ui,
        Event::AccountRemoved {
            account: "b".into(),
        },
    );
    let acc = changes(&outs, Model::Accounts);
    assert!(matches!(acc[..], [Op::Remove { at: 1, count: 1 }]));
    let msgs = changes(&outs, Model::Messages);
    assert!(
        matches!(msgs[..], [Op::Remove { at: 1, count: 2 }]),
        "{msgs:?}"
    );
}

#[test]
fn an_account_already_listed_is_not_added_twice() {
    let mut ui = Ui::new();
    start(&mut ui);
    let outs = feed(&mut ui, Event::AccountAdded { account: info("b") });
    let ops = changes(&outs, Model::Accounts);
    assert!(matches!(ops[..], [Op::Update { at: 1, .. }]), "{ops:?}");
    let outs = feed(&mut ui, Event::AccountAdded { account: info("c") });
    assert!(
        matches!(signals(&outs)[..], [Signal::AccountAdded { account, email }]
            if account == "c" && email == "c@example.org"),
        "the signal names the account and its address"
    );
    let ops = changes(&outs, Model::Accounts);
    assert!(matches!(ops[..], [Op::Insert { at: 2, .. }]), "{ops:?}");
}

fn account_row_of(outs: &[envio_ui::Output]) -> envio_ui::AccountRow {
    let ops = changes(outs, Model::Accounts);
    let [
        Op::Update {
            row: Row::Account(row),
            ..
        },
    ] = ops[..]
    else {
        panic!("{ops:?}")
    };
    row.clone()
}

#[test]
fn a_pin_status_fills_the_row_and_clears_after() {
    let mut ui = Ui::new();
    start(&mut ui);
    let presented = Fingerprint::from_bytes([0xab; 32]);
    let details = CertDetails {
        service: Service::Smtp,
        pinned: Some(Fingerprint::from_bytes([0x01; 32])),
        presented,
        reason: None,
    };
    let outs = feed(
        &mut ui,
        Event::AccountStatus {
            account: "b".into(),
            status: AccountStatus::Certificate(Some(details)),
            text: "The certificate changed".into(),
        },
    );
    let row = account_row_of(&outs);
    assert_eq!(row.state, "pin");
    assert_eq!(row.pin_service, "smtp");
    assert_eq!(row.pin_fingerprint, presented.display());
    assert_eq!(row.pin_hex, presented.to_hex());
    assert_eq!(row.pin_text, "The certificate changed");

    let outs = feed(
        &mut ui,
        Event::AccountStatus {
            account: "b".into(),
            status: AccountStatus::Ready,
            text: "Up to date".into(),
        },
    );
    let row = account_row_of(&outs);
    assert_eq!(row.state, "ok");
    for field in [
        &row.pin_service,
        &row.pin_fingerprint,
        &row.pin_hex,
        &row.pin_text,
    ] {
        assert!(field.is_empty());
    }
}
