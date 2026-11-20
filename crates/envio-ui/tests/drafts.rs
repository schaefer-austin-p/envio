mod support;

use envio_core::{Command, Event, FolderId, FolderRole, ListRef, MessageRow};
use envio_ui::{Model, Op, Output, Row, RowValue, Signal, Ui};
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

fn local_text(id: i64, draft: &str, text: &str) -> MessageRow {
    let mut r = local(id, 20, draft);
    r.not_uploaded_text = text.into();
    r
}

fn text(r: &Row) -> Option<RowValue> {
    r.value("notUploadedText")
}

#[test]
fn not_uploaded_text_is_a_role_with_a_per_row_value() {
    assert!(Row::role_names(Model::Messages).contains(&"notUploadedText"));
    let mut ui = Ui::new();
    with_folders(&mut ui);
    let mut other = local_text(-2, "d2", "Mailbox full");
    other.date = Some(15);
    let outs = opened(
        &mut ui,
        DRAFTS,
        vec![local_text(-1, "d1", "Not uploaded"), other],
        true,
    );
    let rows = message_rows(&outs);
    assert_eq!(text(&rows[0]), Some(RowValue::Str("Not uploaded".into())));
    assert_eq!(text(&rows[1]), Some(RowValue::Str("Mailbox full".into())));
}

#[test]
fn not_uploaded_text_is_empty_on_a_server_row() {
    let mut ui = Ui::new();
    with_folders(&mut ui);
    let mut server = msg_in(5, 10, 3, "a");
    server.not_uploaded_text = "stray".into();
    let outs = opened(&mut ui, DRAFTS, vec![server, local(-1, 20, "")], true);
    let rows = message_rows(&outs);
    for r in &rows {
        assert_eq!(r.value("notUploaded"), Some(RowValue::Bool(false)));
        assert_eq!(text(r), Some(RowValue::Str(String::new())));
    }
}

#[test]
fn a_changed_reason_updates_the_row_without_a_reset() {
    let mut ui = Ui::new();
    with_folders(&mut ui);
    opened(
        &mut ui,
        DRAFTS,
        vec![local_text(-1, "d1", "Not uploaded")],
        true,
    );
    draft_changed(&mut ui, "a");
    let outs = feed(
        &mut ui,
        page(
            DRAFTS,
            None,
            vec![local_text(-1, "d1", "Mailbox full")],
            true,
        ),
    );
    let ops = changes(&outs, Model::Messages);
    assert!(!ops.iter().any(|o| matches!(o, Op::Reset(_))), "{ops:?}");
    let updated: Vec<&Row> = ops
        .iter()
        .filter_map(|o| match o {
            Op::Update { row, .. } => Some(row),
            _ => None,
        })
        .collect();
    assert_eq!(updated.len(), 1, "{ops:?}");
    assert_eq!(text(updated[0]), Some(RowValue::Str("Mailbox full".into())));
}

// The local-only Drafts entry (#303): the core lists it as a Drafts-role
// folder with a negative id, and the UI treats it like any Drafts folder.
const LOCAL: ListRef = ListRef::Folder(FolderId(-2));

fn with_local_entry(ui: &mut Ui) -> Vec<Output> {
    let mut entry = folder(-2, FolderRole::Drafts);
    entry.path = "Drafts".into();
    entry.name = "Drafts".into();
    feed(
        ui,
        Event::Folders {
            account: "a".into(),
            folders: vec![folder(1, FolderRole::Inbox), entry],
        },
    )
}

#[test]
fn the_local_entry_is_an_ordinary_drafts_row_in_the_drawer() {
    let mut ui = Ui::new();
    let outs = with_local_entry(&mut ui);
    let rows: Vec<Row> = changes(&outs, Model::Folders)
        .into_iter()
        .flat_map(|op| match op {
            Op::Reset(r) => r.clone(),
            _ => vec![],
        })
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].value("role"), Some(RowValue::Str("drafts".into())));
    assert_eq!(rows[1].value("name"), Some(RowValue::Str("Drafts".into())));
    assert_eq!(rows[1].value("id"), Some(RowValue::Int(-2)));
}

#[test]
fn a_local_entry_draft_opens_compose_and_the_list_is_a_drafts_list() {
    let mut ui = Ui::new();
    with_local_entry(&mut ui);
    let mut row = msg_in(-1, 20, -2, "a");
    row.local_draft = Some("d1".into());
    let outs = opened(&mut ui, LOCAL, vec![row], true);
    let rows = message_rows(&outs);
    assert_eq!(rows[0].value("draftId"), Some(RowValue::Str("d1".into())));
    assert_eq!(
        rows[0].value("opens"),
        Some(RowValue::Str("compose".into()))
    );
    assert!(ui.list_is_drafts());
}

#[test]
fn local_drafts_changed_refreshes_the_local_entry_list() {
    let mut ui = Ui::new();
    with_local_entry(&mut ui);
    opened(&mut ui, LOCAL, vec![], true);
    assert_eq!(lists(&draft_changed(&mut ui, "a")), vec![None]);
}

#[test]
fn the_view_moves_to_the_real_drafts_folder_when_the_server_gains_one() {
    let mut ui = Ui::new();
    with_local_entry(&mut ui);
    opened(&mut ui, LOCAL, vec![], true);
    // The account's folders again: now with a real Drafts folder, no entry.
    let outs = feed(
        &mut ui,
        Event::Folders {
            account: "a".into(),
            folders: vec![folder(1, FolderRole::Inbox), folder(3, FolderRole::Drafts)],
        },
    );
    let asked: Vec<ListRef> = outs
        .iter()
        .filter_map(|o| match o {
            Output::Send(Command::ListMessages { list, .. }) => Some(*list),
            _ => None,
        })
        .collect();
    assert_eq!(asked, vec![DRAFTS]);
    assert!(ui.list_is_drafts());
    // The page is told, so its search scope follows (#303).
    assert!(
        outs.iter()
            .any(|o| matches!(o, Output::Signal(Signal::ListSwitched { folder: 3 }))),
        "{outs:?}"
    );
}

#[test]
fn the_view_stays_when_the_local_entry_is_still_listed() {
    let mut ui = Ui::new();
    with_local_entry(&mut ui);
    opened(&mut ui, LOCAL, vec![], true);
    let outs = with_local_entry(&mut ui);
    assert!(lists(&outs).is_empty());
}
