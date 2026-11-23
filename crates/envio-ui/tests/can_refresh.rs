//! Whether a pull to refresh can do anything, and the ack of a refresh.

mod support;

use envio_core::{AccountInfo, AccountStatus, Event, FolderRole, ListRef};
use envio_ui::{Signal, Ui};
use support::*;

fn with(ui: &mut Ui, accounts: Vec<AccountInfo>) {
    feed(
        ui,
        Event::Started {
            accounts,
            cache: envio_core::CacheState::Existing,
        },
    );
}

fn status(ui: &mut Ui, account: &str, s: AccountStatus) {
    feed(
        ui,
        Event::AccountStatus {
            account: account.into(),
            status: s,
            text: String::new(),
        },
    );
}

fn broken(key: &str) -> AccountInfo {
    AccountInfo {
        broken: true,
        status: AccountStatus::Broken("unreadable".into()),
        ..info(key)
    }
}

#[test]
fn no_accounts_cannot_refresh() {
    let ui = Ui::new();
    assert!(!ui.can_refresh(None));
    assert!(!ui.can_refresh(Some("a")));
}

#[test]
fn unified_can_refresh_while_one_account_is_fine() {
    let mut ui = Ui::new();
    with(&mut ui, vec![info("a"), broken("b"), info("c")]);
    status(&mut ui, "a", AccountStatus::SignInFailed);
    assert!(ui.can_refresh(None), "c is fine");
    status(&mut ui, "c", AccountStatus::SignInFailed);
    assert!(
        !ui.can_refresh(None),
        "sign-in failed, broken, sign-in failed"
    );
}

#[test]
fn a_tab_asks_about_its_own_account() {
    let mut ui = Ui::new();
    with(&mut ui, vec![info("a"), broken("b"), info("c")]);
    status(&mut ui, "a", AccountStatus::Offline { retry_at_ms: 1 });
    status(&mut ui, "c", AccountStatus::Certificate(None));
    assert!(ui.can_refresh(Some("a")), "offline: refresh is its retry");
    assert!(!ui.can_refresh(Some("b")), "broken");
    assert!(!ui.can_refresh(Some("c")), "certificate: needs the user");
    assert!(!ui.can_refresh(Some("nobody")));
}

#[test]
fn the_shown_list_sets_the_scope() {
    let mut ui = Ui::new();
    with(&mut ui, vec![info("a"), info("b")]);
    status(&mut ui, "b", AccountStatus::SignInFailed);
    opened(&mut ui, ListRef::Inboxes, vec![], true);
    assert!(ui.can_refresh_shown(), "unified: a is fine");
    feed(
        &mut ui,
        Event::Folders {
            account: "b".into(),
            folders: vec![folder(1, FolderRole::Inbox)],
        },
    );
    opened(&mut ui, F1, vec![msg_in(1, 10, 1, "b")], true);
    assert!(!ui.can_refresh_shown(), "b's folder: b needs the user");
    status(&mut ui, "b", AccountStatus::Ready);
    assert!(ui.can_refresh_shown());
}

#[test]
fn refreshed_becomes_a_signal() {
    let mut ui = Ui::new();
    let outs = feed(&mut ui, Event::Refreshed);
    assert_eq!(signals(&outs), vec![&Signal::Refreshed]);
}
