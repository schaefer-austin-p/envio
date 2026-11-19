//! A draft imported from a server message (#228 T2): its server copy is
//! written as given, survives an upload, and a damaged file that names the
//! message can be found.

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
