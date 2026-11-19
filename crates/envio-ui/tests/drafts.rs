mod support;

use envio_core::{Command, Event, FolderId, FolderRole, ListRef, MessageRow};
use envio_ui::{Model, Op, Output, Row, RowValue, Ui};
use support::*;

const DRAFTS: ListRef = ListRef::Folder(FolderId(3));

fn local(id: i64, date: i64, draft: &str) -> MessageRow {
    let mut r = msg_in(id, date, 3, "a");
    r.local_draft = Some(draft.into());
    r
}

fn with_folders(ui: &mut Ui) {
    feed(
        ui,
        Event::Folders {
            account: "a".into(),
            folders: vec![folder(1, FolderRole::Inbox), folder(3, FolderRole::Drafts)],
        },
    );
}

fn message_rows(outs: &[Output]) -> Vec<Row> {
    changes(outs, Model::Messages)
        .into_iter()
        .flat_map(|op| match op {
            Op::Reset(r) | Op::Insert { rows: r, .. } => r.clone(),
            Op::Update { row, .. } => vec![row.clone()],
            Op::Remove { .. } => vec![],
        })
        .collect()
}

fn lists(outs: &[Output]) -> Vec<Option<envio_core::Cursor>> {
    outs.iter()
        .filter_map(|o| match o {
            Output::Send(Command::ListMessages { after, .. }) => Some(*after),
            _ => None,
        })
        .collect()
}

fn draft_changed(ui: &mut Ui, account: &str) -> Vec<Output> {
    feed(
        ui,
        Event::LocalDraftsChanged {
            account: account.into(),
        },
    )
}

#[test]
fn a_local_draft_row_opens_compose_and_is_not_uploaded() {
    let mut ui = Ui::new();
    with_folders(&mut ui);
    let outs = opened(
        &mut ui,
        DRAFTS,
        vec![local(-1, 20, "d1"), msg_in(5, 10, 3, "a")],
        true,
    );
    let rows = message_rows(&outs);
    assert_eq!(rows[0].value("draftId"), Some(RowValue::Str("d1".into())));
    assert_eq!(rows[0].value("notUploaded"), Some(RowValue::Bool(true)));
    assert_eq!(
        rows[0].value("opens"),
        Some(RowValue::Str("compose".into()))
    );
    assert_eq!(rows[1].value("draftId"), Some(RowValue::Str(String::new())));
    assert_eq!(rows[1].value("notUploaded"), Some(RowValue::Bool(false)));
    assert_eq!(rows[1].value("opens"), Some(RowValue::Str("reader".into())));
}

#[test]
fn local_drafts_changed_refreshes_the_shown_drafts_list() {
    let mut ui = Ui::new();
    with_folders(&mut ui);
    opened(&mut ui, DRAFTS, vec![local(-1, 20, "d1")], true);
    let outs = draft_changed(&mut ui, "a");
    assert_eq!(lists(&outs), vec![None]);
}

#[test]
fn local_drafts_changed_does_nothing_elsewhere() {
    let mut ui = Ui::new();
    with_folders(&mut ui);
    opened(&mut ui, F1, vec![msg(1, 10)], true);
    assert!(draft_changed(&mut ui, "a").is_empty(), "Inbox shown");
    opened(&mut ui, DRAFTS, vec![local(-1, 20, "d1")], true);
    assert!(draft_changed(&mut ui, "b").is_empty(), "other account");
    let mut ui = Ui::new();
    opened(&mut ui, ListRef::Inboxes, vec![msg(1, 10)], true);
    assert!(draft_changed(&mut ui, "a").is_empty(), "unified inbox");
}

#[test]
fn list_is_drafts_follows_the_folder_role() {
    let mut ui = Ui::new();
    with_folders(&mut ui);
    assert!(!ui.list_is_drafts(), "nothing shown");
    let _ = ui.open_list(F1);
    assert!(!ui.list_is_drafts());
    let _ = ui.open_list(DRAFTS);
    assert!(ui.list_is_drafts());
    let _ = ui.open_list(ListRef::Inboxes);
    assert!(!ui.list_is_drafts());
}

#[test]
fn an_uploaded_draft_swaps_its_pseudo_row_for_the_server_row() {
    let mut ui = Ui::new();
    with_folders(&mut ui);
    opened(&mut ui, DRAFTS, vec![local(-1, 20, "d1")], true);
    draft_changed(&mut ui, "a");
    let outs = feed(
        &mut ui,
        page(DRAFTS, None, vec![msg_in(9, 20, 3, "a")], true),
    );
    let ops = changes(&outs, Model::Messages);
    assert!(
        ops.iter().any(|o| matches!(o, Op::Remove { .. })),
        "{ops:?}"
    );
    let inserted: Vec<i64> = ops
        .iter()
        .filter(|o| matches!(o, Op::Insert { .. }))
        .flat_map(|o| ids(o))
        .collect();
    assert_eq!(inserted, vec![9]);
    assert!(!ops.iter().any(|o| matches!(o, Op::Reset(_))));
    assert_eq!(ui.rows(Model::Messages).len(), 1);
}

#[test]
fn the_selection_knows_a_local_row_and_loses_it_on_upload() {
    let mut ui = Ui::new();
    with_folders(&mut ui);
    opened(
        &mut ui,
        DRAFTS,
        vec![local(-1, 20, "d1"), msg_in(5, 10, 3, "a")],
        true,
    );
    assert!(!ui.selection_has_local());
    ui.select(5, true);
    assert!(!ui.selection_has_local());
    ui.select(-1, true);
    assert!(ui.selection_has_local());
    draft_changed(&mut ui, "a");
    let outs = feed(
        &mut ui,
        page(DRAFTS, None, vec![msg_in(5, 10, 3, "a")], true),
    );
    assert!(!ui.selection_has_local(), "the pseudo-row left");
    assert!(
        !changes(&outs, Model::Messages)
            .iter()
            .any(|o| matches!(o, Op::Reset(_))),
        "the refresh merges, it does not reset"
    );
    let rows = ui.rows(Model::Messages);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value("id"), Some(RowValue::Int(5)));
    assert_eq!(rows[0].value("selected"), Some(RowValue::Bool(true)));
}

#[test]
fn an_empty_draft_id_is_a_server_row() {
    let mut ui = Ui::new();
    with_folders(&mut ui);
    let outs = opened(&mut ui, DRAFTS, vec![local(-1, 20, "")], true);
    let rows = message_rows(&outs);
    assert_eq!(rows[0].value("opens"), Some(RowValue::Str("reader".into())));
    assert_eq!(rows[0].value("notUploaded"), Some(RowValue::Bool(false)));
}
