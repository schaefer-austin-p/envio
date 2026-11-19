//! A draft imported from a server message (#228 T2): its server copy is
//! written as given, survives an upload, and a damaged file that names the
//! message can be found.

use std::fs;

use envio_store::{Draft, Drafts, EARLIER_MAX, ServerCopy, StoreError};

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
fn mark_uploaded_keeps_imported_from_and_clears_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let mut copy = draft("Copy", 7);
    copy.server = ServerCopy {
        pending_id: Some("<p@x>".into()),
        failed_ms: 6,
        failures: 2,
        tried_ms: 5,
        imported_from: Some("<o@x>".into()),
        ..ServerCopy::default()
    };
    let id = drafts.create_imported(&copy).unwrap();
    assert!(drafts.mark_uploaded(&id, "<c2@x>", 7).unwrap());
    let server = drafts.load(&id).unwrap().server;
    assert_eq!(server.imported_from.as_deref(), Some("<o@x>"));
    assert_eq!(server.message_id.as_deref(), Some("<c2@x>"));
    assert_eq!(server.uploaded_ms, 7);
    assert_eq!(server.pending_id, None);
    assert_eq!(
        (server.failed_ms, server.failures, server.tried_ms),
        (0, 0, 0)
    );
}

#[test]
fn create_imported_writes_the_given_server_copy_and_save_keeps_it() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let mut copy = draft("Copy", 4);
    copy.server = ServerCopy {
        message_id: Some("<o@x>".into()),
        uploaded_ms: 4,
        imported_from: Some("<o@x>".into()),
        ..ServerCopy::default()
    };
    let id = drafts.create_imported(&copy).unwrap();
    assert_eq!(drafts.load(&id).unwrap().server, copy.server);
    drafts.save(&id, &draft("Copy", 4)).unwrap();
    assert_eq!(drafts.load(&id).unwrap().server, copy.server);
}

#[test]
fn a_damaged_file_that_names_a_message_is_found_by_it() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("Hurt", 1)).unwrap();
    // `to` has the wrong type, so the whole file fails to parse, but
    // `account` and `server` can still be read on their own.
    let hurt = r#"{"account":"1700","to":5,
        "server":{"message_id":"<a@x>","imported_from":"<o@x>"}}"#;
    fs::write(dir.path().join(format!("{id}.json")), hurt).unwrap();
    assert!(drafts.load(&id).is_err());
    let found = |account, message_id| drafts.damaged_naming(account, message_id).unwrap();
    assert_eq!(found("1700", "<a@x>"), Some(id.clone()));
    assert_eq!(found("1700", " <o@x> "), Some(id));
    assert_eq!(found("other", "<a@x>"), None);
    assert_eq!(found("1700", "<z@x>"), None);
}

#[test]
fn undoing_an_import_leaves_no_note_that_would_delete_the_original() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let mut copy = draft("Copy", 4);
    copy.server = ServerCopy {
        message_id: Some("<o@x>".into()),
        uploaded_ms: 4,
        imported_from: Some("<o@x>".into()),
        ..ServerCopy::default()
    };
    let id = drafts.create_imported(&copy).unwrap();
    drafts.remove_imported(&id).unwrap();
    assert!(drafts.load(&id).is_err());
    assert!(drafts.deletes().unwrap().is_empty());

    // `delete` is for a draft the user threw away: it does note the server
    // copy, which is the original here.
    let id = drafts.create_imported(&copy).unwrap();
    drafts.delete(&id).unwrap();
    assert_eq!(drafts.deletes().unwrap().len(), 1);
}

const ORIGIN: &str = "<o@x>";

fn plain_copy(updated_at_ms: i64) -> Draft {
    let mut copy = draft("Copy", updated_at_ms);
    copy.server = ServerCopy {
        imported_from: Some(ORIGIN.into()),
        ..ServerCopy::default()
    };
    copy
}

#[test]
fn uploads_remember_the_earlier_message_ids_up_to_a_bound() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("D", 1)).unwrap();
    for n in 0..12 {
        assert!(drafts.mark_uploaded(&id, &format!("<u{n}@x>"), 2).unwrap());
    }
    // Uploading under the id it already has adds nothing.
    drafts.mark_uploaded(&id, "<u11@x>", 3).unwrap();
    let server = drafts.load(&id).unwrap().server;
    assert_eq!(server.message_id.as_deref(), Some("<u11@x>"));
    assert_eq!(server.earlier.len(), EARLIER_MAX);
    assert_eq!(server.earlier.last().map(String::as_str), Some("<u10@x>"));
    assert!(server.names(" <u10@x> "), "any recent one, trimmed");
    assert!(server.names("<u3@x>"));
    assert!(!server.names("<u2@x>"), "the oldest fell off the bound");
    assert!(!server.earlier.iter().any(|e| e == "<u11@x>"));
}

#[test]
fn detached_is_stored_at_import_and_survives_any_number_of_uploads() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let (copy, linked) = drafts
        .import(&plain_copy(7), None, |_, _, _| Ok(()))
        .unwrap();
    assert!(!linked);
    assert!(drafts.load(&copy).unwrap().server.is_detached_copy());
    let (own, linked) = drafts
        .import(&plain_copy(8), Some(ORIGIN), |_, _, _| Ok(()))
        .unwrap();
    assert!(linked);
    assert!(!drafts.load(&own).unwrap().server.is_detached_copy());
    // More uploads than the bounded list holds: the original's id has
    // left `earlier`, and neither answer changes.
    for n in 0..(EARLIER_MAX + 4) {
        for id in [&copy, &own] {
            drafts.mark_uploaded(id, &format!("<u{n}@x>"), 9).unwrap();
        }
    }
    let copy = drafts.load(&copy).unwrap().server;
    let own = drafts.load(&own).unwrap().server;
    assert!(!own.earlier.iter().any(|e| e == ORIGIN));
    assert!(copy.is_detached_copy());
    assert!(!own.is_detached_copy());
    assert!(!ServerCopy::default().is_detached_copy());
}

#[test]
fn an_import_is_incomplete_until_its_files_are_in() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let mut halfway = None;
    let (id, _) = drafts
        .import(&plain_copy(7), Some(ORIGIN), |drafts, id, _| {
            halfway = Some(drafts.load(id).unwrap().server.incomplete);
            Ok(())
        })
        .unwrap();
    assert_eq!(halfway, Some(true));
    assert!(!drafts.load(&id).unwrap().server.incomplete);
    // Also when the link is not made.
    let (id, linked) = drafts
        .import(&plain_copy(8), None, |_, _, _| Ok(()))
        .unwrap();
    assert!(!linked);
    assert!(!drafts.load(&id).unwrap().server.incomplete);
}

#[test]
fn an_import_is_linked_only_after_its_files_are_in() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let mut seen = None;
    let (id, linked) = drafts
        .import(&plain_copy(7), Some(ORIGIN), |drafts, id, draft| {
            // Halfway: a draft on disk, not yet standing in for the original.
            seen = Some(drafts.load(id).unwrap().server);
            draft
                .attachments
                .push(drafts.keep_bytes(id, "a.txt", b"hello")?);
            Ok(())
        })
        .unwrap();
    let mid = seen.unwrap();
    assert_eq!(mid.message_id, None, "not linked while files are added");
    assert_eq!(mid.uploaded_ms, 0);
    assert!(linked);
    let done = drafts.load(&id).unwrap();
    assert_eq!(done.attachments.len(), 1);
    assert_eq!(fs::read(&done.attachments[0]).unwrap(), b"hello");
    assert_eq!(done.server.message_id.as_deref(), Some(ORIGIN));
    assert_eq!(done.server.uploaded_ms, done.updated_at_ms);
    assert_eq!(done.server.imported_from.as_deref(), Some(ORIGIN));
}

#[test]
fn an_import_that_fails_in_the_middle_leaves_nothing_and_no_note() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let failed = drafts.import(&plain_copy(7), Some(ORIGIN), |drafts, id, draft| {
        draft
            .attachments
            .push(drafts.keep_bytes(id, "a.txt", b"one")?);
        Err(StoreError::NotFound)
    });
    assert!(failed.is_err());
    assert!(drafts.list().unwrap().is_empty());
    assert!(drafts.damaged().unwrap().is_empty());
    // A note here would expunge the original.
    assert!(drafts.deletes().unwrap().is_empty());
}

#[test]
fn an_import_touched_by_an_upload_before_the_link_stays_unlinked() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let (id, linked) = drafts
        .import(&plain_copy(7), Some(ORIGIN), |drafts, id, _| {
            drafts.mark_uploaded(id, "<c2@x>", 7)?;
            Ok(())
        })
        .unwrap();
    assert!(!linked);
    let server = drafts.load(&id).unwrap().server;
    assert_eq!(server.message_id.as_deref(), Some("<c2@x>"));
    assert_eq!(server.imported_from.as_deref(), Some(ORIGIN));
}

#[test]
fn an_import_without_a_link_stays_a_plain_copy() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let (id, linked) = drafts
        .import(&plain_copy(7), None, |_, _, _| Ok(()))
        .unwrap();
    assert!(!linked);
    let mut expected = plain_copy(7).server;
    expected.detached = true;
    assert_eq!(drafts.load(&id).unwrap().server, expected);
}

#[test]
fn a_change_stamped_in_the_millisecond_of_the_stored_draft_is_still_newer() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create_imported(&draft("Copy", 500)).unwrap();
    // What the import link (or an upload) records: covered up to the stamp.
    assert!(drafts.mark_uploaded(&id, "<o@x>", 500).unwrap());

    let mut edited = draft("Copy", 500);
    edited.body = "Changed".into();
    drafts.save(&id, &edited).unwrap();
    let saved = drafts.load(&id).unwrap();
    assert!(
        saved.updated_at_ms > saved.server.uploaded_ms,
        "a real edit must not look uploaded: {} vs {}",
        saved.updated_at_ms,
        saved.server.uploaded_ms
    );

    // The same content again changes nothing.
    drafts.save(&id, &edited).unwrap();
    assert_eq!(drafts.load(&id).unwrap().updated_at_ms, saved.updated_at_ms);
}
