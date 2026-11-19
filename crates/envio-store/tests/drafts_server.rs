//! A draft's copy in the server's Drafts folder: what the draft file keeps
//! and the notes left for deleted drafts.

use std::fs;

use envio_store::{Draft, Drafts, ServerCopy};

fn draft(subject: &str, updated_at_ms: i64) -> Draft {
    Draft {
        account: "1700".into(),
        to: vec!["bob@example.org".into()],
        subject: subject.into(),
        body: "Hi Bob".into(),
        updated_at_ms,
        ..Draft::default()
    }
}

#[test]
fn a_draft_file_from_before_server_copies_still_loads() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("Old", 1)).unwrap();
    let old = r#"{"account":"1700","to":[],"subject":"Old","body":"x","updated_at_ms":1}"#;
    fs::write(dir.path().join(format!("{id}.json")), old).unwrap();
    let loaded = drafts.load(&id).unwrap();
    assert_eq!(loaded.subject, "Old");
    assert_eq!(loaded.server, ServerCopy::default());
}

#[test]
fn saving_keeps_what_names_the_server_copy() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("One", 1)).unwrap();
    assert!(drafts.mark_uploaded(&id, "<a@x>", 1).unwrap());
    // The draft thread saves a draft that knows nothing of the server.
    drafts.save(&id, &draft("Two", 2)).unwrap();
    let loaded = drafts.load(&id).unwrap();
    assert_eq!(loaded.subject, "Two");
    assert_eq!(loaded.server.message_id.as_deref(), Some("<a@x>"));
    assert_eq!(loaded.server.uploaded_ms, 1);
}

#[test]
fn saving_over_a_damaged_file_keeps_the_server_copy_it_still_names() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("One", 1)).unwrap();
    // The text is damaged (a wrong type), the server field is whole.
    let damaged =
        r#"{"account":"1700","to":"oops","server":{"message_id":"<a@x>","uploaded_ms":4}}"#;
    fs::write(dir.path().join(format!("{id}.json")), damaged).unwrap();
    assert!(drafts.load(&id).is_err());
    drafts.save(&id, &draft("Two", 5)).unwrap();
    let loaded = drafts.load(&id).unwrap();
    assert_eq!(loaded.subject, "Two");
    assert_eq!(loaded.server.message_id.as_deref(), Some("<a@x>"));
    // Nothing to salvage: the save still succeeds.
    fs::write(dir.path().join(format!("{id}.json")), "not json").unwrap();
    drafts.save(&id, &draft("Three", 6)).unwrap();
    assert_eq!(drafts.load(&id).unwrap().server, ServerCopy::default());
}

#[test]
fn deleting_a_damaged_draft_still_leaves_a_note_for_its_server_copy() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("One", 1)).unwrap();
    let damaged =
        r#"{"account":"1700","to":"oops","server":{"message_id":"<a@x>","pending_id":"<b@x>"}}"#;
    fs::write(dir.path().join(format!("{id}.json")), damaged).unwrap();
    assert!(drafts.load(&id).is_err());
    drafts.delete(&id).unwrap();
    let notes = drafts.deletes().unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].account, "1700");
    assert_eq!(notes[0].message_ids, ["<a@x>", "<b@x>"]);
    // Nothing to salvage: the delete still succeeds, with no note.
    let other = drafts.create(&draft("Two", 2)).unwrap();
    fs::write(dir.path().join(format!("{other}.json")), "not json").unwrap();
    drafts.delete(&other).unwrap();
    assert_eq!(drafts.deletes().unwrap().len(), 1);
}

#[test]
fn marking_a_deleted_draft_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("One", 1)).unwrap();
    drafts.delete(&id).unwrap();
    assert!(!drafts.mark_uploaded(&id, "<a@x>", 1).unwrap());
    assert!(drafts.list().unwrap().is_empty());
}

#[test]
fn deleting_a_draft_with_a_server_copy_leaves_a_note_until_it_is_gone_there() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("One", 1)).unwrap();
    drafts.mark_uploaded(&id, "<a@x>", 1).unwrap();
    drafts.mark_uploading(&id, "<b@x>").unwrap();
    drafts.delete(&id).unwrap();
    let notes = Drafts::open(dir.path()).unwrap().deletes().unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].id, id);
    assert_eq!(notes[0].account, "1700");
    assert_eq!(notes[0].message_ids, ["<a@x>", "<b@x>"]);
    drafts.deleted_on_server(&id).unwrap();
    assert!(drafts.deletes().unwrap().is_empty());
}

#[test]
fn deleting_a_draft_never_uploaded_leaves_no_note() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("One", 1)).unwrap();
    drafts.delete(&id).unwrap();
    assert!(drafts.deletes().unwrap().is_empty());
}

#[test]
fn imported_from_loads_as_none_from_an_old_file_and_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("Old", 1)).unwrap();
    let old = r#"{"account":"1700","server":{"message_id":"<a@x>","uploaded_ms":1}}"#;
    fs::write(dir.path().join(format!("{id}.json")), old).unwrap();
    assert_eq!(drafts.load(&id).unwrap().server.imported_from, None);

    // `create` starts a draft with no server copy, so the file is written.
    let id = drafts.create(&draft("Copy", 2)).unwrap();
    let mut copy = draft("Copy", 2);
    copy.server.imported_from = Some("<o@x>".into());
    fs::write(
        dir.path().join(format!("{id}.json")),
        serde_json::to_string(&copy).unwrap(),
    )
    .unwrap();
    let loaded = drafts.load(&id).unwrap();
    assert_eq!(loaded.server.imported_from.as_deref(), Some("<o@x>"));
    // A save of the text keeps it (the draft thread knows nothing of it).
    drafts.save(&id, &draft("Copy 2", 3)).unwrap();
    assert_eq!(
        drafts.load(&id).unwrap().server.imported_from.as_deref(),
        Some("<o@x>")
    );
}

#[test]
fn forgetting_an_uploaded_draft_removes_it_and_leaves_no_note() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("One", 1)).unwrap();
    drafts.mark_uploaded(&id, "<a@x>", 1).unwrap();
    assert!(drafts.forget_if_uploaded(&id, "<a@x>").unwrap());
    assert!(drafts.load(&id).is_err());
    assert!(drafts.deletes().unwrap().is_empty());
    assert!(!drafts.forget_if_uploaded(&id, "<a@x>").unwrap(), "gone");
}

#[test]
fn forgetting_rechecks_the_message_id_and_that_nothing_was_saved_since() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("One", 1)).unwrap();
    drafts.mark_uploaded(&id, "<a@x>", 1).unwrap();
    assert!(!drafts.forget_if_uploaded(&id, "<b@x>").unwrap());
    // A save after the upload (maybe after the caller's look): kept.
    drafts.save(&id, &draft("Two", 2)).unwrap();
    assert!(!drafts.forget_if_uploaded(&id, "<a@x>").unwrap());
    assert_eq!(drafts.load(&id).unwrap().subject, "Two");
}
