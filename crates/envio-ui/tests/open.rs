mod support;

use envio_core::{Event, FolderId, ListRef, SearchScope};
use envio_ui::{Model, Op, Row, Ui};
use support::*;

/// The ids of the rows a model shows as open.
fn open_ids(ui: &Ui, model: Model) -> Vec<i64> {
    ui.rows(model)
        .iter()
        .filter_map(|r| match r {
            Row::Message(m) if m.open => Some(m.id),
            _ => None,
        })
        .collect()
}

fn selected_ids(ui: &Ui, model: Model) -> Vec<i64> {
    ui.rows(model)
        .iter()
        .filter_map(|r| match r {
            Row::Message(m) if m.selected => Some(m.id),
            _ => None,
        })
        .collect()
}

fn three() -> Ui {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(3, 30), msg(2, 20), msg(1, 10)], false);
    ui
}

#[test]
fn set_open_marks_one_row_and_unmarks_the_previous() {
    let mut ui = three();
    assert_eq!(ui.open_message(), None);
    let outs = ui.set_open(Some(1));
    assert_eq!(changes(&outs, Model::Messages).len(), 1);
    assert_eq!(open_ids(&ui, Model::Messages), [1]);
    assert_eq!(ui.open_message(), Some(1));

    let outs = ui.set_open(Some(2));
    // One update unmarks row 1, one marks row 2.
    assert_eq!(changes(&outs, Model::Messages).len(), 2);
    assert_eq!(open_ids(&ui, Model::Messages), [2]);
    assert_eq!(ui.open_message(), Some(2));

    let outs = ui.set_open(None);
    assert!(matches!(
        changes(&outs, Model::Messages)[..],
        [Op::Update { .. }]
    ));
    assert_eq!(open_ids(&ui, Model::Messages), Vec::<i64>::new());
    assert_eq!(ui.open_message(), None);
    // Nothing changes: no outputs.
    assert!(ui.set_open(None).is_empty());
}

#[test]
fn open_and_selected_are_independent() {
    let mut ui = three();
    ui.set_open(Some(1));
    ui.select(2, true);
    assert_eq!(open_ids(&ui, Model::Messages), [1]);
    assert_eq!(selected_ids(&ui, Model::Messages), [2]);
    // Selecting the open row keeps it open; clearing keeps it open too.
    ui.select(1, true);
    assert_eq!(open_ids(&ui, Model::Messages), [1]);
    ui.clear_selection();
    assert_eq!(open_ids(&ui, Model::Messages), [1]);
    assert_eq!(selected_ids(&ui, Model::Messages), Vec::<i64>::new());
    // Opening another row keeps the selection.
    ui.select(3, true);
    ui.set_open(Some(2));
    assert_eq!(selected_ids(&ui, Model::Messages), [3]);
}

#[test]
fn open_survives_a_page_append() {
    let mut ui = three();
    ui.set_open(Some(1));
    let _ = ui.more().unwrap();
    feed(
        &mut ui,
        page(F1, Some(cursor(1, 10)), vec![msg(0, 5)], true),
    );
    assert_eq!(ui.open_message(), Some(1));
    assert_eq!(open_ids(&ui, Model::Messages), [1]);
}

#[test]
fn open_clears_when_the_row_is_removed() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(3, 30), msg(2, 20), msg(1, 10)], true);
    ui.set_open(Some(1));
    feed(
        &mut ui,
        Event::MessagesChanged {
            folders: vec![FolderId(1)],
        },
    );
    feed(&mut ui, page(F1, None, vec![msg(3, 30), msg(2, 20)], true));
    assert_eq!(ui.open_message(), None);
    assert_eq!(open_ids(&ui, Model::Messages), Vec::<i64>::new());
}

#[test]
fn folder_change_clears_open() {
    let mut ui = three();
    ui.set_open(Some(2));
    let (_, outs) = ui.open_list(ListRef::Inboxes);
    assert_eq!(ui.open_message(), None);
    let ops = changes(&outs, Model::Messages);
    assert!(matches!(ops[..], [Op::Update { .. }]), "{ops:?}");
    assert_eq!(open_ids(&ui, Model::Messages), Vec::<i64>::new());
}

#[test]
fn search_rows_get_the_open_mark_too() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(2, 20), msg(1, 10)], true);
    let _ = ui.search(SearchScope::All, "q".into(), false);
    feed(
        &mut ui,
        Event::SearchResults {
            scope: SearchScope::All,
            query: "q".into(),
            server: false,
            rows: vec![msg(2, 20), msg(5, 50)],
        },
    );
    let outs = ui.set_open(Some(2));
    assert_eq!(changes(&outs, Model::Messages).len(), 1);
    assert_eq!(changes(&outs, Model::Search).len(), 1);
    assert_eq!(open_ids(&ui, Model::Search), [2]);
    // Moving to a row only Search shows unmarks Messages.
    let outs = ui.set_open(Some(5));
    assert_eq!(changes(&outs, Model::Messages).len(), 1);
    assert_eq!(open_ids(&ui, Model::Messages), Vec::<i64>::new());
    assert_eq!(open_ids(&ui, Model::Search), [5]);
}

#[test]
fn an_unknown_id_does_not_open() {
    let mut ui = three();
    ui.set_open(Some(1));
    ui.set_open(Some(99));
    assert_eq!(ui.open_message(), None);
    assert_eq!(open_ids(&ui, Model::Messages), Vec::<i64>::new());
}
