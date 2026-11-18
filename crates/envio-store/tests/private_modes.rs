//! Unsent mail and cached bodies are for the user alone: files 0600, folders
//! the store creates 0700.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use envio_store::{ActionKind, Bodies, Db, Draft, Drafts, FolderId, Journal, MessageRef, Outbox};

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

#[test]
fn the_database_and_its_wal_files_are_private() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache").join("envio.db");
    let db = Db::open(&path).unwrap();
    for name in ["envio.db", "envio.db-wal", "envio.db-shm"] {
        let file = path.with_file_name(name);
        assert!(file.exists(), "{name}");
        assert_eq!(mode(&file), 0o600, "{name}");
    }
    drop(db);
}

#[test]
fn an_existing_database_is_narrowed_on_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("envio.db");
    drop(Db::open(&path).unwrap());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let db = Db::open(&path).unwrap();
    assert_eq!(mode(&path), 0o600);
    drop(db);
}

#[test]
fn drafts_and_their_attachment_copies_are_private() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("drafts");
    let drafts = Drafts::open(&root).unwrap();
    assert_eq!(mode(&root), 0o700);
    let source = dir.path().join("report.pdf");
    fs::write(&source, b"secret").unwrap();
    let id = drafts.create(&Draft::default()).unwrap();
    let copy = drafts.keep_attachment(&id, &source).unwrap();
    assert_eq!(mode(&copy), 0o600);
    assert_eq!(mode(copy.parent().unwrap()), 0o700);
}

#[test]
fn tighten_tree_narrows_what_is_too_open() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    let sub = root.join("sub");
    fs::create_dir_all(&sub).unwrap();
    let file = sub.join("a.eml");
    fs::write(&file, b"x").unwrap();
    let closed = root.join("keep");
    fs::write(&closed, b"x").unwrap();
    for (path, m) in [
        (&root, 0o755),
        (&sub, 0o755),
        (&file, 0o644),
        (&closed, 0o400),
    ] {
        fs::set_permissions(path, fs::Permissions::from_mode(m)).unwrap();
    }
    let outside = dir.path().join("outside");
    fs::write(&outside, b"x").unwrap();
    fs::set_permissions(&outside, fs::Permissions::from_mode(0o644)).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
    envio_store::tighten_tree(&root).unwrap();
    assert_eq!(mode(&root), 0o700);
    assert_eq!(mode(&sub), 0o700);
    assert_eq!(mode(&file), 0o600);
    assert_eq!(mode(&closed), 0o400, "already private: left alone");
    assert_eq!(mode(&outside), 0o644, "links are not followed");
    envio_store::tighten_tree(&dir.path().join("missing")).unwrap();
}
