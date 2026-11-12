use std::fs;

use envio_store::{Draft, Drafts, StoreError};

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
fn a_draft_round_trips_and_save_replaces_it() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("First", 1)).unwrap();
    assert_eq!(drafts.load(&id).unwrap(), draft("First", 1));
    drafts.save(&id, &draft("Second", 2)).unwrap();
    assert_eq!(
        Drafts::open(dir.path()).unwrap().load(&id).unwrap(),
        draft("Second", 2)
    );
}

#[test]
fn list_is_newest_first() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let old = drafts.create(&draft("old", 1)).unwrap();
    let new = drafts.create(&draft("new", 1)).unwrap();
    drafts.save(&new, &draft("new", 5)).unwrap();
    let ids: Vec<String> = drafts
        .list()
        .unwrap()
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(ids, [new, old]);
}

#[test]
fn delete_clears_the_files_folder_even_when_the_draft_file_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("x", 1)).unwrap();
    let source = dir.path().join("a.txt");
    std::fs::write(&source, "data").unwrap();
    let copy = drafts.keep_attachment(&id, &source).unwrap();
    // A partial failure: the draft file went, its copies stayed.
    std::fs::remove_file(dir.path().join(format!("{id}.json"))).unwrap();
    assert!(copy.exists());
    assert!(matches!(drafts.delete(&id), Err(StoreError::NotFound)));
    assert!(!copy.exists());
    assert!(!copy.parent().unwrap().exists());
}

#[test]
fn delete_removes_a_draft() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("x", 1)).unwrap();
    drafts.delete(&id).unwrap();
    assert!(drafts.list().unwrap().is_empty());
    assert!(matches!(drafts.load(&id), Err(StoreError::NotFound)));
    assert!(matches!(drafts.delete(&id), Err(StoreError::NotFound)));
}

#[test]
fn an_interrupted_save_keeps_the_last_complete_draft() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("Saved", 1)).unwrap();
    fs::write(dir.path().join(format!(".{id}.json.tmp")), b"{\"subj").unwrap();
    assert_eq!(drafts.list().unwrap().len(), 1);
    let reopened = Drafts::open(dir.path()).unwrap();
    assert_eq!(reopened.load(&id).unwrap().subject, "Saved");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn drafts_from_other_versions_still_load() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("5.json"),
        br#"{"subject": "Old", "future_field": 1}"#,
    )
    .unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let loaded = drafts.load("5").unwrap();
    assert_eq!(loaded.subject, "Old");
    assert!(loaded.to.is_empty());
}

#[test]
fn a_damaged_draft_is_a_bad_record() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("5.json"), b"not json").unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    assert!(matches!(
        drafts.load("5"),
        Err(StoreError::BadRecord { .. })
    ));
}

#[test]
fn ids_that_are_not_keys_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    for bad in ["../x", "", "a"] {
        assert!(
            matches!(drafts.load(bad), Err(StoreError::NotFound)),
            "{bad}"
        );
        assert!(matches!(
            drafts.save(bad, &Draft::default()),
            Err(StoreError::NotFound)
        ));
        assert!(matches!(drafts.delete(bad), Err(StoreError::NotFound)));
    }
}

#[test]
fn a_damaged_draft_is_skipped_reported_and_kept() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("ok", 1)).unwrap();
    fs::write(dir.path().join("77.json"), b"garbage").unwrap();
    let ids: Vec<String> = drafts
        .list()
        .unwrap()
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(ids, [id]);
    assert_eq!(drafts.damaged().unwrap(), ["77"]);
    assert!(dir.path().join("77.json").exists());
}

#[test]
fn attachments_round_trip_through_a_draft() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let with = Draft {
        attachments: vec!["/home/u/a.pdf".into(), "/home/u/b.png".into()],
        ..draft("Files", 1)
    };
    let id = drafts.create(&with).unwrap();
    assert_eq!(drafts.load(&id).unwrap(), with);
}

#[test]
fn a_draft_without_attachments_field_loads_with_none() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    fs::write(dir.path().join("9.json"), br#"{"subject":"old"}"#).unwrap();
    let loaded = drafts.load("9").unwrap();
    assert!(loaded.attachments.is_empty());
    assert_eq!(loaded.subject, "old");
}

#[test]
fn a_non_utf8_attachment_path_fails_the_save_without_panicking() {
    use std::os::unix::ffi::OsStrExt;
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let mut bad = draft("Odd name", 1);
    bad.attachments
        .push(std::path::PathBuf::from(std::ffi::OsStr::from_bytes(
            b"/tmp/\xff.txt",
        )));
    assert!(matches!(
        drafts.create(&bad),
        Err(StoreError::BadRecord { .. })
    ));
    let id = drafts.create(&draft("Fine", 2)).unwrap();
    assert!(drafts.save(&id, &bad).is_err());
    assert_eq!(drafts.load(&id).unwrap(), draft("Fine", 2));
}

fn source(dir: &std::path::Path, name: &str, content: &[u8]) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, content).unwrap();
    path
}

#[test]
fn an_attachment_is_copied_into_the_draft_folder() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(&dir.path().join("drafts")).unwrap();
    let original = source(dir.path(), "report.pdf", b"pdf bytes");
    let copy = drafts.keep_attachment("77", &original).unwrap();
    assert_eq!(
        copy,
        dir.path()
            .join("drafts")
            .join("files")
            .join("77")
            .join("report.pdf")
    );
    assert_eq!(fs::read(&copy).unwrap(), b"pdf bytes");
    assert!(original.exists(), "copied, not moved");
}

#[test]
fn name_collisions_get_a_number() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(&dir.path().join("drafts")).unwrap();
    let a = source(dir.path(), "report.pdf", b"1");
    let names: Vec<_> = (0..3)
        .map(|_| drafts.keep_attachment("77", &a).unwrap())
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["report.pdf", "report (2).pdf", "report (3).pdf"]);
    let bare = source(dir.path(), "notes", b"n");
    drafts.keep_attachment("77", &bare).unwrap();
    let second = drafts.keep_attachment("77", &bare).unwrap();
    assert_eq!(second.file_name().unwrap(), "notes (2)");
}

#[test]
fn attachment_names_never_leave_the_draft_folder() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(&dir.path().join("drafts")).unwrap();
    let hidden = source(dir.path(), "..sneaky", b"x");
    let copy = drafts.keep_attachment("77", &hidden).unwrap();
    assert_eq!(copy.file_name().unwrap(), "sneaky");
    let folder = dir.path().join("drafts").join("files").join("77");
    assert_eq!(copy.parent().unwrap(), folder);
    let backslash = source(dir.path(), "a\\..\\b.txt", b"x");
    let copy = drafts.keep_attachment("77", &backslash).unwrap();
    assert_eq!(copy.file_name().unwrap(), "b.txt");
    assert!(matches!(
        drafts.keep_attachment("../x", &hidden),
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        drafts.keep_attachment("77", &dir.path().join("missing")),
        Err(StoreError::NotFound)
    ));
}

#[test]
fn deleting_a_draft_deletes_its_files() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(&dir.path().join("drafts")).unwrap();
    let id = drafts.create(&draft("with file", 1)).unwrap();
    let copy = drafts
        .keep_attachment(&id, &source(dir.path(), "a.txt", b"a"))
        .unwrap();
    drafts.delete(&id).unwrap();
    assert!(!copy.exists());
    assert!(!copy.parent().unwrap().exists());
}

#[test]
fn sweep_removes_files_of_drafts_that_are_gone() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(&dir.path().join("drafts")).unwrap();
    assert_eq!(drafts.sweep_files().unwrap(), 0);
    let live = drafts.create(&draft("live", 1)).unwrap();
    let a = source(dir.path(), "a.txt", b"a");
    let kept = drafts.keep_attachment(&live, &a).unwrap();
    let orphan = drafts.keep_attachment("9999", &a).unwrap();
    assert_eq!(drafts.sweep_files().unwrap(), 1);
    assert!(kept.exists());
    assert!(!orphan.exists());
    assert_eq!(drafts.list().unwrap().len(), 1);
}
