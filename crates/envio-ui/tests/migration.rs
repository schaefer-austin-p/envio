//! The move to Graph on the account's row (#342).

mod support;

use envio_core::{AccountStatus, Event, MigrationState};
use envio_ui::{Model, Op, Row, Ui};
use support::*;

fn state(stage: envio_core::MigrationStage) -> MigrationState {
    MigrationState {
        stage,
        title: "Moving to the new connection".into(),
        text: "Finishing 2 changes first.".into(),
        hint: "You can leave this page.".into(),
        note_title: String::new(),
        note_text: String::new(),
        can_switch: false,
    }
}

fn row_of(outs: &[envio_ui::Output]) -> envio_ui::AccountRow {
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

fn started(update_sign_in: bool) -> Ui {
    let mut ui = Ui::new();
    let mut account = info("b");
    account.oauth = true;
    account.update_sign_in = update_sign_in;
    feed(
        &mut ui,
        Event::Started {
            accounts: vec![account],
            cache: envio_core::CacheState::Existing,
        },
    );
    ui
}

#[test]
fn an_eligible_account_row_offers_update_sign_in() {
    let mut ui = started(true);
    let outs = feed(
        &mut ui,
        Event::AccountStatus {
            account: "b".into(),
            status: AccountStatus::Ready,
            text: "Up to date".into(),
        },
    );
    assert!(row_of(&outs).update_sign_in);
}

#[test]
fn the_move_fills_the_row_and_replaces_the_status_line() {
    let mut ui = started(true);
    let mut moving = state(envio_core::MigrationStage::Moving);
    moving.can_switch = true;
    let outs = feed(
        &mut ui,
        Event::Migration {
            account: "b".into(),
            state: Some(moving),
        },
    );
    let row = row_of(&outs);
    assert_eq!(row.moving.stage, "moving");
    assert_eq!(row.status, "Moving to the new connection");
    assert!(row.moving.can_switch);
    assert!(!row.update_sign_in, "the offer ends once the move runs");
    // A sync status does not overwrite the line while moving.
    let outs = feed(
        &mut ui,
        Event::AccountStatus {
            account: "b".into(),
            status: AccountStatus::Ready,
            text: "Up to date".into(),
        },
    );
    assert_eq!(row_of(&outs).status, "Moving to the new connection");
}

#[test]
fn done_keeps_the_notice_and_returns_the_status_line() {
    let mut ui = started(true);
    let mut done = state(envio_core::MigrationStage::Done);
    done.note_title = "1 change was not made".into();
    let outs = feed(
        &mut ui,
        Event::Migration {
            account: "b".into(),
            state: Some(done),
        },
    );
    let row = row_of(&outs);
    assert_eq!(row.moving.stage, "done");
    assert_eq!(row.moving.note_title, "1 change was not made");
    let outs = feed(
        &mut ui,
        Event::AccountStatus {
            account: "b".into(),
            status: AccountStatus::Ready,
            text: "Up to date".into(),
        },
    );
    assert_eq!(row_of(&outs).status, "Up to date");
}

#[test]
fn a_move_for_an_unknown_account_changes_nothing() {
    let mut ui = started(false);
    let outs = feed(
        &mut ui,
        Event::Migration {
            account: "zzz".into(),
            state: Some(state(envio_core::MigrationStage::Moving)),
        },
    );
    assert!(changes(&outs, Model::Accounts).is_empty());
}
