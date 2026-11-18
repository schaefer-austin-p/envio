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

#[test]
fn keep_bytes_writes_a_safe_numbered_copy_that_delete_removes() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(&dir.path().join("drafts")).unwrap();
    let id = drafts.create(&draft("x", 1)).unwrap();
    let a = drafts.keep_bytes(&id, "../report.pdf", b"one").unwrap();
    let b = drafts.keep_bytes(&id, "../report.pdf", b"two").unwrap();
    assert_eq!(a.file_name().unwrap(), "report.pdf");
    assert_eq!(b.file_name().unwrap(), "report (2).pdf");
    assert_eq!(a.parent().unwrap().file_name().unwrap(), id.as_str());
    assert_eq!(fs::read(&a).unwrap(), b"one");
    assert_eq!(fs::read(&b).unwrap(), b"two");
    drafts.delete(&id).unwrap();
    assert!(!a.exists());
}

#[test]
fn save_prunes_copies_the_draft_no_longer_names() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("x", 1)).unwrap();
    let kept = drafts.keep_bytes(&id, "kept.txt", b"k").unwrap();
    let dropped = drafts.keep_bytes(&id, "dropped.txt", b"d").unwrap();
    let mut with_both = draft("x", 2);
    with_both.attachments = vec![kept.clone(), dropped.clone()];
    drafts.save(&id, &with_both).unwrap();
    assert!(kept.exists() && dropped.exists());

    let mut with_one = draft("x", 3);
    with_one.attachments = vec![kept.clone()];
    drafts.save(&id, &with_one).unwrap();
    assert!(kept.exists());
    assert!(!dropped.exists());

    drafts.save(&id, &draft("x", 4)).unwrap();
    assert!(!kept.exists());
    assert!(!kept.parent().unwrap().exists());
}

#[test]
fn save_never_touches_a_path_outside_the_drafts_own_folder() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("x", 1)).unwrap();
    let other = drafts.create(&draft("y", 1)).unwrap();
    let picked = dir.path().join("picked.txt");
    fs::write(&picked, "mine").unwrap();
    let theirs = drafts.keep_bytes(&other, "theirs.txt", b"t").unwrap();
    let mut named = draft("x", 2);
    named.attachments = vec![picked.clone(), theirs.clone()];
    drafts.save(&id, &named).unwrap();
    drafts.save(&id, &draft("x", 3)).unwrap();
    assert!(picked.exists());
    assert!(theirs.exists());
}

#[test]
fn save_keeps_a_fresh_copy_the_draft_has_not_named_yet() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::open(dir.path()).unwrap();
    let id = drafts.create(&draft("x", 1)).unwrap();
    let fresh = drafts.keep_bytes(&id, "fresh.txt", b"f").unwrap();
    // An autosave that raced the attach does not name it yet.
    drafts.save(&id, &draft("x", 2)).unwrap();
    assert!(fresh.exists());
}
