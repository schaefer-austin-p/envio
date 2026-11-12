mod support;

use envio_core::Event;
use envio_ui::{Model, Op, Row, RowValue, Ui};
use support::*;

#[test]
fn every_role_name_has_a_value() {
    let mut ui = Ui::new();
    let outs = opened(&mut ui, F1, vec![msg(1, 10)], true);
    let Op::Reset(rows) = changes(&outs, Model::Messages)[0] else {
        panic!()
    };
    for role in Row::role_names(Model::Messages) {
        assert!(rows[0].value(role).is_some(), "{role}");
    }
    assert_eq!(rows[0].value("id"), Some(RowValue::Int(1)));
    assert_eq!(
        rows[0].value("dateBucket"),
        Some(RowValue::Str("full".into()))
    );
    assert_eq!(rows[0].value("unread"), Some(RowValue::Bool(true)));
    assert_eq!(rows[0].value("nope"), None);
}

#[test]
fn folder_rows_carry_role_and_depth() {
    let mut ui = Ui::new();
    let mut f = folder(5, envio_core::FolderRole::Other);
    f.path = "Work/Clients".into();
    f.name = "Clients".into();
    let outs = feed(
        &mut ui,
        Event::Folders {
            account: "a".into(),
            folders: vec![f],
        },
    );
    let Op::Reset(rows) = changes(&outs, Model::Folders)[0] else {
        panic!()
    };
    assert_eq!(rows[0].value("depth"), Some(RowValue::Int(1)));
    assert_eq!(rows[0].value("role"), Some(RowValue::Str(String::new())));
}
