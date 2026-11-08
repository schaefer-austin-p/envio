use std::fs::{self, OpenOptions};
use std::io::Write;

use envio_store::{ActionKind, Flags, Journal, MessageRef, StoreError};

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
