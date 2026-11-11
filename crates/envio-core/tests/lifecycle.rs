//! Starting, restarting after a panic, and stopping the worker.

mod support;

use std::time::{Duration, Instant};

use envio_core::{AccountInfo, AccountStatus, CacheState, Command, Event};
use envio_store::{Bodies, FolderId};
use support::Harness;

fn started(event: &Event) -> Option<(Vec<AccountInfo>, CacheState)> {
    match event {
        Event::Started { accounts, cache } => Some((accounts.clone(), *cache)),
        _ => None,
    }
}

#[test]
fn start_reports_accounts_and_a_fresh_cache() {
    let mut h = Harness::unstarted();
    let key = h.seed_account("me@example.org", Some("pw"));
    h.start_core();
    let (accounts, cache) = h.wait(started);
    assert_eq!(cache, CacheState::Fresh);
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].key, key);
    assert_eq!(accounts[0].email, "me@example.org");
    assert_eq!(accounts[0].status, AccountStatus::Ready);
}

#[test]
fn a_second_start_reports_an_existing_cache() {
    let mut h = Harness::start();
    h.wait(started);
    h.restart();
    let (_, cache) = h.wait(started);
    assert_eq!(cache, CacheState::Existing);
}

#[test]
fn a_damaged_cache_is_rebuilt_at_start() {
    let mut h = Harness::unstarted();
    std::fs::create_dir_all(h.cache_dir()).unwrap();
    std::fs::write(h.paths().db(), b"this is not a database at all").unwrap();
    h.start_core();
    // The notice comes just before Started.
    h.wait(|e| matches!(e, Event::Notice { .. }).then_some(()));
    let (_, cache) = h.wait(started);
    assert_eq!(cache, CacheState::Rebuilt);
}

#[test]
fn a_panic_restarts_the_worker_and_writes_a_crash_log() {
    let h = Harness::start();
    h.wait(started);
    h.send(Command::Crash);
    h.wait(|e| matches!(e, Event::WorkerRestarted { .. }).then_some(()));
    h.wait(started);
    let crashes = std::fs::read_dir(h.cache_dir().join("logs"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("crash-"))
        .count();
    assert_eq!(crashes, 1);
}

#[test]
fn shutdown_joins_the_worker() {
    let mut h = Harness::start();
    h.wait(started);
    let core = h.core.take().unwrap();
    let began = Instant::now();
    core.shutdown();
    assert!(began.elapsed() < Duration::from_secs(2));
}

#[test]
fn orphan_bodies_are_swept_at_start() {
    let mut h = Harness::unstarted();
    std::fs::create_dir_all(h.cache_dir()).unwrap();
    let bodies = Bodies::new(h.paths().bodies());
    bodies.write(FolderId(7), 3, b"orphan").unwrap();
    h.start_core();
    h.wait(started);
    assert!(bodies.read(FolderId(7), 3).is_err());
}

#[test]
fn shutdown_while_the_store_cannot_open_returns() {
    let mut h = Harness::unstarted();
    std::fs::write(h.data_dir(), b"a file where the data dir should be").unwrap();
    h.start_core();
    std::thread::sleep(Duration::from_millis(200));
    let core = h.core.take().unwrap();
    let (done, finished) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        core.shutdown();
        let _ = done.send(());
    });
    finished
        .recv_timeout(Duration::from_secs(2))
        .expect("shutdown did not return");
}

#[test]
fn a_command_sent_while_the_store_cannot_open_runs_after_it_opens() {
    let mut h = Harness::unstarted();
    std::fs::write(h.data_dir(), b"a file where the data dir should be").unwrap();
    h.start_core();
    h.send(Command::Crash);
    std::thread::sleep(Duration::from_millis(200));
    std::fs::remove_file(h.data_dir()).unwrap();
    h.wait(|e| matches!(e, Event::WorkerRestarted { .. }).then_some(()));
}
