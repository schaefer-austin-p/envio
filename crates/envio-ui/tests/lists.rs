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
