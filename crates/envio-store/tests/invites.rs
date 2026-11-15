use std::fs;

use envio_store::{InviteAnswer, forget_invite, load_invites, record_invite};

const DAY_S: i64 = 86_400;
const NOW: i64 = 1_800_000_000_000;

fn answer(uid: &str, end_unix: i64) -> InviteAnswer {
    InviteAnswer {
        account: "1700".into(),
        uid: uid.into(),
        recurrence_id: String::new(),
        sequence: 0,
        answer: "accepted".into(),
        answered_at_ms: NOW,
        event_end_unix: end_unix,
        reply_message_id: format!("<reply-{uid}@envio.test>"),
    }
}

#[test]
fn invites_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invites.json");
    assert!(load_invites(&path).unwrap().is_empty());
    let a = answer("a@example.org", NOW / 1000 + 3600);
    record_invite(&path, a.clone(), NOW).unwrap();
    assert_eq!(load_invites(&path).unwrap(), [a]);
}

#[test]
fn record_replaces_same_uid() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invites.json");
    let end = NOW / 1000 + 3600;
    record_invite(&path, answer("a@example.org", end), NOW).unwrap();
    let mut second = answer("a@example.org", end);
    second.answer = "declined".into();
    second.sequence = 1;
    record_invite(&path, second.clone(), NOW).unwrap();
    // The same uid in another account is a different invite.
    let mut other = answer("a@example.org", end);
    other.account = "1701".into();
    record_invite(&path, other.clone(), NOW).unwrap();
    assert_eq!(load_invites(&path).unwrap(), [second, other]);
}

#[test]
fn record_prunes_old_events() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invites.json");
    let now_s = NOW / 1000;
    let old = answer("old@example.org", now_s - 31 * DAY_S);
    let recent = answer("recent@example.org", now_s - 29 * DAY_S);
    let future = answer("future@example.org", now_s + 3600);
    record_invite(&path, old, NOW).unwrap();
    record_invite(&path, recent.clone(), NOW).unwrap();
    record_invite(&path, future.clone(), NOW).unwrap();
    assert_eq!(load_invites(&path).unwrap(), [recent, future]);
}

#[test]
fn forget_removes_only_that_uid() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invites.json");
    let end = NOW / 1000 + 3600;
    let keep = answer("keep@example.org", end);
    record_invite(&path, answer("drop@example.org", end), NOW).unwrap();
    record_invite(&path, keep.clone(), NOW).unwrap();
    forget_invite(&path, "1700", "drop@example.org", "").unwrap();
    forget_invite(&path, "1700", "never@example.org", "").unwrap();
    assert_eq!(load_invites(&path).unwrap(), [keep]);
}

#[test]
fn invites_file_written_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invites.json");
    record_invite(&path, answer("a@example.org", NOW / 1000 + 60), NOW).unwrap();
    let names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, ["invites.json"]);
    // A damaged file is moved aside, not overwritten.
    fs::write(&path, b"[1, 2").unwrap();
    assert!(load_invites(&path).unwrap().is_empty());
    assert_eq!(
        fs::read(dir.path().join("invites.json.damaged")).unwrap(),
        b"[1, 2"
    );
}

#[test]
fn an_occurrence_and_its_series_are_separate_answers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invites.json");
    let end = NOW / 1000 + 3600;
    let series = answer("a@example.org", end);
    let mut one = answer("a@example.org", end);
    one.recurrence_id = "20261118T100000".into();
    one.answer = "declined".into();
    record_invite(&path, series.clone(), NOW).unwrap();
    record_invite(&path, one.clone(), NOW).unwrap();
    assert_eq!(load_invites(&path).unwrap(), [series.clone(), one.clone()]);
    forget_invite(&path, "1700", "a@example.org", "20261118T100000").unwrap();
    assert_eq!(load_invites(&path).unwrap(), [series]);
}

#[test]
fn an_old_file_without_recurrence_id_loads_as_the_series() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invites.json");
    fs::write(
        &path,
        r#"[{"account":"1700","uid":"a@example.org","sequence":0,"answer":"accepted",
"answered_at_ms":1,"event_end_unix":2000000000,"reply_message_id":"<r@envio.test>"}]"#,
    )
    .unwrap();
    let all = load_invites(&path).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].recurrence_id, "");
}
