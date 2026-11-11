//! Journal actions reach the server after the Undo window, once.

mod support;

use envio_net::NetError;
use envio_net::imap::{Caps, Role};
use envio_store::{ActionKind, Flags};
use envio_sync::driver::{record, replay, sync_folder, sync_folders};
use envio_sync::{ErrorKind, UNDO_MS};
use support::fake::{FakeServer, NOW_MS};
use support::local::{ACCOUNT, Store};

const LATER: i64 = NOW_MS + UNDO_MS;

async fn synced(server: &mut FakeServer, ids: &[&str]) -> Store {
    for id in ids {
        server.deliver_id("INBOX", id);
    }
    let mut store = Store::new();
    sync_folders(server, &mut store.local(), ACCOUNT)
        .await
        .unwrap();
    let inbox = store.folder("INBOX").id;
    sync_folder(server, &mut store.local(), inbox, NOW_MS)
        .await
        .unwrap();
    store
}

fn take(store: &mut Store, uid: u32, kind: ActionKind) {
    let inbox = store.folder("INBOX").id;
    let id = store.db.message_at(inbox, uid).unwrap().unwrap().id;
    record(&mut store.local(), id, kind, NOW_MS).unwrap();
}

fn read() -> ActionKind {
    ActionKind::SetFlags {
        add: Flags::SEEN,
        remove: Flags::empty(),
    }
}

#[tokio::test]
async fn a_flag_change_waits_for_the_undo_window_then_goes_once() {
    let mut server = FakeServer::new();
    let mut store = synced(&mut server, &["a"]).await;
    take(&mut store, 1, read());
    let early = replay(&mut server, &mut store.local(), ACCOUNT, LATER - 1)
        .await
        .unwrap();
    assert_eq!(early.done, 0);
    assert!(!server.message("INBOX", 1).unwrap().flags.seen);
    let done = replay(&mut server, &mut store.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    assert_eq!(done.done, 1);
    assert!(server.message("INBOX", 1).unwrap().flags.seen);
    assert_eq!(store.journal.pending().count(), 0);
}

#[tokio::test]
async fn with_no_folders_cached_a_delete_stays_pending_and_nothing_is_expunged() {
    let mut server = FakeServer::new();
    let mut store = synced(&mut server, &["a"]).await;
    take(&mut store, 1, ActionKind::Delete);
    let mut empty = Store::new();
    empty.journal = store.journal;
    let replayed = replay(&mut server, &mut empty.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    assert_eq!(replayed.done, 0);
    assert_eq!(server.uids("INBOX"), [1]);
    assert_eq!(empty.journal.pending().count(), 1);
}

#[tokio::test]
async fn delete_moves_to_trash_and_deletes_for_good_in_trash() {
    let mut server = FakeServer::new();
    server.add_folder("Trash", Some(Role::Trash));
    let mut store = synced(&mut server, &["a"]).await;
    take(&mut store, 1, ActionKind::Delete);
    replay(&mut server, &mut store.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    assert!(server.uids("INBOX").is_empty());
    assert_eq!(server.ids("Trash"), ["a@example.org"]);
    let trash = store.folder("Trash").id;
    sync_folder(&mut server, &mut store.local(), trash, LATER)
        .await
        .unwrap();
    let id = store.db.message_at(trash, 1).unwrap().unwrap().id;
    record(&mut store.local(), id, ActionKind::Delete, LATER).unwrap();
    replay(&mut server, &mut store.local(), ACCOUNT, LATER + UNDO_MS)
        .await
        .unwrap();
    assert!(server.uids("Trash").is_empty());
}

#[tokio::test]
async fn a_move_cut_between_copy_and_delete_does_not_copy_twice() {
    let mut server = FakeServer::new();
    server.caps = Caps::from_names(["UIDPLUS"]);
    server.add_folder("Archive", Some(Role::Archive));
    let mut store = synced(&mut server, &["a"]).await;
    take(
        &mut store,
        1,
        ActionKind::Move {
            to: "Archive".into(),
        },
    );
    server.fail_on("delete", NetError::Timeout);
    let cut = replay(&mut server, &mut store.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    assert_eq!(cut.stopped, Some(ErrorKind::Temporary));
    assert_eq!(server.ids("Archive").len(), 1);
    assert_eq!(server.uids("INBOX"), [1]);
    let retry = store.schedule.next_due().unwrap();
    let early = replay(&mut server, &mut store.local(), ACCOUNT, retry - 1)
        .await
        .unwrap();
    assert_eq!(early.done, 0, "waits out its backoff");
    let done = replay(&mut server, &mut store.local(), ACCOUNT, retry)
        .await
        .unwrap();
    assert_eq!(done.done, 1);
    assert_eq!(server.ids("Archive"), ["a@example.org"]);
    assert!(server.uids("INBOX").is_empty());
}

#[tokio::test]
async fn a_refused_flag_change_is_undone_locally() {
    let mut server = FakeServer::new();
    let mut store = synced(&mut server, &["a"]).await;
    take(&mut store, 1, read());
    server.fail_on("store_flags", NetError::Refused("read-only".into()));
    let replayed = replay(&mut server, &mut store.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    assert_eq!(replayed.reverted.len(), 1);
    assert_eq!(store.journal.pending().count(), 0);
    let inbox = store.folder("INBOX").id;
    assert_eq!(
        store.db.message_at(inbox, 1).unwrap().unwrap().flags,
        Flags::empty()
    );
}

#[tokio::test]
async fn after_a_uidvalidity_change_the_message_is_found_by_message_id() {
    let mut server = FakeServer::new();
    let mut store = synced(&mut server, &["a", "b"]).await;
    take(&mut store, 2, read());
    server.folders.get_mut("INBOX").unwrap().messages.remove(&1);
    server.renumber("INBOX");
    replay(&mut server, &mut store.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    assert_eq!(server.ids("INBOX"), ["b@example.org"]);
    assert!(server.message("INBOX", 1).unwrap().flags.seen);
}

#[tokio::test]
async fn an_action_on_a_message_that_is_gone_is_dropped_quietly() {
    let mut server = FakeServer::new();
    let mut store = synced(&mut server, &["a"]).await;
    take(&mut store, 1, ActionKind::Delete);
    server.folders.get_mut("INBOX").unwrap().messages.remove(&1);
    server.renumber("INBOX");
    let replayed = replay(&mut server, &mut store.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    assert_eq!((replayed.done, replayed.reverted.len()), (1, 0));
}

#[tokio::test]
async fn a_sign_in_failure_stops_and_keeps_the_action() {
    let mut server = FakeServer::new();
    let mut store = synced(&mut server, &["a", "b"]).await;
    take(&mut store, 1, read());
    take(&mut store, 2, read());
    server.fail_on("select", NetError::SignIn("expired".into()));
    let replayed = replay(&mut server, &mut store.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    assert_eq!(replayed.stopped, Some(ErrorKind::SignIn));
    assert_eq!(replayed.done, 0);
    assert_eq!(store.journal.pending().count(), 2);
    let after = replay(&mut server, &mut store.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    assert_eq!(after.done, 2);
}

fn live_ids(server: &FakeServer, path: &str) -> usize {
    server.folders[path]
        .messages
        .values()
        .filter(|message| !message.flags.deleted)
        .count()
}

#[tokio::test]
async fn moving_back_without_move_or_uidplus_is_not_fooled_by_the_deleted_leftover() {
    let mut server = FakeServer::new();
    server.caps = Caps::default();
    server.add_folder("Archive", Some(Role::Archive));
    let mut store = synced(&mut server, &["a"]).await;
    take(
        &mut store,
        1,
        ActionKind::Move {
            to: "Archive".into(),
        },
    );
    replay(&mut server, &mut store.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    assert_eq!(live_ids(&server, "Archive"), 1);
    let archive = store.folder("Archive").id;
    sync_folder(&mut server, &mut store.local(), archive, LATER)
        .await
        .unwrap();
    let id = store.db.message_at(archive, 1).unwrap().unwrap().id;
    let back = ActionKind::Move { to: "INBOX".into() };
    record(&mut store.local(), id, back, LATER).unwrap();
    replay(&mut server, &mut store.local(), ACCOUNT, LATER + UNDO_MS)
        .await
        .unwrap();
    assert_eq!(live_ids(&server, "INBOX"), 1, "one live copy in INBOX");
    assert_eq!(live_ids(&server, "Archive"), 0);
}

#[tokio::test]
async fn later_actions_on_a_message_wait_behind_one_that_is_backing_off() {
    let mut server = FakeServer::new();
    server.add_folder("Archive", Some(Role::Archive));
    let mut store = synced(&mut server, &["a"]).await;
    let inbox = store.folder("INBOX").id;
    let id = store.db.message_at(inbox, 1).unwrap().unwrap().id;
    record(&mut store.local(), id, read(), NOW_MS).unwrap();
    let to = ActionKind::Move {
        to: "Archive".into(),
    };
    record(&mut store.local(), id, to, NOW_MS).unwrap();
    server.fail_on("store_flags", NetError::Timeout);
    let first = replay(&mut server, &mut store.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    assert_eq!(first.stopped, Some(ErrorKind::Temporary));
    let held = replay(&mut server, &mut store.local(), ACCOUNT, LATER + 1)
        .await
        .unwrap();
    assert_eq!(held.done, 0, "the move waits for the flag change");
    assert_eq!(server.uids("INBOX"), [1]);
    assert!(server.ids("Archive").is_empty());
    let retry = store.schedule.next_due().unwrap();
    let both = replay(&mut server, &mut store.local(), ACCOUNT, retry)
        .await
        .unwrap();
    assert_eq!(both.done, 2);
    assert!(
        server.message("Archive", 1).unwrap().flags.seen,
        "the flag went first"
    );
}

#[tokio::test]
async fn a_failed_revert_keeps_the_run_result() {
    let mut server = FakeServer::new();
    let mut store = synced(&mut server, &["a"]).await;
    take(&mut store, 1, read());
    server.fail_on("store_flags", NetError::Refused("read-only".into()));
    server.fail_on("fetch_flags", NetError::Timeout);
    let replayed = replay(&mut server, &mut store.local(), ACCOUNT, LATER)
        .await
        .unwrap();
    assert_eq!(replayed.reverted.len(), 1);
    assert_eq!(replayed.stopped, Some(ErrorKind::Temporary));
    assert_eq!(store.journal.pending().count(), 0);
}

/// Runs replay once with one queued failure of `store_flags`, after the
/// action's backoff has passed.
async fn failing_run(
    server: &mut FakeServer,
    store: &mut Store,
    now: &mut i64,
    error: NetError,
) -> envio_sync::driver::Replayed {
    if let Some(due) = store.schedule.next_due() {
        *now = (*now).max(due);
    }
    server.fail_on("store_flags", error);
    replay(server, &mut store.local(), ACCOUNT, *now)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_protocol_failure_is_given_up_after_the_ceiling() {
    let mut server = FakeServer::new();
    let mut store = synced(&mut server, &["a"]).await;
    take(&mut store, 1, read());
    let mut now = LATER;
    for run in 1..=9 {
        let error = NetError::Protocol("?".into());
        let kept = failing_run(&mut server, &mut store, &mut now, error).await;
        assert!(kept.given_up.is_empty(), "run {run}");
        assert_eq!(kept.stopped, Some(ErrorKind::Protocol));
        assert_eq!(store.journal.pending().count(), 1, "run {run}");
    }
    let error = NetError::Protocol("?".into());
    let last = failing_run(&mut server, &mut store, &mut now, error).await;
    assert_eq!(last.given_up.len(), 1);
    assert_eq!(last.stopped, Some(ErrorKind::Protocol));
    assert_eq!(last.done, 0);
    assert_eq!(store.journal.pending().count(), 0);
    assert_eq!(store.schedule.next_due(), None, "the entry is cleared");
}

#[tokio::test]
async fn temporary_failures_never_give_up() {
    let mut server = FakeServer::new();
    let mut store = synced(&mut server, &["a"]).await;
    take(&mut store, 1, read());
    let mut now = LATER;
    for run in 1..=20 {
        let kept = failing_run(&mut server, &mut store, &mut now, NetError::Timeout).await;
        assert!(kept.given_up.is_empty(), "run {run}");
        assert_eq!(store.journal.pending().count(), 1, "run {run}");
    }
}

#[tokio::test]
async fn stopped_reports_protocol_not_temporary() {
    let mut server = FakeServer::new();
    let mut store = synced(&mut server, &["a"]).await;
    take(&mut store, 1, read());
    let mut now = LATER;
    let error = NetError::Protocol("?".into());
    let protocol = failing_run(&mut server, &mut store, &mut now, error).await;
    assert_eq!(protocol.stopped, Some(ErrorKind::Protocol));
    let timeout = failing_run(&mut server, &mut store, &mut now, NetError::Timeout).await;
    assert_eq!(timeout.stopped, Some(ErrorKind::Temporary));
}
