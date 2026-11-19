//! A draft's copy in the server's Drafts folder: what the draft file keeps
//! and the notes left for deleted drafts.

use std::fs;

use envio_store::{Draft, Drafts, RefusalKind, ServerCopy};

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

#[test]
fn a_draft_file_from_before_refusals_loads_with_no_reason() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("Old", 1)).unwrap();
    let old = r#"{"account":"1700","to":[],"subject":"Old","body":"x","updated_at_ms":1,
        "server":{"failed_ms":1,"failures":3}}"#;
    fs::write(dir.path().join(format!("{id}.json")), old).unwrap();
    let server = drafts.load(&id).unwrap().server;
    assert_eq!(server.refusal, None);
    assert_eq!(server.failed_at_ms, 0);
    assert_eq!(server.failures, 3);
}

#[test]
fn mark_failed_stores_the_kind_and_the_time() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("One", 5)).unwrap();
    assert!(
        drafts
            .mark_failed(&id, 5, 1, RefusalKind::OverQuota, 900)
            .unwrap()
    );
    let server = drafts.load(&id).unwrap().server;
    assert_eq!(server.refusal, Some(RefusalKind::OverQuota));
    assert_eq!(server.failed_at_ms, 900);
    assert_eq!((server.failed_ms, server.failures), (5, 1));
    // The same save fails again: the count grows, the kind is replaced.
    drafts
        .mark_failed(&id, 5, 1, RefusalKind::ReadOnly, 950)
        .unwrap();
    let server = drafts.load(&id).unwrap().server;
    assert_eq!(server.refusal, Some(RefusalKind::ReadOnly));
    assert_eq!(server.failed_at_ms, 950);
    assert_eq!(server.failures, 2);
    // A gone draft is reported.
    drafts.delete(&id).unwrap();
    assert!(
        !drafts
            .mark_failed(&id, 5, 1, RefusalKind::Other, 1)
            .unwrap()
    );
}

#[test]
fn a_new_save_resets_the_count_and_replaces_the_kind() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("One", 5)).unwrap();
    drafts
        .mark_failed(&id, 5, 1, RefusalKind::OverQuota, 900)
        .unwrap();
    drafts
        .mark_failed(&id, 5, 1, RefusalKind::OverQuota, 910)
        .unwrap();
    drafts
        .mark_failed(&id, 8, 1, RefusalKind::TooLarge, 1000)
        .unwrap();
    let server = drafts.load(&id).unwrap().server;
    assert_eq!((server.failed_ms, server.failures), (8, 1));
    assert_eq!(server.refusal, Some(RefusalKind::TooLarge));
    assert_eq!(server.failed_at_ms, 1000);
}

#[test]
fn an_upload_clears_the_reason_and_a_try_keeps_it() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("One", 5)).unwrap();
    drafts
        .mark_failed(&id, 5, 1, RefusalKind::OverQuota, 900)
        .unwrap();
    drafts.mark_tried(&id, 999).unwrap();
    drafts.mark_uploading(&id, "<p@x>").unwrap();
    let server = drafts.load(&id).unwrap().server;
    assert_eq!(server.refusal, Some(RefusalKind::OverQuota));
    assert_eq!(server.failed_at_ms, 900);
    assert!(drafts.mark_uploaded(&id, "<a@x>", 5).unwrap());
    let server = drafts.load(&id).unwrap().server;
    assert_eq!(server.refusal, None);
    assert_eq!(server.failed_at_ms, 0);
}

#[test]
fn clear_backoff_zeroes_the_time_of_one_account_only() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let mine = drafts.create(&draft("Mine", 5)).unwrap();
    let mut other = draft("Other", 6);
    other.account = "1800".into();
    let theirs = drafts.create(&other).unwrap();
    drafts
        .mark_failed(&mine, 5, 2, RefusalKind::OverQuota, 900)
        .unwrap();
    drafts
        .mark_failed(&theirs, 6, 1, RefusalKind::Other, 800)
        .unwrap();
    assert_eq!(drafts.clear_backoff("1700").unwrap(), 1);
    let server = drafts.load(&mine).unwrap().server;
    assert_eq!(server.failed_at_ms, 0);
    assert_eq!(server.failures, 2);
    assert_eq!(server.refusal, Some(RefusalKind::OverQuota));
    let server = drafts.load(&theirs).unwrap().server;
    assert_eq!(server.failed_at_ms, 800);
    // Nothing to clear the second time.
    assert_eq!(drafts.clear_backoff("1700").unwrap(), 0);
}
