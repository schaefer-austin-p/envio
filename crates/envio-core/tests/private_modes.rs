//! Mail on disk is for the user alone: every folder and file the core
//! creates is private from the first start, and an older install is
//! tightened at the next one.

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use envio_core::Event;
use support::Harness;

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

/// Every path under `root` (not `root` itself).
fn walk(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(root).unwrap().flatten() {
        let path = entry.path();
        found.push(path.clone());
        if path.is_dir() {
            found.extend(walk(&path));
        }
    }
    found
}

fn assert_private_tree(root: &Path) {
    assert_eq!(mode(root), 0o700, "{root:?}");
    for path in walk(root) {
        let want = if path.is_dir() { 0o700 } else { 0o600 };
        assert_eq!(mode(&path), want, "{path:?}");
    }
}

fn started(event: &Event) -> Option<()> {
    matches!(event, Event::Started { .. }).then_some(())
}

#[test]
fn a_fresh_install_is_private_from_the_roots_down() {
    let mut h = Harness::unstarted();
    h.seed_account("me@example.org", Some("pw"));
    h.start_core();
    h.wait(started);
    // The database is open now: its -wal and -shm exist beside it.
    let cache = h.cache_dir();
    for name in ["envio.db", "envio.db-wal", "envio.db-shm"] {
        assert!(cache.join(name).exists(), "{name}");
    }
    assert!(cache.join("logs").is_dir());
    assert!(h.data_dir().join("drafts").is_dir());
    assert_private_tree(&cache);
    assert_private_tree(&h.data_dir());
}

#[test]
fn an_older_install_is_tightened_at_start() {
    let mut h = Harness::unstarted();
    h.seed_account("me@example.org", Some("pw"));
    h.start_core();
    h.wait(started);
    drop(h.core.take());
    for root in [h.cache_dir(), h.data_dir()] {
        // An older install has no marker: the first start migrates it.
        assert!(root.join(".private-v1").exists(), "{root:?}");
        fs::remove_file(root.join(".private-v1")).unwrap();
        set_mode(&root, 0o755);
        for path in walk(&root) {
            set_mode(&path, if path.is_dir() { 0o755 } else { 0o644 });
        }
    }
    h.start_core();
    h.wait(started);
    assert_private_tree(&h.cache_dir());
    assert_private_tree(&h.data_dir());
}

#[test]
fn the_migration_runs_once() {
    let mut h = Harness::unstarted();
    h.seed_account("me@example.org", Some("pw"));
    h.start_core();
    h.wait(started);
    drop(h.core.take());
    // New files are created private, so a later start does not walk the
    // tree again: a file loosened after the marker was written stays loose.
    let file = h.data_dir().join("accounts.json");
    set_mode(&file, 0o644);
    h.start_core();
    h.wait(started);
    assert_eq!(mode(&file), 0o644);
}
