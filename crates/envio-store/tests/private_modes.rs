//! Unsent mail and cached bodies are for the user alone: files 0600, folders
//! the store creates 0700.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use envio_store::{ActionKind, Bodies, Db, FolderId, Journal, MessageRef, Outbox};

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn files_under(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            found.push(path);
        }
    }
    found
}

#[test]
fn the_cache_folder_is_private() {
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("cache");
    drop(Db::open(&cache.join("envio.db")).unwrap());
    assert_eq!(mode(&cache), 0o700);
}

#[test]
fn journal_and_its_folder_are_private() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data").join("actions.journal");
    let mut journal = Journal::open(&path).unwrap();
    let target = MessageRef {
        account: "a".into(),
        folder: "INBOX".into(),
        uid: 1,
        message_id: None,
        uidvalidity: 1,
    };
    journal.push(1, target, ActionKind::Delete).unwrap();
    assert_eq!(mode(&path), 0o600);
    assert_eq!(mode(path.parent().unwrap()), 0o700);
}

#[test]
fn outbox_folder_is_private() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("outbox");
    drop(Outbox::open(&root).unwrap());
    assert_eq!(mode(&root), 0o700);
}

#[test]
fn cached_bodies_are_private() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("bodies");
    let bodies = Bodies::new(&root);
    bodies
        .write(FolderId(1), 1, b"Subject: x\r\n\r\nbody")
        .unwrap();
    assert_eq!(mode(&root), 0o700);
    let files = files_under(&root);
    assert!(!files.is_empty());
    for file in files {
        assert_eq!(mode(&file), 0o600, "{file:?}");
    }
}
