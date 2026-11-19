mod support;

use envio_core::{Command, MessageRowId, UserAction};
use envio_ui::{Model, Row, Ui};
use support::*;

fn selected_ids(ui: &Ui) -> Vec<i64> {
    let mut ids: Vec<i64> = ui
        .rows(Model::Messages)
        .iter()
        .filter_map(|r| match r {
            Row::Message(m) if m.selected => Some(m.id),
            _ => None,
        })
        .collect();
    ids.sort();
    ids
}

fn five() -> Ui {
    let mut ui = Ui::new();
    let rows = (1..=5).rev().map(|i| msg(i, i * 10)).collect();
    opened(&mut ui, F1, rows, false);
    ui
}

#[test]
fn act_on_leaves_the_selection_alone() {
    let mut ui = five();
    ui.select(2, true);
    ui.select(3, true);
    let (cmd, outs) = ui.act_on(vec![5], UserAction::Archive).unwrap();
    assert!(
        matches!(&cmd, Command::Act { ids, action: UserAction::Archive } if *ids == [MessageRowId(5)]),
        "{cmd:?}"
    );
    assert!(changes(&outs, Model::Messages).is_empty());
    assert_eq!(selected_ids(&ui), [2, 3]);
    // The set is still live: acting on the selection reaches rows 2 and 3.
    let (cmd, _) = ui.act_on_selection(UserAction::Delete).unwrap();
    assert!(
        matches!(&cmd, Command::Act { ids, .. } if *ids == [MessageRowId(2), MessageRowId(3)]),
        "{cmd:?}"
    );
}

#[test]
fn act_on_unknown_id_is_none() {
    let mut ui = five();
    assert!(ui.act_on(vec![99], UserAction::Archive).is_none());
    assert!(ui.act_on(vec![], UserAction::Archive).is_none());
}

#[test]
fn act_on_open_message_clears_open() {
    let mut ui = five();
    ui.set_open(Some(4));
    let (_, outs) = ui.act_on(vec![4], UserAction::Archive).unwrap();
    assert_eq!(ui.open_message(), None);
    assert_eq!(changes(&outs, Model::Messages).len(), 1);
    // Acting on another message keeps the open one.
    ui.set_open(Some(1));
    let (_, outs) = ui.act_on(vec![4], UserAction::Archive).unwrap();
    assert_eq!(ui.open_message(), Some(1));
    assert!(outs.is_empty());
}

#[test]
fn act_on_selection_still_clears_selection() {
    let mut ui = five();
    ui.select(2, true);
    let (_, outs) = ui.act_on_selection(UserAction::Archive).unwrap();
    assert_eq!(changes(&outs, Model::Messages).len(), 1);
    assert_eq!(selected_ids(&ui), Vec::<i64>::new());
}
