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
