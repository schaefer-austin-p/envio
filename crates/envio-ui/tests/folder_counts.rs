mod support;

use envio_core::{Command, Event, FolderId, FolderRole};
use envio_ui::{Output, Ui};
use support::*;

fn list_of(outs: &[Output]) -> Vec<String> {
    outs.iter()
        .filter_map(|o| match o {
            Output::Send(Command::ListFolders { account }) => Some(account.clone()),
            _ => None,
        })
        .collect()
}

fn shown(ui: &mut Ui, account: &str, rows: Vec<(i64, FolderRole)>) -> Vec<Output> {
    let folders = rows.into_iter().map(|(id, r)| folder(id, r)).collect();
    feed(
        ui,
        Event::Folders {
            account: account.into(),
            folders,
        },
    )
}

fn changed(ui: &mut Ui, folders: &[i64]) -> Vec<Output> {
    let folders = folders.iter().map(|&f| FolderId(f)).collect();
    feed(ui, Event::MessagesChanged { folders })
}

fn two_accounts() -> Ui {
    let mut ui = Ui::new();
    shown(&mut ui, "b", vec![(20, FolderRole::Inbox)]);
    shown(
        &mut ui,
        "a",
        vec![(10, FolderRole::Inbox), (11, FolderRole::Sent)],
    );
    ui.set_drawer_account(Some("a".into()));
    ui
}

#[test]
fn reading_in_a_shown_folder_lists_the_folders_again() {
    let mut ui = two_accounts();
    assert_eq!(list_of(&changed(&mut ui, &[10])), ["a"]);
}

#[test]
fn a_folder_not_listed_yet_lists_nothing() {
    // It is not known to be the drawer account's; new folders arrive as
    // FoldersChanged, which lists separately.
    let mut ui = two_accounts();
    assert!(list_of(&changed(&mut ui, &[99])).is_empty());
}

#[test]
fn a_failed_list_does_not_freeze_the_counts() {
    let mut ui = two_accounts();
    assert_eq!(list_of(&changed(&mut ui, &[10])).len(), 1);
    assert!(list_of(&changed(&mut ui, &[10])).is_empty());
    // The core could not read the folders: a Notice, no Folders.
    feed(
        &mut ui,
        Event::Notice {
            text: "could not read".into(),
        },
    );
    assert_eq!(list_of(&changed(&mut ui, &[10])), ["a"]);
}

#[test]
fn changes_in_another_account_do_not_list() {
    let mut ui = two_accounts();
    assert!(list_of(&changed(&mut ui, &[20])).is_empty());
}

#[test]
fn account_unread_for_the_shown_account_lists_again() {
    let mut ui = two_accounts();
    let unread = |account: &str| Event::AccountUnread {
        account: account.into(),
        unread: 0,
    };
    assert!(list_of(&feed(&mut ui, unread("b"))).is_empty());
    assert_eq!(list_of(&feed(&mut ui, unread("a"))), ["a"]);
}

#[test]
fn a_burst_of_changes_sends_at_most_two_lists() {
    let mut ui = two_accounts();
    let mut sent = 0;
    for _ in 0..5 {
        sent += list_of(&changed(&mut ui, &[10])).len();
    }
    assert_eq!(sent, 1);
    let after = shown(&mut ui, "a", vec![(10, FolderRole::Inbox)]);
    assert_eq!(list_of(&after), ["a"]);
    // That second list is outstanding too; its answer ends the burst.
    let done = shown(&mut ui, "a", vec![(10, FolderRole::Inbox)]);
    assert!(list_of(&done).is_empty());
}

#[test]
fn an_answer_with_nothing_new_sends_nothing() {
    let mut ui = two_accounts();
    assert_eq!(list_of(&changed(&mut ui, &[10])).len(), 1);
    assert!(list_of(&shown(&mut ui, "a", vec![(10, FolderRole::Inbox)])).is_empty());
}

#[test]
fn nothing_is_listed_when_no_folders_are_shown() {
    let mut ui = Ui::new();
    assert!(list_of(&changed(&mut ui, &[10])).is_empty());
    let unread = Event::AccountUnread {
        account: "a".into(),
        unread: 1,
    };
    assert!(list_of(&feed(&mut ui, unread)).is_empty());
}

#[test]
fn another_accounts_answer_does_not_end_a_pending_list() {
    let mut ui = two_accounts();
    assert_eq!(list_of(&changed(&mut ui, &[10])).len(), 1);
    // The user opens b; its list lands while a's is outstanding.
    assert!(list_of(&shown(&mut ui, "b", vec![(20, FolderRole::Inbox)])).is_empty());
    // The drawer moves to b: it gets its own list, not folded into a's.
    ui.set_drawer_account(Some("b".into()));
    assert_eq!(list_of(&changed(&mut ui, &[20])), ["b"]);
}

#[test]
fn a_pending_list_for_a_removed_account_does_not_block_the_next() {
    let mut ui = two_accounts();
    assert_eq!(list_of(&changed(&mut ui, &[10])).len(), 1);
    feed(
        &mut ui,
        Event::AccountRemoved {
            account: "a".into(),
        },
    );
    shown(&mut ui, "b", vec![(20, FolderRole::Inbox)]);
    ui.set_drawer_account(Some("b".into()));
    assert_eq!(list_of(&changed(&mut ui, &[20])), ["b"]);
}

#[test]
fn a_closed_drawer_lists_nothing() {
    let mut ui = two_accounts();
    ui.set_drawer_account(None);
    assert!(list_of(&changed(&mut ui, &[10])).is_empty());
    let unread = Event::AccountUnread {
        account: "a".into(),
        unread: 2,
    };
    assert!(list_of(&feed(&mut ui, unread)).is_empty());
}

#[test]
fn closing_the_drawer_while_a_list_is_pending_drops_the_follow_up() {
    let mut ui = two_accounts();
    assert_eq!(list_of(&changed(&mut ui, &[10])).len(), 1);
    assert!(list_of(&changed(&mut ui, &[10])).is_empty());
    ui.set_drawer_account(None);
    assert!(list_of(&shown(&mut ui, "a", vec![(10, FolderRole::Inbox)])).is_empty());
}

#[test]
fn removing_the_drawers_account_clears_it() {
    let mut ui = two_accounts();
    feed(
        &mut ui,
        Event::AccountRemoved {
            account: "a".into(),
        },
    );
    // Same id again: a re-added account must not be listed unasked.
    shown(&mut ui, "a", vec![(10, FolderRole::Inbox)]);
    assert!(list_of(&changed(&mut ui, &[10])).is_empty());
}

#[test]
fn an_unrelated_notice_keeps_a_folded_in_change() {
    let mut ui = two_accounts();
    assert_eq!(list_of(&changed(&mut ui, &[10])).len(), 1);
    // A change while the list is outstanding is folded in.
    assert!(list_of(&changed(&mut ui, &[10])).is_empty());
    // An unrelated Notice, then the late answer: it still lists once more.
    feed(
        &mut ui,
        Event::Notice {
            text: "sent".into(),
        },
    );
    let late = shown(&mut ui, "a", vec![(10, FolderRole::Inbox)]);
    assert_eq!(list_of(&late), ["a"]);
}

#[test]
fn another_accounts_late_answer_keeps_the_folded_in_change() {
    let mut ui = two_accounts();
    assert_eq!(list_of(&changed(&mut ui, &[10])).len(), 1);
    assert!(list_of(&changed(&mut ui, &[10])).is_empty());
    feed(
        &mut ui,
        Event::Notice {
            text: "sent".into(),
        },
    );
    // Account b's answer arrives first: it does not use up a's change.
    let other = shown(&mut ui, "b", vec![(20, FolderRole::Inbox)]);
    assert!(list_of(&other).is_empty());
    let late = shown(&mut ui, "a", vec![(10, FolderRole::Inbox)]);
    assert_eq!(list_of(&late), ["a"]);
}
