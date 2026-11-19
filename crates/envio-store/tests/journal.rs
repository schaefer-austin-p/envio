use std::fs::{self, OpenOptions};
use std::io::Write;

use envio_store::{ActionKind, Flags, Journal, MessageRef, SkipReason, StoreError};

fn target(uid: u32) -> MessageRef {
    MessageRef {
        account: "1700".into(),
        folder: "INBOX".into(),
        uidvalidity: 9,
        uid,
        message_id: Some(format!("<{uid}@example.org>")),
    }
}

fn seqs(journal: &Journal) -> Vec<u64> {
    journal.pending().map(|action| action.seq).collect()
}

#[test]
fn a_new_journal_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let journal = Journal::open(&dir.path().join("actions.journal")).unwrap();
    assert!(seqs(&journal).is_empty());
}

#[test]
fn pending_actions_survive_reopening_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actions.journal");
    let mut journal = Journal::open(&path).unwrap();
    let flag = ActionKind::SetFlags {
        add: Flags::SEEN,
        remove: Flags::empty(),
    };
    let first = journal.push(10, target(1), flag.clone()).unwrap();
    let second = journal
        .push(
            20,
            target(2),
            ActionKind::Move {
                to: "Archive".into(),
            },
        )
        .unwrap();
    assert_eq!((first.seq, second.seq), (1, 2));
    drop(journal);

    let journal = Journal::open(&path).unwrap();
    let pending: Vec<_> = journal.pending().cloned().collect();
    assert_eq!(pending, [first, second]);
    assert_eq!(pending[0].kind, flag);
    assert_eq!(pending[0].target, target(1));
}

#[test]
fn done_actions_are_gone_after_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actions.journal");
    let mut journal = Journal::open(&path).unwrap();
    journal.push(1, target(1), ActionKind::Delete).unwrap();
    journal.push(2, target(2), ActionKind::Delete).unwrap();
    journal.done(1).unwrap();
    assert_eq!(seqs(&journal), [2]);
    assert!(matches!(journal.done(1), Err(StoreError::NotFound)));
    drop(journal);
    assert_eq!(seqs(&Journal::open(&path).unwrap()), [2]);
}

#[test]
fn the_file_is_only_appended_to() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actions.journal");
    let mut journal = Journal::open(&path).unwrap();
    journal.push(1, target(1), ActionKind::Delete).unwrap();
    let before = fs::read(&path).unwrap();
    journal.done(1).unwrap();
    let after = fs::read(&path).unwrap();
    assert!(after.len() > before.len());
    assert!(after.starts_with(&before));
}

#[test]
fn compact_keeps_pending_actions_and_the_sequence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actions.journal");
    let mut journal = Journal::open(&path).unwrap();
    for uid in 1..=3 {
        journal.push(1, target(uid), ActionKind::Delete).unwrap();
    }
    journal.done(1).unwrap();
    journal.done(3).unwrap();
    let before = fs::metadata(&path).unwrap().len();
    journal.compact().unwrap();
    assert!(fs::metadata(&path).unwrap().len() < before);
    assert_eq!(seqs(&journal), [2]);
    assert_eq!(
        journal.push(1, target(4), ActionKind::Delete).unwrap().seq,
        4
    );
    drop(journal);
    assert_eq!(seqs(&Journal::open(&path).unwrap()), [2, 4]);
}

#[test]
fn sequence_numbers_are_never_reused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actions.journal");
    let mut journal = Journal::open(&path).unwrap();
    journal.push(1, target(1), ActionKind::Delete).unwrap();
    journal.done(1).unwrap();
    journal.compact().unwrap();
    drop(journal);
    let mut journal = Journal::open(&path).unwrap();
    assert_eq!(
        journal.push(1, target(2), ActionKind::Delete).unwrap().seq,
        2
    );
}

#[test]
fn a_torn_last_line_is_dropped_and_repaired() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actions.journal");
    let mut journal = Journal::open(&path).unwrap();
    journal.push(1, target(1), ActionKind::Delete).unwrap();
    drop(journal);
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(br#"{"op":"add","act"#)
        .unwrap();

    let mut journal = Journal::open(&path).unwrap();
    assert_eq!(seqs(&journal), [1]);
    journal.push(2, target(2), ActionKind::Delete).unwrap();
    drop(journal);
    assert_eq!(seqs(&Journal::open(&path).unwrap()), [1, 2]);
}

#[test]
fn a_damaged_line_in_the_middle_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actions.journal");
    fs::write(&path, b"not json\n{\"op\":\"done\",\"seq\":1}\n").unwrap();
    let error = Journal::open(&path).err().unwrap();
    assert!(matches!(error, StoreError::BadRecord { .. }), "{error:?}");
    assert_eq!(
        fs::read(&path).unwrap(),
        b"not json\n{\"op\":\"done\",\"seq\":1}\n"
    );
}

#[test]
fn opening_under_a_missing_nested_folder_works() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a").join("b").join("journal.jsonl");
    let mut journal = Journal::open(&path).unwrap();
    journal.push(1, target(1), ActionKind::Delete).unwrap();
    assert_eq!(Journal::open(&path).unwrap().pending().count(), 1);
}

const OLD_ADD: &str = r#"{"op":"add","action":{"seq":1,"at_ms":5,"target":{"account":"1700","folder":"INBOX","uidvalidity":9,"uid":1,"message_id":null},"kind":{"kind":"delete"}}}"#;
const NEWER_KIND: &str = r#"{"op":"add","action":{"seq":2,"at_ms":6,"target":{"account":"1700","folder":"INBOX","uidvalidity":9,"uid":2,"message_id":null},"kind":{"kind":"snooze","until":9}}}"#;
const NEWER_VERSION: &str = r#"{"v":7,"op":"add","action":{"seq":3,"at_ms":7,"anything":"new"}}"#;
const LAST_ADD: &str = r#"{"v":1,"op":"add","action":{"seq":4,"at_ms":8,"target":{"account":"1700","folder":"INBOX","uidvalidity":9,"uid":4,"message_id":null},"kind":{"kind":"delete"}}}"#;

#[test]
fn a_file_written_before_versions_still_opens_and_replays() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actions.journal");
    fs::write(&path, format!("{OLD_ADD}\n")).unwrap();
    let journal = Journal::open(&path).unwrap();
    assert_eq!(seqs(&journal), [1]);
    assert!(journal.skipped().is_empty());
}

#[test]
fn new_records_carry_a_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actions.journal");
    let mut journal = Journal::open(&path).unwrap();
    journal.push(1, target(1), ActionKind::Delete).unwrap();
    journal.done(1).unwrap();
    for line in fs::read_to_string(&path).unwrap().lines() {
        assert!(line.contains(r#""v":1"#), "{line}");
    }
}

#[test]
fn an_unknown_kind_or_version_in_the_middle_is_skipped_surfaced_and_kept() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actions.journal");
    let text = format!("{OLD_ADD}\n{NEWER_KIND}\n{NEWER_VERSION}\n{LAST_ADD}\n");
    fs::write(&path, &text).unwrap();
    let mut journal = Journal::open(&path).unwrap();
    assert_eq!(seqs(&journal), [1, 4]);
    let skipped = journal.skipped();
    assert_eq!(skipped.len(), 2);
    assert_eq!((skipped[0].line, skipped[0].seq), (2, Some(2)));
    assert_eq!(skipped[0].reason, SkipReason::UnknownKind);
    assert_eq!((skipped[1].line, skipped[1].seq), (3, Some(3)));
    assert_eq!(skipped[1].reason, SkipReason::UnknownVersion(7));
    // Opening writes nothing, and a compact keeps the skipped lines.
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
    journal.compact().unwrap();
    let kept = fs::read_to_string(&path).unwrap();
    assert!(kept.contains(NEWER_KIND) && kept.contains(NEWER_VERSION));
    drop(journal);
    let mut journal = Journal::open(&path).unwrap();
    assert_eq!(seqs(&journal), [1, 4]);
    assert_eq!(journal.skipped().len(), 2);
    // Skipped sequence numbers are never reused.
    assert_eq!(
        journal.push(1, target(9), ActionKind::Delete).unwrap().seq,
        5
    );
}

#[test]
fn a_failed_sync_is_reported_as_possibly_applied() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actions.journal");
    let mut journal = Journal::open(&path).unwrap();
    journal.fail_next_sync();
    let error = journal.push(1, target(1), ActionKind::Delete).unwrap_err();
    assert!(error.may_still_apply(), "{error:?}");
    // The line is on disk: a crash now replays it, which is why the error
    // said "may".
    assert_eq!(seqs(&Journal::open(&path).unwrap()), [1]);
    // The session carries on and the next append repairs the file.
    let next = journal.push(2, target(2), ActionKind::Delete).unwrap();
    assert_eq!(seqs(&journal), [next.seq]);
}

#[test]
fn a_push_after_a_may_still_apply_failure_never_reuses_its_seq() {
    let dir = tempfile::tempdir().unwrap();
    let mut journal = Journal::open(&dir.path().join("actions.journal")).unwrap();
    journal.fail_next_sync();
    assert!(journal.push(1, target(1), ActionKind::Delete).is_err());
    assert_eq!(
        journal.push(2, target(2), ActionKind::Delete).unwrap().seq,
        2
    );
}

#[test]
fn other_errors_do_not_claim_the_action_may_apply() {
    assert!(!StoreError::NotFound.may_still_apply());
    assert!(!StoreError::Io(std::io::Error::other("x")).may_still_apply());
}

#[test]
fn a_done_of_any_version_finishes_its_action() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actions.journal");
    let text = format!(
        "{OLD_ADD}\n{}\n",
        r#"{"v":2,"op":"done","seq":1,"why":"undo"}"#
    );
    fs::write(&path, text).unwrap();
    let journal = Journal::open(&path).unwrap();
    assert!(seqs(&journal).is_empty());
    assert!(journal.skipped().is_empty());
}

#[test]
fn a_done_drops_the_skipped_add_it_finishes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actions.journal");
    let done = r#"{"v":1,"op":"done","seq":2}"#;
    fs::write(&path, format!("{OLD_ADD}\n{NEWER_KIND}\n{done}\n")).unwrap();
    let mut journal = Journal::open(&path).unwrap();
    assert!(journal.skipped().is_empty());
    journal.compact().unwrap();
    assert!(!fs::read_to_string(&path).unwrap().contains("snooze"));
    drop(journal);
    let journal = Journal::open(&path).unwrap();
    assert_eq!(seqs(&journal), [1]);
    assert!(journal.skipped().is_empty());
}

#[test]
fn compact_keeps_skipped_lines_in_sequence_order() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actions.journal");
    fs::write(&path, format!("{OLD_ADD}\n{NEWER_KIND}\n{LAST_ADD}\n")).unwrap();
    let mut journal = Journal::open(&path).unwrap();
    journal.compact().unwrap();
    let text = fs::read_to_string(&path).unwrap();
    let at = |needle: &str| text.find(needle).unwrap();
    assert!(at(r#""seq":1,"#) < at("snooze") && at("snooze") < at(r#""seq":4,"#));
}

#[test]
fn a_failed_sync_on_done_may_still_apply() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actions.journal");
    let mut journal = Journal::open(&path).unwrap();
    journal.push(1, target(1), ActionKind::Delete).unwrap();
    journal.fail_next_sync();
    let error = journal.done(1).unwrap_err();
    assert!(error.may_still_apply(), "{error:?}");
    assert!(seqs(&Journal::open(&path).unwrap()).is_empty());
}

#[test]
fn a_sync_that_runs_out_of_space_is_disk_full() {
    let error = StoreError::MayStillApply(std::io::Error::from(std::io::ErrorKind::StorageFull));
    assert!(error.is_disk_full());
}

#[test]
fn a_committed_delete_or_move_leaves_a_forget_intent_that_survives_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actions.journal");
    let mut journal = Journal::open(&path).unwrap();
    let delete = journal.push(1, target(1), ActionKind::Delete).unwrap();
    let flag = ActionKind::SetFlags {
        add: Flags::SEEN,
        remove: Flags::empty(),
    };
    let seen = journal.push(2, target(2), flag).unwrap();
    // Pending: not owed yet.
    assert!(journal.to_forget("1700").is_empty());
    journal.commit(delete.seq).unwrap();
    journal.commit(seen.seq).unwrap();
    drop(journal);
    let mut journal = Journal::open(&path).unwrap();
    let owed = journal.to_forget("1700");
    assert_eq!(owed.iter().map(|a| a.seq).collect::<Vec<_>>(), [delete.seq]);
    assert!(journal.to_forget("other").is_empty());
    journal.settle(&[delete.seq]).unwrap();
    drop(journal);
    assert!(Journal::open(&path).unwrap().to_forget("1700").is_empty());
}

#[test]
fn a_torn_forget_intent_line_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actions.journal");
    let mut journal = Journal::open(&path).unwrap();
    let delete = journal.push(1, target(1), ActionKind::Delete).unwrap();
    journal.commit(delete.seq).unwrap();
    drop(journal);
    let mut sidecar = OpenOptions::new()
        .append(true)
        .open(dir.path().join("actions.journal.forget"))
        .unwrap();
    sidecar.write_all(b"{\"seq\":9,\"at_").unwrap();
    let journal = Journal::open(&path).unwrap();
    assert_eq!(journal.to_forget("1700").len(), 1);
}

#[test]
fn an_intent_added_after_a_torn_line_survives_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actions.journal");
    let mut journal = Journal::open(&path).unwrap();
    let first = journal.push(1, target(1), ActionKind::Delete).unwrap();
    journal.commit(first.seq).unwrap();
    drop(journal);
    let mut sidecar = OpenOptions::new()
        .append(true)
        .open(dir.path().join("actions.journal.forget"))
        .unwrap();
    sidecar.write_all(b"{\"seq\":9,\"at_").unwrap();
    let mut journal = Journal::open(&path).unwrap();
    let second = journal.push(2, target(2), ActionKind::Delete).unwrap();
    journal.commit(second.seq).unwrap();
    drop(journal);
    let owed = Journal::open(&path).unwrap().to_forget("1700");
    assert_eq!(
        owed.iter().map(|a| a.seq).collect::<Vec<_>>(),
        [first.seq, second.seq]
    );
}
