use std::fs;

use envio_store::{NewOutboxItem, Outbox, SendState, StoreError};

fn new(queued_at_ms: i64) -> NewOutboxItem {
    NewOutboxItem {
        send_after_ms: 0,
        account: "1700".into(),
        message_id: format!("<{queued_at_ms}@example.org>"),
        queued_at_ms,
        from: "ada@example.org".into(),
        recipients: vec!["bob@example.org".into()],
    }
}

#[test]
fn an_enqueued_message_is_queued_with_its_eml() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = Outbox::open(dir.path()).unwrap();
    let item = outbox.enqueue(new(5), b"Subject: hi\r\n\r\nbody").unwrap();
    assert_eq!(item.meta.state, SendState::Queued);
    assert_eq!(outbox.items().unwrap(), std::slice::from_ref(&item));
    assert_eq!(outbox.eml(&item.id).unwrap(), b"Subject: hi\r\n\r\nbody");
    let reopened = Outbox::open(dir.path()).unwrap();
    assert_eq!(reopened.items().unwrap(), [item]);
}

#[test]
fn items_are_oldest_first_with_distinct_ids() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = Outbox::open(dir.path()).unwrap();
    let late = outbox.enqueue(new(20), b"b").unwrap();
    let early = outbox.enqueue(new(10), b"a").unwrap();
    let same_ms = outbox.enqueue(new(10), b"c").unwrap();
    assert_ne!(early.id, same_ms.id);
    let ids: Vec<String> = outbox
        .items()
        .unwrap()
        .into_iter()
        .map(|item| item.id)
        .collect();
    assert_eq!(ids, [early.id, same_ms.id, late.id]);
}

#[test]
fn an_item_left_sending_may_have_been_sent_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = Outbox::open(dir.path()).unwrap();
    let item = outbox.enqueue(new(5), b"x").unwrap();
    outbox.set_state(&item.id, SendState::Sending).unwrap();
    drop(outbox);
    for _ in 0..2 {
        let outbox = Outbox::open(dir.path()).unwrap();
        assert_eq!(
            outbox.items().unwrap()[0].meta.state,
            SendState::MayHaveBeenSent
        );
    }
}

#[test]
fn an_accepted_item_stays_accepted_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = Outbox::open(dir.path()).unwrap();
    let item = outbox.enqueue(new(5), b"x").unwrap();
    outbox.set_state(&item.id, SendState::Accepted).unwrap();
    drop(outbox);
    let outbox = Outbox::open(dir.path()).unwrap();
    assert_eq!(outbox.items().unwrap()[0].meta.state, SendState::Accepted);
}

#[test]
fn a_failure_reason_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = Outbox::open(dir.path()).unwrap();
    let item = outbox.enqueue(new(5), b"x").unwrap();
    let failed = SendState::Failed {
        reason: "550 mailbox unavailable".into(),
    };
    outbox.set_state(&item.id, failed.clone()).unwrap();
    assert_eq!(
        Outbox::open(dir.path()).unwrap().items().unwrap()[0]
            .meta
            .state,
        failed
    );
}

#[test]
fn the_smtp_envelope_is_kept_across_state_changes() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = Outbox::open(dir.path()).unwrap();
    let item = outbox.enqueue(new(5), b"x").unwrap();
    outbox.set_state(&item.id, SendState::Sending).unwrap();
    let meta = &Outbox::open(dir.path()).unwrap().items().unwrap()[0].meta;
    assert_eq!(meta.from, "ada@example.org");
    assert_eq!(meta.recipients, ["bob@example.org"]);
}

#[test]
fn remove_deletes_both_files() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = Outbox::open(dir.path()).unwrap();
    let item = outbox.enqueue(new(5), b"x").unwrap();
    outbox.remove(&item.id).unwrap();
    assert!(outbox.items().unwrap().is_empty());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    assert!(matches!(outbox.remove(&item.id), Err(StoreError::NotFound)));
    assert!(matches!(outbox.eml(&item.id), Err(StoreError::NotFound)));
}

#[test]
fn half_finished_enqueues_and_temp_files_are_cleared_on_open() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("99.eml"), b"orphan").unwrap();
    fs::write(dir.path().join(".98.json.tmp"), b"torn").unwrap();
    let outbox = Outbox::open(dir.path()).unwrap();
    assert!(outbox.items().unwrap().is_empty());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn ids_that_are_not_keys_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = Outbox::open(dir.path()).unwrap();
    for bad in ["../x", "", "a"] {
        assert!(
            matches!(outbox.eml(bad), Err(StoreError::NotFound)),
            "{bad}"
        );
        assert!(
            matches!(outbox.remove(bad), Err(StoreError::NotFound)),
            "{bad}"
        );
        assert!(matches!(
            outbox.set_state(bad, SendState::Queued),
            Err(StoreError::NotFound)
        ));
    }
}

#[test]
fn a_damaged_item_is_reported_and_kept_while_the_rest_still_load() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = Outbox::open(dir.path()).unwrap();
    let good = outbox.enqueue(new(5), b"x").unwrap();
    outbox.set_state(&good.id, SendState::Sending).unwrap();
    fs::write(dir.path().join("77.json"), b"garbage").unwrap();
    fs::write(dir.path().join("77.eml"), b"keep me").unwrap();
    drop(outbox);
    let outbox = Outbox::open(dir.path()).unwrap();
    let items = outbox.items().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].meta.state, SendState::MayHaveBeenSent);
    assert_eq!(outbox.damaged().unwrap(), ["77"]);
    assert!(dir.path().join("77.json").exists());
    assert!(dir.path().join("77.eml").exists());
}

#[test]
fn an_item_without_its_eml_is_damaged_not_listed() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = Outbox::open(dir.path()).unwrap();
    let item = outbox.enqueue(new(5), b"x").unwrap();
    fs::remove_file(dir.path().join(format!("{}.eml", item.id))).unwrap();
    assert!(outbox.items().unwrap().is_empty());
    assert_eq!(outbox.damaged().unwrap(), [item.id]);
}

#[test]
fn a_damaged_item_can_be_removed() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = Outbox::open(dir.path()).unwrap();
    let item = outbox.enqueue(new(5), b"x").unwrap();
    fs::remove_file(dir.path().join(format!("{}.eml", item.id))).unwrap();
    outbox.remove(&item.id).unwrap();
    assert!(outbox.damaged().unwrap().is_empty());
    fs::write(dir.path().join("77.json"), b"garbage").unwrap();
    fs::write(dir.path().join("77.eml"), b"x").unwrap();
    outbox.remove("77").unwrap();
    assert!(outbox.damaged().unwrap().is_empty());
    assert!(!dir.path().join("77.eml").exists());
}

#[test]
fn old_meta_without_send_after_reads_as_zero() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = Outbox::open(dir.path()).unwrap();
    let item = outbox.enqueue(new(5), b"x").unwrap();
    let path = dir.path().join(format!("{}.json", item.id));
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .remove("send_after_ms")
        .unwrap();
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(outbox.items().unwrap()[0].meta.send_after_ms, 0);
}

#[test]
fn a_held_item_keeps_its_send_after() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = Outbox::open(dir.path()).unwrap();
    let item = outbox
        .enqueue(
            NewOutboxItem {
                send_after_ms: 9_000,
                ..new(5)
            },
            b"x",
        )
        .unwrap();
    assert_eq!(item.meta.send_after_ms, 9_000);
    assert_eq!(outbox.items().unwrap()[0].meta.send_after_ms, 9_000);
}

#[test]
fn a_resolve_miss_is_kept_across_a_restart_and_leaves_the_state_alone() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = Outbox::open(dir.path()).unwrap();
    let item = outbox.enqueue(new(5), b"x").unwrap();
    outbox
        .set_state(&item.id, SendState::MayHaveBeenSent)
        .unwrap();
    outbox.set_resolve(&item.id, 2, 77_000).unwrap();
    let meta = |o: &Outbox| o.items().unwrap().remove(0).meta;
    assert_eq!(meta(&outbox).state, SendState::MayHaveBeenSent);
    assert_eq!(meta(&outbox).resolve_tries, 2);
    assert_eq!(meta(&outbox).resolve_after_ms, 77_000);
    let reopened = Outbox::open(dir.path()).unwrap();
    assert_eq!(meta(&reopened).resolve_tries, 2);
    assert_eq!(meta(&reopened).resolve_after_ms, 77_000);
}

#[test]
fn queueing_again_or_accepting_zeroes_the_resolve_count() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = Outbox::open(dir.path()).unwrap();
    let item = outbox.enqueue(new(5), b"x").unwrap();
    let meta = |o: &Outbox| o.items().unwrap().remove(0).meta;
    for state in [SendState::Queued, SendState::Accepted] {
        outbox.set_resolve(&item.id, 2, 77_000).unwrap();
        outbox.set_state(&item.id, state.clone()).unwrap();
        assert_eq!(meta(&outbox).resolve_tries, 0, "{state:?}");
        assert_eq!(meta(&outbox).resolve_after_ms, 0, "{state:?}");
    }
    // Other states keep the count.
    outbox.set_resolve(&item.id, 2, 77_000).unwrap();
    outbox
        .set_state(&item.id, SendState::MayHaveBeenSent)
        .unwrap();
    assert_eq!(meta(&outbox).resolve_tries, 2);
}

#[test]
fn a_resolve_miss_does_not_clear_an_item_found_half_sent() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = Outbox::open(dir.path()).unwrap();
    let item = outbox.enqueue(new(5), b"x").unwrap();
    outbox.set_state(&item.id, SendState::Sending).unwrap();
    // After a power loss the item is found `Sending`: it may have been sent.
    let reopened = Outbox::open(dir.path()).unwrap();
    reopened.set_resolve(&item.id, 1, 5).unwrap();
    let after = reopened.items().unwrap().remove(0).meta;
    assert_eq!(after.state, SendState::MayHaveBeenSent);
    assert_eq!(after.resolve_tries, 1);
}

#[test]
fn old_meta_without_resolve_fields_reads_as_zero() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = Outbox::open(dir.path()).unwrap();
    let item = outbox.enqueue(new(5), b"x").unwrap();
    let path = dir.path().join(format!("{}.json", item.id));
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let object = value.as_object_mut().unwrap();
    object.remove("resolve_tries").unwrap();
    object.remove("resolve_after_ms").unwrap();
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    let meta = outbox.items().unwrap().remove(0).meta;
    assert_eq!((meta.resolve_tries, meta.resolve_after_ms), (0, 0));
}
