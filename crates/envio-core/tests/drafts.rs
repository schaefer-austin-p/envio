//! Drafts are saved on their own thread: while the worker is stuck, across a
//! worker panic, and Send hands them over to the Outbox.

mod support;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use envio_core::{Command, ComposeId, DraftInput, Event, StartFrom};
use envio_store::{Drafts, Outbox};
use support::Harness;

const ME: &str = "me@example.org";
const C: ComposeId = ComposeId(1);
const SAVE_MS: u64 = 1_000;

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

#[test]
fn a_change_is_saved_while_the_worker_is_stalled() {
    let s = started();
    s.stall(3_000);
    let start = Instant::now();
    s.change(s.draft("typed"));
    let limit = Duration::from_millis(SAVE_MS + 200);
    assert!(s.wait_for_draft(limit, |d| d.body == "typed"));
    assert!(start.elapsed() < limit + Duration::from_millis(100));
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
    assert!(s.wait_for_draft(Duration::from_secs(3), |d| d.body == "two"));
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
    assert!(s.wait_for_draft(Duration::from_secs(3), |d| d.body == "with a file"));
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
