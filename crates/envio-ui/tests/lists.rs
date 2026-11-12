mod support;

use envio_core::{Command, Event, FolderId, FolderRole, ListRef};
use envio_ui::{Model, Op, Ui};
use support::*;

fn changed(ui: &mut Ui, folders: &[i64]) -> Vec<envio_ui::Output> {
    let folders = folders.iter().map(|&f| FolderId(f)).collect();
    feed(ui, Event::MessagesChanged { folders })
}

#[test]
fn a_stale_page_is_dropped() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(2, 20), msg(1, 10)], false);
    let _ = ui.more().unwrap();
    // Another list's answer, and a page after another cursor.
    let other = ListRef::Folder(FolderId(9));
    assert!(feed(&mut ui, page(other, None, vec![msg(5, 50)], true)).is_empty());
    let late = page(F1, Some(cursor(7, 70)), vec![msg(0, 5)], true);
    assert!(feed(&mut ui, late).is_empty());
    // The outstanding one still lands.
    let ok = page(F1, Some(cursor(1, 10)), vec![msg(0, 5)], true);
    assert_eq!(changes(&feed(&mut ui, ok), Model::Messages).len(), 1);
}

#[test]
fn more_waits_for_the_outstanding_page() {
    let mut ui = Ui::new();
    let _ = ui.open_list(F1);
    assert!(ui.more().is_none(), "first page still outstanding");
    feed(&mut ui, page(F1, None, vec![msg(2, 20), msg(1, 10)], false));
    match ui.more() {
        Some(Command::ListMessages { list, after }) => {
            assert_eq!(list, F1);
            assert_eq!(after, Some(cursor(1, 10)));
        }
        other => panic!("{other:?}"),
    }
    assert!(ui.more().is_none());
    feed(
        &mut ui,
        page(F1, Some(cursor(1, 10)), vec![msg(0, 5)], false),
    );
    assert!(ui.more().is_some());
}

#[test]
fn more_stops_at_the_end() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(2, 20)], true);
    assert!(ui.more().is_none());
}

#[test]
fn a_refresh_inserts_new_mail_at_the_top_without_reset() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(2, 20), msg(1, 10)], false);
    let outs = changed(&mut ui, &[1]);
    assert_eq!(sends(&outs), 1);
    let outs = feed(&mut ui, page(F1, None, vec![msg(3, 30), msg(2, 20)], false));
    let ops = changes(&outs, Model::Messages);
    assert_eq!(ops.len(), 1, "{ops:?}");
    assert!(matches!(ops[0], Op::Insert { at: 0, .. }));
    assert_eq!(ids(ops[0]), vec![3]);
    // The walk reads the page below (row 1) and finds it unchanged.
    let outs = feed(
        &mut ui,
        page(F1, Some(cursor(2, 20)), vec![msg(1, 10)], true),
    );
    assert!(changes(&outs, Model::Messages).is_empty());
    // A changed row is an update in place.
    changed(&mut ui, &[1]);
    let mut flagged = msg(2, 20);
    flagged.flags = envio_core::Flags::FLAGGED;
    let outs = feed(&mut ui, page(F1, None, vec![msg(3, 30), flagged], false));
    let ops = changes(&outs, Model::Messages);
    assert!(matches!(ops[..], [Op::Update { at: 1, .. }]), "{ops:?}");
}

#[test]
fn a_refresh_removes_deleted_rows() {
    let mut ui = Ui::new();
    opened(
        &mut ui,
        F1,
        vec![msg(4, 40), msg(3, 30), msg(2, 20), msg(1, 10)],
        true,
    );
    changed(&mut ui, &[1]);
    let outs = feed(&mut ui, page(F1, None, vec![msg(4, 40), msg(1, 10)], true));
    let ops = changes(&outs, Model::Messages);
    assert!(
        matches!(ops[..], [Op::Remove { at: 1, count: 2 }]),
        "{ops:?}"
    );
}

#[test]
fn rows_beyond_the_first_page_survive_a_refresh() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(4, 40), msg(3, 30)], false);
    let _ = ui.more();
    feed(
        &mut ui,
        page(F1, Some(cursor(3, 30)), vec![msg(2, 20), msg(1, 10)], true),
    );
    changed(&mut ui, &[1]);
    // The first page is now 5 and 4; 3 fell out of it but is merely older.
    let outs = feed(&mut ui, page(F1, None, vec![msg(5, 50), msg(4, 40)], false));
    let ops = changes(&outs, Model::Messages);
    // 3, 2 and 1 are below the page's bound (40): none is removed.
    assert!(matches!(ops[..], [Op::Insert { at: 0, .. }]), "{ops:?}");
}

#[test]
fn changes_in_other_folders_are_ignored() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(1, 10)], true);
    assert!(changed(&mut ui, &[2, 3]).is_empty());
}

#[test]
fn the_unified_inbox_reacts_to_any_inbox() {
    let mut ui = Ui::new();
    feed(
        &mut ui,
        Event::Folders {
            account: "a".into(),
            folders: vec![folder(1, FolderRole::Inbox), folder(2, FolderRole::Sent)],
        },
    );
    feed(
        &mut ui,
        Event::Folders {
            account: "b".into(),
            folders: vec![folder(7, FolderRole::Inbox)],
        },
    );
    opened(&mut ui, ListRef::Inboxes, vec![msg(1, 10)], true);
    assert_eq!(sends(&changed(&mut ui, &[7])), 1);
    feed(
        &mut ui,
        page(ListRef::Inboxes, None, vec![msg(1, 10)], true),
    );
    assert!(changed(&mut ui, &[2]).is_empty(), "Sent is not an Inbox");
}

#[test]
fn a_change_in_an_unseen_folder_refreshes_the_unified_inbox() {
    let mut ui = Ui::new();
    feed(
        &mut ui,
        Event::Folders {
            account: "a".into(),
            folders: vec![folder(2, FolderRole::Sent)],
        },
    );
    opened(&mut ui, ListRef::Inboxes, vec![msg(1, 10)], true);
    assert_eq!(sends(&changed(&mut ui, &[99])), 1);
}

#[test]
fn a_refresh_removes_only_rows_inside_a_partial_first_page() {
    let mut ui = Ui::new();
    opened(
        &mut ui,
        F1,
        vec![msg(5, 50), msg(4, 40), msg(3, 30), msg(2, 20), msg(1, 10)],
        false,
    );
    changed(&mut ui, &[1]);
    // The page (not the end) is 5, 3 down to the bound 30: 4 is gone,
    // 2 and 1 lie beyond the bound and stay.
    let outs = feed(&mut ui, page(F1, None, vec![msg(5, 50), msg(3, 30)], false));
    let ops = changes(&outs, Model::Messages);
    assert!(
        matches!(ops[..], [Op::Remove { at: 1, count: 1 }]),
        "{ops:?}"
    );
}

#[test]
fn a_refresh_inserts_in_the_middle() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(4, 40), msg(2, 20)], true);
    changed(&mut ui, &[1]);
    let rows = vec![msg(4, 40), msg(3, 30), msg(2, 20)];
    let outs = feed(&mut ui, page(F1, None, rows, true));
    let ops = changes(&outs, Model::Messages);
    assert!(matches!(ops[..], [Op::Insert { at: 1, .. }]), "{ops:?}");
    assert_eq!(ids(ops[0]), vec![3]);
}

/// Page one (4, 3) and the page after it (2, 1) are loaded.
fn two_pages(ui: &mut Ui) {
    opened(ui, F1, vec![msg(4, 40), msg(3, 30)], false);
    let _ = ui.more();
    feed(
        ui,
        page(F1, Some(cursor(3, 30)), vec![msg(2, 20), msg(1, 10)], true),
    );
}

fn after_of(outs: &[envio_ui::Output]) -> Vec<Option<envio_core::Cursor>> {
    outs.iter()
        .filter_map(|o| match o {
            envio_ui::Output::Send(Command::ListMessages { after, .. }) => Some(*after),
            _ => None,
        })
        .collect()
}

#[test]
fn a_row_gone_past_the_first_page_is_removed_by_a_refresh() {
    let mut ui = Ui::new();
    two_pages(&mut ui);
    changed(&mut ui, &[1]);
    // Page one is unchanged; the refresh walks on to the page below it.
    let outs = feed(&mut ui, page(F1, None, vec![msg(4, 40), msg(3, 30)], false));
    assert!(changes(&outs, Model::Messages).is_empty());
    assert_eq!(after_of(&outs), vec![Some(cursor(3, 30))]);
    // Row 2 was archived: it goes, as a row operation.
    let outs = feed(
        &mut ui,
        page(F1, Some(cursor(3, 30)), vec![msg(1, 10)], true),
    );
    let ops = changes(&outs, Model::Messages);
    assert!(
        matches!(ops[..], [Op::Remove { at: 2, count: 1 }]),
        "{ops:?}"
    );
    assert!(after_of(&outs).is_empty());
}

#[test]
fn a_row_changed_past_the_first_page_is_updated_by_a_refresh() {
    let mut ui = Ui::new();
    two_pages(&mut ui);
    changed(&mut ui, &[1]);
    feed(&mut ui, page(F1, None, vec![msg(4, 40), msg(3, 30)], false));
    let mut read = msg(1, 10);
    read.flags = envio_core::Flags::SEEN;
    let outs = feed(
        &mut ui,
        page(F1, Some(cursor(3, 30)), vec![msg(2, 20), read], true),
    );
    let ops = changes(&outs, Model::Messages);
    assert!(matches!(ops[..], [Op::Update { at: 3, .. }]), "{ops:?}");
}

#[test]
fn a_refresh_walk_reads_only_the_loaded_pages() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(4, 40), msg(3, 30)], false);
    changed(&mut ui, &[1]);
    let outs = feed(&mut ui, page(F1, None, vec![msg(4, 40), msg(3, 30)], false));
    assert!(
        after_of(&outs).is_empty(),
        "nothing is loaded below page one"
    );
}

#[test]
fn a_change_during_the_walk_refreshes_again_after_it() {
    let mut ui = Ui::new();
    two_pages(&mut ui);
    changed(&mut ui, &[1]);
    feed(&mut ui, page(F1, None, vec![msg(4, 40), msg(3, 30)], false));
    assert!(changed(&mut ui, &[1]).is_empty());
    assert!(ui.more().is_none(), "paging waits for the walk");
    let outs = feed(
        &mut ui,
        page(F1, Some(cursor(3, 30)), vec![msg(2, 20), msg(1, 10)], true),
    );
    assert_eq!(after_of(&outs), vec![None]);
}

fn failed(ui: &mut Ui, after: Option<envio_core::Cursor>) {
    feed(ui, Event::MessagesFailed { list: F1, after });
}

#[test]
fn a_failed_first_page_lets_a_refresh_through() {
    let mut ui = Ui::new();
    let _ = ui.open_list(F1);
    failed(&mut ui, None);
    // The list is empty and idle: new mail is asked for again.
    assert_eq!(sends(&changed(&mut ui, &[1])), 1);
    let outs = feed(&mut ui, page(F1, None, vec![msg(2, 20)], true));
    assert_eq!(changes(&outs, Model::Messages).len(), 1);
}

#[test]
fn a_failed_refresh_lets_the_next_one_through() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(2, 20)], true);
    assert_eq!(sends(&changed(&mut ui, &[1])), 1);
    assert!(changed(&mut ui, &[1]).is_empty(), "outstanding");
    failed(&mut ui, None);
    assert_eq!(sends(&changed(&mut ui, &[1])), 1);
}

#[test]
fn a_failed_walk_page_lets_the_next_refresh_through() {
    let mut ui = Ui::new();
    two_pages(&mut ui);
    changed(&mut ui, &[1]);
    feed(&mut ui, page(F1, None, vec![msg(4, 40), msg(3, 30)], false));
    failed(&mut ui, Some(cursor(3, 30)));
    assert_eq!(sends(&changed(&mut ui, &[1])), 1);
}

#[test]
fn a_failed_next_page_can_be_asked_again() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(2, 20), msg(1, 10)], false);
    assert!(ui.more().is_some());
    assert!(ui.more().is_none());
    failed(&mut ui, Some(cursor(1, 10)));
    assert!(ui.more().is_some());
}
