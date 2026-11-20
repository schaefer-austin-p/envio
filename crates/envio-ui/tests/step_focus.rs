mod support;

use envio_core::{Event, FolderId, ListRef};
use envio_ui::{Model, Row, Ui};
use support::*;

/// The ids of the rows a model shows with the keyboard-focus mark.
fn focus_ids(ui: &Ui) -> Vec<i64> {
    ui.rows(Model::Messages)
        .iter()
        .filter_map(|r| match r {
            Row::Message(m) if m.kbd_focus => Some(m.id),
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
fn step_down_focuses_next_row() {
    let mut ui = three();
    ui.step_focus(1);
    let outs = ui.step_focus(1);
    assert_eq!(ui.focus_message(), Some(2));
    assert_eq!(focus_ids(&ui), [2]);
    // One update unmarks row 3, one marks row 2.
    assert_eq!(changes(&outs, Model::Messages).len(), 2);
    ui.step_focus(-1);
    assert_eq!(ui.focus_message(), Some(3));
}

#[test]
fn step_stops_at_the_ends() {
    let mut ui = three();
    ui.step_focus(1);
    let outs = ui.step_focus(-1);
    assert_eq!(ui.focus_message(), Some(3));
    assert!(outs.is_empty());
    ui.step_focus(5);
    assert_eq!(ui.focus_message(), Some(1));
    assert!(ui.step_focus(1).is_empty());
    assert_eq!(focus_ids(&ui), [1]);
}

#[test]
fn step_from_none_focuses_first() {
    let mut ui = three();
    assert_eq!(ui.focus_message(), None);
    ui.step_focus(-1);
    assert_eq!(focus_ids(&ui), [3]);
    let mut ui = three();
    ui.step_focus(1);
    assert_eq!(focus_ids(&ui), [3]);
}

#[test]
fn focus_is_independent_of_open_and_selected() {
    let mut ui = three();
    ui.set_open(Some(1));
    ui.select(2, true);
    ui.step_focus(1);
    ui.step_focus(1);
    assert_eq!(ui.focus_message(), Some(2));
    assert_eq!(ui.open_message(), Some(1));
    ui.set_open(Some(3));
    ui.clear_selection();
    assert_eq!(ui.focus_message(), Some(2));
    assert_eq!(focus_ids(&ui), [2]);
}

#[test]
fn focus_clears_on_folder_change() {
    let mut ui = three();
    ui.step_focus(1);
    let (_, outs) = ui.open_list(ListRef::Inboxes);
    assert_eq!(ui.focus_message(), None);
    assert_eq!(changes(&outs, Model::Messages).len(), 1);
    assert_eq!(focus_ids(&ui), Vec::<i64>::new());
}

#[test]
fn step_under_paging_keeps_order() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(3, 30), msg(2, 20)], false);
    ui.step_focus(1);
    ui.step_focus(1);
    let _ = ui.more().unwrap();
    feed(
        &mut ui,
        page(F1, Some(cursor(2, 20)), vec![msg(1, 10), msg(0, 5)], true),
    );
    assert_eq!(ui.focus_message(), Some(2));
    ui.step_focus(1);
    assert_eq!(ui.focus_message(), Some(1));
    ui.step_focus(1);
    assert_eq!(focus_ids(&ui), [0]);
}

#[test]
fn focus_clears_when_the_row_is_removed() {
    let mut ui = Ui::new();
    opened(&mut ui, F1, vec![msg(3, 30), msg(2, 20), msg(1, 10)], true);
    ui.step_focus(1);
    ui.step_focus(5);
    assert_eq!(ui.focus_message(), Some(1));
    feed(
        &mut ui,
        Event::MessagesChanged {
            folders: vec![FolderId(1)],
        },
    );
    feed(&mut ui, page(F1, None, vec![msg(3, 30), msg(2, 20)], true));
    assert_eq!(ui.focus_message(), None);
}
