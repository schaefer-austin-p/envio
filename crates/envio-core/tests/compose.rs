//! Compose: drafts saved within a second, Send into the Outbox.

mod support;

use std::time::{Duration, Instant};

use envio_core::{Command, ComposeId, DraftInput, Event, FolderRole, MessageRowId, StartFrom};
use envio_store::{Drafts, Outbox};
use support::Harness;

const ME: &str = "me@example.org";
const C: ComposeId = ComposeId(1);

/// A message with one attachment whose name tries to leave its folder.
const WITH_FILE: &[u8] = b"From: Ada <ada@example.org>\r\nTo: me@example.org\r\n\
Subject: report\r\nMessage-ID: <m2@example.org>\r\nMIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=XX\r\n\r\n\
--XX\r\nContent-Type: text/plain\r\n\r\nsee file\r\n\
--XX\r\nContent-Type: application/pdf; name=\"../evil.pdf\"\r\n\
Content-Disposition: attachment; filename=\"../evil.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\r\naGVsbG8=\r\n--XX--\r\n";

const FLOWED: &[u8] = b"From: Ada <ada@example.org>\r\nTo: me@example.org\r\n\
Subject: hi\r\nMessage-ID: <m1@example.org>\r\nMIME-Version: 1.0\r\n\
Content-Type: text/plain; charset=utf-8; format=flowed\r\n\r\n\
hello \r\nworld again\r\n";

struct Seeded {
    h: Harness,
    key: String,
}

fn seeded() -> Seeded {
    seeded_with(false)
}

/// `missing`: the message's body is not in the cache (and the server is
/// unreachable), so a Reply has to wait for a download that fails.
fn seeded_with(missing: bool) -> Seeded {
    seeded_raw(missing, FLOWED)
}

fn seeded_raw(missing: bool, raw: &[u8]) -> Seeded {
    let mut h = Harness::unstarted();
    h.foreground = missing;
    let key = h.seed_account(ME, Some("pw"));
    let folder = h.seed_folder_with(&key, "INBOX", FolderRole::Inbox, &[(1, raw)]);
    if missing {
        let db = envio_store::Db::open(&h.paths().db()).unwrap();
        let rows = db
            .list(envio_store::ListRef::Folder(folder), None, 10)
            .unwrap();
        db.set_body_state(rows[0].id, envio_store::BodyState::Missing)
            .unwrap();
    }
    h.start_core();
    Seeded { h, key }
}

impl Seeded {
    fn draft(&self, to: &[&str]) -> DraftInput {
        DraftInput {
            account: self.key.clone(),
            to: to.iter().map(|s| (*s).to_owned()).collect(),
            subject: "Plans".into(),
            body: "See you".into(),
            ..DraftInput::default()
        }
    }

    fn drafts(&self) -> Vec<(String, envio_store::Draft)> {
        Drafts::open(&self.h.paths().drafts())
            .unwrap()
            .list()
            .unwrap()
    }

    fn outbox(&self) -> Vec<envio_store::OutboxItem> {
        Outbox::open(&self.h.paths().outbox())
            .unwrap()
            .items()
            .unwrap()
    }

    /// The first stored draft that `ok` accepts, within 5 s.
    fn wait_for_draft(&self, ok: impl Fn(&envio_store::Draft) -> bool) -> envio_store::Draft {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some((_, d)) = self.drafts().into_iter().find(|(_, d)| ok(d)) {
                return d;
            }
            assert!(Instant::now() < deadline, "no such draft");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Everything sent so far has been handled.
    fn settle(&self) {
        self.h.send(Command::ListFolders {
            account: self.key.clone(),
        });
        self.h
            .wait(|e| matches!(e, Event::Folders { .. }).then_some(()));
    }

    fn saved(&self) -> String {
        self.h.wait(|e| match e {
            Event::DraftSaved { compose, draft_id } if *compose == C => Some(draft_id.clone()),
            _ => None,
        })
    }

    fn refused(&self) -> String {
        self.h.wait(|e| match e {
            Event::ComposeRefused { compose, text } if *compose == C => Some(text.clone()),
            _ => None,
        })
    }

    fn change(&self, draft: DraftInput) {
        self.h.send(Command::DraftChanged { compose: C, draft });
    }

    fn send(&self, draft: DraftInput) {
        self.h.send(Command::Send { compose: C, draft });
    }

    fn queued(&self) -> String {
        self.h.wait(|e| match e {
            Event::Queued { compose, item } if *compose == C => Some(item.clone()),
            _ => None,
        })
    }

    fn message_id(&self) -> MessageRowId {
        self.h.send(Command::ListMessages {
            list: envio_core::ListRef::Folder(envio_core::FolderId(1)),
            after: None,
        });
        self.h.wait(|e| match e {
            Event::Messages { rows, .. } => rows.first().map(|r| r.id),
            _ => None,
        })
    }
}

#[test]
fn a_change_is_saved_within_a_second() {
    let s = seeded();
    let started = Instant::now();
    s.change(s.draft(&["ada@example.org"]));
    let id = s.saved();
    assert!(started.elapsed() < Duration::from_millis(1_800));
    let drafts = s.drafts();
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].0, id);
    assert_eq!(drafts[0].1.subject, "Plans");
}

#[test]
fn send_with_no_recipients_is_refused_and_keeps_the_draft() {
    let s = seeded();
    s.change(s.draft(&[]));
    s.saved();
    s.send(s.draft(&[]));
    assert!(!s.refused().is_empty());
    assert!(s.outbox().is_empty());
    assert_eq!(s.drafts().len(), 1);
    // What the Send carried is kept too, even if no change came before it.
    let mut newer = s.draft(&[]);
    newer.body = "typed just before Send".into();
    s.send(newer);
    s.refused();
    let stored = s.wait_for_draft(|d| d.body == "typed just before Send");
    assert_eq!(stored.subject, "Plans");
}

#[test]
fn send_queues_in_the_outbox_then_deletes_the_draft() {
    let s = seeded();
    let mut d = s.draft(&["ada@example.org"]);
    d.bcc = vec!["hidden@example.org".into()];
    s.change(d.clone());
    let id = s.saved();
    d.draft_id = Some(id);
    s.send(d);
    let item = s.queued();
    let items = s.outbox();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, item);
    assert_eq!(items[0].meta.from, ME);
    assert!(
        items[0]
            .meta
            .recipients
            .contains(&"hidden@example.org".to_owned())
    );
    assert!(s.drafts().is_empty());
}

#[test]
fn an_unreadable_attachment_is_refused() {
    let s = seeded();
    let mut d = s.draft(&["ada@example.org"]);
    d.attachments = vec![s.h.data_dir().join("missing.bin")];
    s.send(d);
    assert!(s.refused().contains("missing.bin"));
    assert!(s.outbox().is_empty());
}

#[test]
fn reply_quotes_flowed_text_unflowed() {
    let s = seeded();
    let id = s.message_id();
    s.h.send(Command::StartCompose {
        compose: C,
        from: StartFrom::Reply { id, all: false },
        draft_id: None,
        mailto: None,
    });
    let (draft, quoted) = s.h.wait(|e| match e {
        Event::ComposeReady {
            compose,
            draft,
            quoted,
        } if *compose == C => Some((draft.clone(), quoted.clone())),
        _ => None,
    });
    assert!(quoted.contains("> hello world again"), "{quoted}");
    assert_eq!(draft.to, vec!["Ada <ada@example.org>".to_owned()]);
    assert_eq!(draft.subject, "Re: hi");
    assert_eq!(draft.in_reply_to.as_deref(), Some("<m1@example.org>"));
}

#[test]
fn forward_with_attachments_attaches_them_with_safe_names() {
    let s = seeded_raw(false, WITH_FILE);
    let id = s.message_id();
    s.h.send(Command::StartCompose {
        compose: C,
        from: StartFrom::Forward {
            id,
            attachments: true,
        },
        draft_id: None,
        mailto: None,
    });
    let (name, path, size) = s.h.wait(|e| match e {
        Event::Attached {
            compose,
            name,
            path,
            size,
        } if *compose == C => Some((name.clone(), path.clone(), *size)),
        _ => None,
    });
    assert_eq!(name, "evil.pdf");
    assert_eq!(size, 5);
    assert_eq!(std::fs::read(&path).unwrap(), b"hello");
    assert!(path.starts_with(s.h.paths().draft_files()), "{path:?}");
}

#[test]
fn forward_without_attachments_attaches_nothing() {
    let s = seeded_raw(false, WITH_FILE);
    let id = s.message_id();
    s.h.send(Command::StartCompose {
        compose: C,
        from: StartFrom::Forward {
            id,
            attachments: false,
        },
        draft_id: None,
        mailto: None,
    });
    s.h.wait(|e| matches!(e, Event::ComposeReady { compose, .. } if *compose == C).then_some(()));
    s.settle();
    // Nothing was copied into the drafts' files.
    let files = s.h.paths().draft_files();
    assert!(!files.exists() || std::fs::read_dir(files).unwrap().next().is_none());
}

#[test]
fn shutdown_saves_unsaved_drafts() {
    let mut s = seeded();
    s.change(s.draft(&["ada@example.org"]));
    // Closed before the second is up.
    s.h.send(Command::Shutdown);
    s.h.restart();
    assert_eq!(s.drafts().len(), 1);
}

#[test]
fn discard_deletes_the_draft() {
    let s = seeded();
    s.change(s.draft(&["ada@example.org"]));
    s.saved();
    assert_eq!(s.drafts().len(), 1);
    s.change(s.draft(&["bob@example.org"]));
    s.h.send(Command::CloseCompose {
        compose: C,
        discard: true,
    });
    // Neither the stored draft nor the pending change survives.
    std::thread::sleep(Duration::from_millis(1_300));
    assert!(s.drafts().is_empty());
}

#[test]
fn a_queued_message_id_is_searchable() {
    let s = seeded();
    s.send(s.draft(&["ada@example.org"]));
    s.queued();
    let items = s.outbox();
    assert!(envio_sync::plan::message_id::usable_message_id(
        &items[0].meta.message_id
    ));
}

#[test]
fn a_reopened_draft_keeps_its_attachments() {
    let s = seeded();
    let mut d = s.draft(&["ada@example.org"]);
    d.attachments = vec!["/tmp/keep-me.txt".into()];
    s.change(d);
    let id = s.saved();
    s.h.send(Command::StartCompose {
        compose: ComposeId(2),
        from: StartFrom::Draft,
        draft_id: Some(id),
        mailto: None,
    });
    let draft = s.h.wait(|e| match e {
        Event::ComposeReady { draft, .. } => Some(draft.clone()),
        _ => None,
    });
    assert_eq!(
        draft.attachments,
        vec![std::path::PathBuf::from("/tmp/keep-me.txt")]
    );
}

/// Whether a file can still be written to `dir` (root ignores the mode).
fn writable_anyway(dir: &std::path::Path) -> bool {
    let probe = dir.join("probe");
    let ok = std::fs::write(&probe, b"x").is_ok();
    if ok {
        std::fs::remove_file(&probe).unwrap();
    }
    ok
}

#[test]
fn a_failed_save_stays_pending_and_is_retried() {
    use std::os::unix::fs::PermissionsExt;
    let s = seeded();
    let dir = s.h.paths().drafts();
    Drafts::open(&dir).unwrap();
    let mode = |m| std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(m)).unwrap();
    mode(0o555);
    if writable_anyway(&dir) {
        mode(0o755);
        eprintln!("skipped: running as root, read-only has no effect");
        return;
    }
    s.change(s.draft(&["ada@example.org"]));
    // The failed save tells the user once.
    s.h.wait(|e| matches!(e, Event::Notice { .. }).then_some(()));
    let more_notices = std::cell::Cell::new(0);
    std::thread::sleep(Duration::from_millis(1_300));
    mode(0o755);
    s.h.wait(|e| match e {
        Event::Notice { .. } => {
            more_notices.set(more_notices.get() + 1);
            None
        }
        Event::DraftSaved { .. } => Some(()),
        _ => None,
    });
    assert_eq!(more_notices.get(), 0, "one notice per run of failures");
    assert_eq!(s.drafts().len(), 1);
}

#[test]
fn a_failed_save_is_still_saved_when_the_window_closes() {
    use std::os::unix::fs::PermissionsExt;
    let s = seeded();
    let dir = s.h.paths().drafts();
    Drafts::open(&dir).unwrap();
    let mode = |m| std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(m)).unwrap();
    mode(0o555);
    if writable_anyway(&dir) {
        mode(0o755);
        eprintln!("skipped: running as root, read-only has no effect");
        return;
    }
    s.change(s.draft(&["ada@example.org"]));
    s.h.wait(|e| matches!(e, Event::Notice { .. }).then_some(()));
    mode(0o755);
    s.h.send(Command::CloseCompose {
        compose: C,
        discard: false,
    });
    s.saved();
    assert_eq!(s.drafts().len(), 1);
}

#[test]
fn a_second_send_queues_nothing_more() {
    let s = seeded();
    let d = s.draft(&["ada@example.org"]);
    s.send(d.clone());
    s.queued();
    s.send(d);
    s.settle();
    assert_eq!(s.outbox().len(), 1);
}

#[test]
fn a_change_after_send_does_not_bring_the_draft_back() {
    let s = seeded();
    let d = s.draft(&["ada@example.org"]);
    s.send(d.clone());
    s.queued();
    s.change(d);
    std::thread::sleep(Duration::from_millis(1_300));
    assert!(s.drafts().is_empty());
}

#[test]
fn send_drops_the_pending_save() {
    let s = seeded();
    let d = s.draft(&["ada@example.org"]);
    s.change(d.clone());
    s.send(d);
    s.queued();
    std::thread::sleep(Duration::from_millis(1_300));
    assert!(s.drafts().is_empty());
}

#[test]
fn a_change_for_a_removed_account_is_not_saved() {
    let s = seeded();
    s.h.send(Command::RemoveAccount {
        account: s.key.clone(),
        confirmed: true,
    });
    s.h.wait(|e| matches!(e, Event::AccountRemoved { .. }).then_some(()));
    s.change(s.draft(&["ada@example.org"]));
    std::thread::sleep(Duration::from_millis(1_300));
    assert!(s.drafts().is_empty());
}

fn reply(id: MessageRowId) -> Command {
    Command::StartCompose {
        compose: C,
        from: StartFrom::Reply { id, all: false },
        draft_id: None,
        mailto: None,
    }
}

#[test]
fn a_reply_waiting_for_a_body_is_refused_when_it_cannot_be_had() {
    let s = seeded_with(true);
    let id = s.message_id();
    s.h.send(reply(id));
    assert!(!s.refused().is_empty());
}

#[test]
fn removing_the_account_answers_a_waiting_reply() {
    let s = seeded_with(true);
    let id = s.message_id();
    s.h.send(reply(id));
    s.h.send(Command::RemoveAccount {
        account: s.key.clone(),
        confirmed: true,
    });
    assert!(!s.refused().is_empty());
}

#[test]
fn a_change_after_send_and_close_does_not_bring_the_draft_back() {
    let s = seeded();
    let d = s.draft(&["ada@example.org"]);
    s.send(d.clone());
    s.queued();
    s.h.send(Command::CloseCompose {
        compose: C,
        discard: false,
    });
    s.change(d);
    std::thread::sleep(Duration::from_millis(1_300));
    assert!(s.drafts().is_empty());
}
