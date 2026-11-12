//! Drafts are saved on their own thread: while the worker is stuck, across a
//! worker panic, and Send hands them over to the Outbox.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use envio_core::{Command, ComposeId, Core, DraftInput, Event, EventSink, StartFrom};
use envio_store::{Drafts, Outbox};
use support::Harness;

const ME: &str = "me@example.org";
const C: ComposeId = ComposeId(1);
const SAVE_MS: u64 = 1_000;
/// Time a save may take past its due time. Tests here run in parallel and
/// each save syncs a file, which on a CI runner has taken over 200 ms
/// (#80). A save held up by the 3 s stall or a copy still misses it.
const SLACK_MS: u64 = 800;

struct S {
    h: Harness,
    key: String,
}

fn started() -> S {
    let mut h = Harness::unstarted();
    let key = h.seed_account(ME, Some("pw"));
    h.start_core();
    // The worker is up before a test stalls or crashes it.
    h.wait(|e| matches!(e, Event::Started { .. }).then_some(()));
    S { h, key }
}

impl S {
    fn draft(&self, body: &str) -> DraftInput {
        DraftInput {
            account: self.key.clone(),
            to: vec!["ada@example.org".into()],
            subject: "Plans".into(),
            body: body.into(),
            ..DraftInput::default()
        }
    }

    fn drafts(&self) -> Vec<(String, envio_store::Draft)> {
        Drafts::open(&self.h.paths().drafts())
            .unwrap()
            .list()
            .unwrap()
    }

    fn files_dir(&self) -> PathBuf {
        self.h.paths().drafts().join("files")
    }

    /// The first stored draft `ok` accepts, within `limit`.
    fn wait_for_draft(&self, limit: Duration, ok: impl Fn(&envio_store::Draft) -> bool) -> bool {
        let deadline = Instant::now() + limit;
        loop {
            if self.drafts().iter().any(|(_, d)| ok(d)) {
                return true;
            }
            if Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn change(&self, draft: DraftInput) {
        self.h.send(Command::DraftChanged { compose: C, draft });
    }

    fn stall(&self, ms: u64) {
        self.h.send(Command::Stall { ms });
        // The worker has taken it by now.
        std::thread::sleep(Duration::from_millis(200));
    }

    fn saved(&self) -> String {
        self.h.wait(|e| match e {
            Event::DraftSaved { compose, draft_id } if *compose == C => Some(draft_id.clone()),
            _ => None,
        })
    }

    /// A new compose window, as the app opens one before anything is typed.
    fn open_compose(&self) {
        self.h.send(Command::StartCompose {
            compose: C,
            from: StartFrom::New {
                account: self.key.clone(),
            },
            draft_id: None,
            mailto: None,
        });
        self.h
            .wait(|e| matches!(e, Event::ComposeReady { .. }).then_some(()));
    }

    /// Attaches `source`; the path of the copy.
    fn attach(&self, source: &Path) -> PathBuf {
        self.open_compose();
        self.h.send(Command::Attach {
            compose: C,
            path: source.to_owned(),
        });
        self.h.wait(|e| match e {
            Event::Attached { compose, path, .. } if *compose == C => Some(path.clone()),
            _ => None,
        })
    }

    fn outbox(&self) -> Outbox {
        Outbox::open(&self.h.paths().outbox()).unwrap()
    }

    fn refused(&self) -> String {
        self.h.wait(|e| match e {
            Event::ComposeRefused { compose, text } if *compose == C => Some(text.clone()),
            _ => None,
        })
    }
}

fn source_file(s: &S, name: &str, bytes: &[u8]) -> PathBuf {
    let dir = s.h.dir.path().join("picked");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn file_count(dir: &Path) -> usize {
    std::fs::read_dir(dir).map_or(0, Iterator::count)
}

/// A named pipe: copying from it blocks until `release` writes, which is a
/// copy of any length the test wants.
fn slow_source(s: &S, name: &str) -> PathBuf {
    let dir = s.h.dir.path().join("picked");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    path
}

fn release(source: &Path, bytes: &[u8]) {
    use std::io::Write;
    std::fs::OpenOptions::new()
        .write(true)
        .open(source)
        .unwrap()
        .write_all(bytes)
        .unwrap();
}

#[test]
fn a_slow_attachment_copy_does_not_delay_saves_or_the_flush() {
    let s = started();
    s.open_compose();
    s.change(s.draft("typed"));
    let slow = slow_source(&s, "big.bin");
    s.h.send(Command::Attach {
        compose: C,
        path: slow.clone(),
    });
    // The copy is under way (it waits for the pipe's writer).
    std::thread::sleep(Duration::from_millis(300));
    // The copy stays blocked until `release` below, so order proves the
    // point: a save or flush that waited for the copy cannot land before it.
    s.change(s.draft("typed while copying"));
    let saved = s.wait_for_draft(Duration::from_secs(10), |d| d.body == "typed while copying");
    s.change(s.draft("typed again"));
    let core = s.h.core.as_ref().unwrap();
    let (done, flushed_rx) = std::sync::mpsc::channel();
    let flushed = std::thread::scope(|scope| {
        scope.spawn(move || {
            core.send(Command::AppActive(false));
            let _ = done.send(());
        });
        flushed_rx.recv_timeout(Duration::from_secs(10)).is_ok()
            && s.wait_for_draft(Duration::ZERO, |d| d.body == "typed again")
    });
    // Let the copy end whatever the result, so nothing is left blocked.
    release(&slow, b"payload");
    let copy = s.h.wait(|e| match e {
        Event::Attached { path, .. } => Some(path.clone()),
        _ => None,
    });
    assert!(saved, "a text save waited for the copy");
    assert!(flushed, "the flush waited for the copy");
    assert_eq!(std::fs::read(copy).unwrap(), b"payload");
}

#[test]
fn a_change_is_saved_while_the_worker_is_stalled() {
    let s = started();
    s.stall(3_000);
    s.change(s.draft("typed"));
    let limit = Duration::from_millis(SAVE_MS + SLACK_MS);
    assert!(s.wait_for_draft(limit, |d| d.body == "typed"));
}

#[test]
fn app_inactive_flushes_while_the_worker_is_stalled() {
    let s = started();
    s.stall(3_000);
    s.change(s.draft("before background"));
    s.h.send(Command::AppActive(false));
    // `send` waited for the flush: no more waiting here.
    assert!(s.wait_for_draft(Duration::ZERO, |d| d.body == "before background"));
}

#[test]
fn a_worker_panic_keeps_the_draft_id() {
    let s = started();
    s.change(s.draft("one"));
    let id = s.saved();
    s.h.send(Command::Crash);
    s.h.wait(|e| matches!(e, Event::WorkerRestarted { .. }).then_some(()));
    s.change(s.draft("two"));
    assert!(s.wait_for_draft(Duration::from_secs(10), |d| d.body == "two"));
    let drafts = s.drafts();
    assert_eq!(drafts.len(), 1, "no ghost or duplicate file");
    assert_eq!(drafts[0].0, id);
}

#[test]
fn send_deletes_the_draft_and_its_attachment_copies() {
    let s = started();
    let copy = s.attach(&source_file(&s, "note.txt", b"hello"));
    assert!(copy.starts_with(s.files_dir()));
    let mut draft = s.draft("with a file");
    draft.attachments = vec![copy.clone()];
    s.change(draft.clone());
    assert!(s.wait_for_draft(Duration::from_secs(10), |d| d.body == "with a file"));
    s.h.send(Command::Send { compose: C, draft });
    s.h.wait(|e| matches!(e, Event::Queued { .. }).then_some(()));
    assert!(s.drafts().is_empty());
    assert!(!copy.exists());
    assert_eq!(file_count(&s.files_dir()), 0);
    let items = s.outbox().items().unwrap();
    assert_eq!(items.len(), 1);
    let eml = s.outbox().eml(&items[0].id).unwrap();
    assert!(String::from_utf8_lossy(&eml).contains("note.txt"));
}

#[test]
fn a_failed_enqueue_keeps_the_draft_and_files() {
    let s = started();
    let copy = s.attach(&source_file(&s, "note.txt", b"hello"));
    let mut draft = s.draft("keep me");
    draft.attachments = vec![copy.clone()];
    s.change(draft.clone());
    assert!(s.wait_for_draft(Duration::from_secs(3), |d| d.body == "keep me"));
    // The Outbox folder is gone, so enqueueing fails.
    let outbox = s.h.paths().outbox();
    std::fs::remove_dir_all(&outbox).unwrap();
    std::fs::write(&outbox, b"in the way").unwrap();
    s.h.send(Command::Send { compose: C, draft });
    assert!(!s.refused().is_empty());
    assert_eq!(s.drafts().len(), 1);
    assert!(copy.exists());
}

#[test]
fn an_attachment_survives_its_source_being_deleted() {
    let s = started();
    let source = source_file(&s, "note.txt", b"still here");
    let copy = s.attach(&source);
    std::fs::remove_file(&source).unwrap();
    assert_eq!(std::fs::read(&copy).unwrap(), b"still here");
    let mut draft = s.draft("sent after the source went");
    draft.attachments = vec![copy];
    s.h.send(Command::Send { compose: C, draft });
    s.h.wait(|e| matches!(e, Event::Queued { .. }).then_some(()));
    let items = s.outbox().items().unwrap();
    assert_eq!(items.len(), 1);
}

#[test]
fn a_reopened_draft_saves_to_the_same_file() {
    let s = started();
    s.change(s.draft("first"));
    let id = s.saved();
    s.h.send(Command::CloseCompose {
        compose: C,
        discard: false,
    });
    let again = ComposeId(2);
    s.h.send(Command::StartCompose {
        compose: again,
        from: StartFrom::Draft,
        draft_id: Some(id.clone()),
        mailto: None,
    });
    s.h.wait(|e| {
        matches!(e, Event::ComposeReady { compose, .. } if *compose == again).then_some(())
    });
    // No draft id in the change: the book knows it from the reopening.
    s.h.send(Command::DraftChanged {
        compose: again,
        draft: s.draft("second"),
    });
    assert!(s.wait_for_draft(Duration::from_secs(3), |d| d.body == "second"));
    let drafts = s.drafts();
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].0, id);
}

#[test]
fn close_then_send_deletes_the_draft() {
    let s = started();
    s.stall(1_000);
    s.change(s.draft("typed"));
    // The close reaches the draft thread at once; the Send waits behind the
    // stalled worker.
    s.h.send(Command::CloseCompose {
        compose: C,
        discard: false,
    });
    s.h.send(Command::Send {
        compose: C,
        draft: s.draft("typed"),
    });
    s.h.wait(|e| matches!(e, Event::Queued { .. }).then_some(()));
    std::thread::sleep(Duration::from_millis(300));
    assert!(s.drafts().is_empty(), "the sent mail is not a draft");
}

/// Panics once, on the first `DraftSaved`.
struct PanicOnce(AtomicBool);

impl EventSink for PanicOnce {
    fn emit(&self, event: Event) {
        if matches!(event, Event::DraftSaved { .. }) && !self.0.swap(true, Ordering::SeqCst) {
            panic!("sink panic");
        }
    }
}

#[test]
fn a_panicking_sink_does_not_stop_draft_saving() {
    let mut h = Harness::unstarted();
    let key = h.seed_account(ME, Some("pw"));
    h.core = Some(Core::start(h.config(), Arc::new(PanicOnce(AtomicBool::new(false)))).unwrap());
    let s = S { h, key };
    s.change(s.draft("first"));
    assert!(s.wait_for_draft(Duration::from_secs(3), |d| d.body == "first"));
    std::thread::sleep(Duration::from_millis(100));
    s.h.send(Command::DraftChanged {
        compose: ComposeId(2),
        draft: s.draft("second"),
    });
    // This checks the thread survives, not the save's timing. The sink's
    // panic goes through std's panic hook, which takes the process-wide
    // backtrace lock; a_worker_panic_keeps_the_draft_id, running alongside,
    // resolves a worker backtrace under that lock, which took over 1.8 s on
    // CI's coverage build.
    let limit = Duration::from_secs(10);
    assert!(s.wait_for_draft(limit, |d| d.body == "second"));
}
