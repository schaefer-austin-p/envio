//! Opening must not need to write: a full or read-only disk can't block startup.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use envio_store::{ActionKind, Journal, MessageRef, NewOutboxItem, Outbox, SendState};

fn set_read_only(dir: &Path, read_only: bool) {
    let mut permissions = fs::metadata(dir).unwrap().permissions();
    permissions.set_readonly(read_only);
    fs::set_permissions(dir, permissions).unwrap();
}

/// Makes `dir` read-only; returns false (after undoing it) when that has no
/// effect, as when running as root.
fn make_read_only(dir: &Path) -> bool {
    set_read_only(dir, true);
    let probe = dir.join("probe");
    if fs::write(&probe, b"x").is_ok() {
        fs::remove_file(&probe).unwrap();
        set_read_only(dir, false);
        return false;
    }
    true
}

fn target() -> MessageRef {
    MessageRef {
        account: "1".into(),
        folder: "INBOX".into(),
        uidvalidity: 1,
        uid: 1,
        message_id: None,
    }
}

fn item() -> NewOutboxItem {
    NewOutboxItem {
        send_after_ms: 0,
        account: "1".into(),
        message_id: "<a@example.org>".into(),
        queued_at_ms: 5,
        from: "ada@example.org".into(),
        recipients: vec!["bob@example.org".into()],
    }
}

#[test]
fn a_journal_with_a_torn_tail_opens_in_a_read_only_folder() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actions.journal");
    let mut journal = Journal::open(&path).unwrap();
    journal.push(1, target(), ActionKind::Delete).unwrap();
    drop(journal);
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(br#"{"op":"add","act"#)
        .unwrap();
    if !make_read_only(dir.path()) {
        return;
    }
    let result = Journal::open(&path);
    set_read_only(dir.path(), false);
    let journal = result.unwrap();
    assert_eq!(journal.pending().count(), 1);
}

#[test]
fn an_outbox_item_left_sending_shows_as_may_have_been_sent_when_the_disk_is_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = Outbox::open(dir.path()).unwrap();
    let queued = outbox.enqueue(item(), b"x").unwrap();
    outbox.set_state(&queued.id, SendState::Sending).unwrap();
    drop(outbox);
    if !make_read_only(dir.path()) {
        return;
    }
    let result = Outbox::open(dir.path());
    let items = result.as_ref().ok().map(|outbox| outbox.items().unwrap());
    set_read_only(dir.path(), false);
    let outbox = result.unwrap();
    assert_eq!(items.unwrap()[0].meta.state, SendState::MayHaveBeenSent);
    // Once writes work again, a later change replaces the in-memory answer.
    outbox
        .set_state(&queued.id, SendState::Failed { reason: "x".into() })
        .unwrap();
    assert_eq!(
        outbox.items().unwrap()[0].meta.state,
        SendState::Failed { reason: "x".into() }
    );
}
