use super::*;
use std::rc::Rc;

use envio_sync::Key;
use tokio::sync::Notify;

use crate::clock::now_ms;
use crate::worker::testing::{Sink, open, seed};

#[test]
fn an_open_in_the_background_is_served() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    assert!(!config.foreground);
    let sink = Sink::default();
    let mut worker = open(&config, &sink);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let account = seeded.account.clone();
    // Nothing is wanted: the background stays quiet (and no poll is due).
    worker.live.get_mut(&account).unwrap().next_full_ms = i64::MAX;
    let idle = runtime.block_on(worker.pick_for(&account, now_ms(), false));
    assert_eq!(idle, None);
    // The reader opens a message that was never downloaded.
    worker.open_message(
        seeded.message,
        crate::api::command::ImagesChoice::Default,
        300,
    );
    let step = runtime.block_on(worker.pick_for(&account, now_ms(), false));
    assert_eq!(step, Some(Step::Body(seeded.message)));
    // Not with a full disk, which has no room for it.
    let full = runtime.block_on(worker.pick_for(&account, now_ms(), true));
    assert_eq!(full, None);
}

/// The seeded account, in the background, with its IDLE capability
/// set and a poll due; what `pick_for` made of it and the clock then.
fn background_pick(idle: Option<bool>, graph: bool, silent: bool) -> (Option<Step>, i64, i64) {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut worker = open(&config, &sink);
    if graph {
        crate::worker::testing::into_graph(&mut worker, &seeded.account);
    }
    if silent {
        worker
            .store
            .settings
            .notify_off
            .insert(seeded.account.clone());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let live = worker.live.get_mut(&seeded.account).unwrap();
    live.idle_capable = idle;
    live.next_full_ms = 0;
    let now = now_ms();
    let step = runtime.block_on(worker.pick_for(&seeded.account, now, false));
    let after = worker.live[&seeded.account].next_full_ms;
    (step, after, now)
}

#[test]
fn a_background_account_without_idle_polls_its_inbox() {
    for graph in [false, true] {
        let (step, after, now) = background_pick(Some(false), graph, false);
        assert!(matches!(step, Some(Step::Folder(_))), "{graph}: {step:?}");
        assert!(
            (now + BACKGROUND_POLL_MS..=now + BACKGROUND_POLL_MS + 5_000).contains(&after),
            "{graph}: {after}"
        );
    }
}

#[test]
fn a_background_account_with_idle_keeps_its_clock() {
    let (step, after, _) = background_pick(Some(true), false, false);
    assert_eq!(step, None);
    assert_eq!(after, 0);
}

#[test]
fn a_background_account_that_never_notifies_does_not_poll() {
    let (step, after, _) = background_pick(Some(false), true, true);
    assert_eq!(step, None);
    assert_eq!(after, 0);
}

#[test]
fn a_background_account_not_yet_known_to_lack_idle_does_not_poll() {
    let (step, after, _) = background_pick(None, false, false);
    assert_eq!(step, None);
    assert_eq!(after, 0);
}

#[test]
fn a_failing_poll_does_not_spin() {
    // The clock moved on when the poll was picked, before the step ran:
    // asking again at once picks nothing.
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut worker = open(&config, &sink);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let live = worker.live.get_mut(&seeded.account).unwrap();
    live.idle_capable = Some(false);
    live.next_full_ms = 0;
    let now = now_ms();
    let first = runtime.block_on(worker.pick_for(&seeded.account, now, false));
    assert!(first.is_some());
    worker
        .live
        .get_mut(&seeded.account)
        .unwrap()
        .wants
        .folders
        .clear();
    let again = runtime.block_on(worker.pick_for(&seeded.account, now + 1, false));
    assert_eq!(again, None);
    let at = worker.wake_at(now + 1);
    assert!(
        at.is_some_and(|at| at >= now + BACKGROUND_POLL_MS),
        "{at:?}"
    );
}

#[test]
fn recent_inbox_bodies_are_prefetched_until_skipped() {
    use envio_store::{Envelope, Flags, NewMessage};

    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut worker = open(&config, &sink);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let account = seeded.account.clone();
    // The seeded message arrived just now.
    let recent = NewMessage {
        uid: 1,
        size: 10,
        envelope: Envelope {
            subject: "hi".into(),
            date: now_ms() / 1_000,
            ..Envelope::default()
        },
        flags: Flags::default(),
        preview: String::new(),
    };
    worker
        .store
        .db
        .upsert_message(seeded.folder, &recent)
        .unwrap();
    worker.live.get_mut(&account).unwrap().next_full_ms = i64::MAX;
    // In the background nothing is downloaded ahead.
    let background = runtime.block_on(worker.pick_for(&account, now_ms(), false));
    assert_eq!(background, None);
    worker.foreground = true;
    let full = runtime.block_on(worker.pick_for(&account, now_ms(), true));
    assert_eq!(full, None, "not with a full disk");
    let step = runtime.block_on(worker.pick_for(&account, now_ms(), false));
    assert_eq!(step, Some(Step::Prefetch(seeded.folder)));
    // A message whose body could not be had is not tried again.
    let live = worker.live.get_mut(&account).unwrap();
    live.prefetch_skip.insert(seeded.message);
    let step = runtime.block_on(worker.pick_for(&account, now_ms(), false));
    assert_eq!(step, None);
}

/// Runs `body` (which builds its own worker: it is not `Send`) on a
/// thread, so a spinning `pick_for` fails the test instead of hanging.
fn within_bound<T: Send + 'static>(body: impl FnOnce() -> T + Send + 'static) -> T {
    let (done, result) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = done.send(body());
    });
    result
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("pick_for did not return within 5 s (a spin never returns)")
}

/// `pick_for` (with a full disk) for the seeded account, with a prefetch
/// candidate in the Inbox in the foreground. Returns the step and
/// whether the folder list was still `listed` afterwards: a reset of an
/// empty plan clears it, so it shows `has_wants` and `steps` agreed.
fn pick_with_candidate_on_a_full_disk() -> (Option<Step>, bool) {
    use envio_store::{Envelope, Flags, NewMessage};

    within_bound(move || {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let recent = NewMessage {
            uid: 1,
            size: 10,
            envelope: Envelope {
                subject: "hi".into(),
                date: now_ms() / 1_000,
                ..Envelope::default()
            },
            flags: Flags::default(),
            preview: String::new(),
        };
        worker
            .store
            .db
            .upsert_message(seeded.folder, &recent)
            .unwrap();
        worker.foreground = true;
        let live = worker.live.get_mut(&seeded.account).unwrap();
        live.next_full_ms = i64::MAX;
        live.listed = true;
        let step = runtime.block_on(worker.pick_for(&seeded.account, now_ms(), true));
        (step, worker.live[&seeded.account].listed)
    })
}

#[test]
fn a_second_empty_plan_stops() {
    assert_eq!(after_plan(true, true, 0), Pass::ResetAndRetry);
    assert_eq!(after_plan(true, true, 1), Pass::Stop);
    assert_eq!(after_plan(true, false, 1), Pass::Run);
}

#[test]
fn pick_for_ends_when_wants_and_steps_disagree() {
    // The #116 disagreement: `has_wants` says run, `steps` is empty.
    let step = within_bound(|| {
        FORCE_WANTS.with(|force| force.set(true));
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        worker.foreground = true;
        let live = worker.live.get_mut(&seeded.account).unwrap();
        live.wants = Wants::default();
        live.next_full_ms = i64::MAX;
        runtime.block_on(worker.pick_for(&seeded.account, now_ms(), false))
    });
    assert_eq!(step, None);
}

#[test]
fn a_reset_that_clears_the_wants_ends_the_refresh_waiting_on_them() {
    // `ResetAndRetry` forgets the wants of a folder the background does
    // not serve; the refresh that waited on them is then over.
    let (events, left) = within_bound(|| {
        FORCE_WANTS.with(|force| force.set(true));
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let live = worker.live.get_mut(&seeded.account).unwrap();
        live.wants = Wants::default();
        live.wants.folders.insert(envio_store::FolderId(9_999));
        live.next_full_ms = i64::MAX;
        worker
            .refreshes
            .push(std::iter::once(seeded.account.clone()).collect());
        worker.settle_refreshes();
        let before = sink.events().len();
        runtime.block_on(worker.pick_for(&seeded.account, now_ms(), false));
        worker.settle_refreshes();
        let events = sink.events();
        let left = worker.live[&seeded.account].wants.folders.len();
        (
            events[before..]
                .iter()
                .filter(|e| matches!(e, crate::api::event::Event::Refreshed))
                .count(),
            left,
        )
    });
    assert_eq!(left, 0, "the reset cleared the folder");
    assert_eq!(events, 1);
}

// Passes on master too (the rules agree there): a guard for the single
// prefetch rule. `disk_full` is in `Env` but not in `Timing`, so a full
// disk is where `has_wants` and `steps` could part.
#[test]
fn a_full_disk_with_candidates_left_neither_prefetches_nor_resets() {
    let (step, listed) = pick_with_candidate_on_a_full_disk();
    assert_eq!(step, None, "no prefetch on a full disk");
    assert!(listed, "the planner sent the account to sleep, not to run");
}

#[test]
fn a_folder_in_parts_goes_first_and_commands_run_between_parts() {
    use crate::api::command::Command;
    use crate::api::event::Event;
    use crate::worker::Input;
    use envio_store::FolderRole;

    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut worker = open(&config, &sink);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let account = seeded.account.clone();
    worker.foreground = true;
    // The Inbox sorts first; the Archive is the one with parts left.
    let archive = worker
        .store
        .db
        .upsert_folder(&account, "Archive", FolderRole::Archive)
        .unwrap();
    let live = worker.live.get_mut(&account).unwrap();
    live.listed = true;
    live.next_full_ms = i64::MAX;
    live.wants.folders.insert(seeded.folder);
    live.wants.folders.insert(archive);
    let plain = runtime.block_on(worker.pick_for(&account, now_ms(), false));
    assert_eq!(plain, Some(Step::Folder(seeded.folder)));
    worker.live.get_mut(&account).unwrap().continuing = Some(archive);
    let first = runtime.block_on(worker.pick_for(&account, now_ms(), false));
    assert_eq!(first, Some(Step::Folder(archive)));
    // Between two parts the app asks for the folder list: answered at once.
    let command = Command::ListFolders {
        account: account.clone(),
    };
    runtime.block_on(worker.input(Input::Command((command, now_ms()))));
    assert!(
        sink.events()
            .iter()
            .any(|e| matches!(e, Event::Folders { .. }))
    );
    // And the folder in progress is still the next step.
    let next = runtime.block_on(worker.pick_for(&account, now_ms(), false));
    assert_eq!(next, Some(Step::Folder(archive)));
}

#[test]
fn worker_wakes_at_send_after() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut worker = open(&config, &sink);
    // An IDLE account: no poll timer among the wakes.
    worker.live.get_mut(&seeded.account).unwrap().idle_capable = None;
    let at = now_ms() + 600_000;
    let new = envio_store::NewOutboxItem {
        send_after_ms: at,
        account: seeded.account.clone(),
        message_id: "<held@example.org>".into(),
        queued_at_ms: now_ms(),
        from: "me@example.org".into(),
        recipients: vec!["you@example.org".into()],
    };
    worker
        .store
        .outbox
        .enqueue(new, b"Subject: x\r\n\r\n")
        .unwrap();
    assert_eq!(worker.wake_at(now_ms()), Some(at));
    // Before the time nothing is asked; at it, a send is wanted once.
    worker.live.get_mut(&seeded.account).unwrap().wants.send = false;
    worker.on_timers(at - 1);
    assert!(!worker.live[&seeded.account].wants.send);
    worker.on_timers(at);
    assert!(worker.live[&seeded.account].wants.send);
    assert_ne!(worker.wake_at(at), Some(at));
}

#[test]
fn an_idling_account_yields_no_wake_time() {
    let health = Health::default();
    // Its full run is overdue, which would spin the loop if it counted.
    assert_eq!(account_wake(false, &health, Some(0), 10_000), None);
    assert_eq!(account_wake(true, &health, Some(0), 10_000), Some(0));
}

#[test]
fn only_a_foreground_or_notifying_account_outside_idle_needs_a_timer() {
    let idling = Conn::Idling(Rc::new(Notify::new()));
    assert!(!needs_timer(true, true, &idling));
    assert!(!needs_timer(false, false, &Conn::Closed));
    assert!(needs_timer(true, false, &Conn::Closed));
    // A notifying account in the background waits for its retries only.
    assert!(needs_timer(false, true, &Conn::Closed));
    assert!(!needs_timer(false, true, &idling));
}

#[test]
fn a_connect_in_flight_needs_no_timer() {
    // Its own future wakes the loop (its timeout), like IDLE's.
    let (_future, handle) = futures::future::abortable(std::future::pending::<()>());
    let opening = Conn::Opening(handle);
    assert!(!needs_timer(true, true, &opening));
    assert!(!needs_timer(false, true, &opening));
}

#[test]
fn a_retry_is_handled_once_and_only_when_due() {
    let mut schedule = Schedule::default();
    let first = schedule.failed(Key::Action(1), 0);
    let second = schedule.failed(Key::Action(2), 5_000);
    assert_eq!(due_retry(&schedule, i64::MIN, first - 1), None);
    assert_eq!(due_retry(&schedule, i64::MIN, first), Some(first));
    // With the first handled, the second is next, not the first again.
    assert_eq!(due_retry(&schedule, first, second - 1), None);
    assert_eq!(due_retry(&schedule, first, second), Some(second));
}

#[test]
fn a_stale_retry_does_not_hide_a_later_one() {
    // Account 1's entry is never retried (it is paused); account 2's is.
    let mut schedule = Schedule::default();
    let stale = schedule.failed(Key::Action(1), 0);
    let later = schedule.failed(Key::Action(2), 2_000);
    let mut handled = i64::MIN;
    assert_eq!(schedule.next_due_after(handled), Some(stale));
    handled = stale;
    assert_eq!(schedule.next_due_after(handled), Some(later));
    handled = later;
    assert_eq!(schedule.next_due_after(handled), None);
}

#[test]
fn a_queued_inbox_waits_for_the_disk_in_the_background() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut worker = open(&config, &sink);
    let inbox = seeded.folder;
    let live = worker.live.get_mut(&seeded.account).unwrap();
    // An IDLE account: no poll timer among the wakes.
    live.idle_capable = None;
    live.wants.folders.insert(inbox);
    let now = now_ms();
    assert_eq!(worker.wake_at(now), None);
    worker.disk_full_until = Some(now + 60_000);
    assert_eq!(worker.wake_at(now), Some(now + 60_000));
}

/// The seeded worker with an unconfirmed item whose next look is at
/// `at`; the account's IDLE capability is set so no poll timer shows.
fn with_resolve_due(at: i64) -> (Worker<'static>, String) {
    let dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
    let (config, seeded) = seed(dir.path());
    let config = Box::leak(Box::new(config));
    let sink = Box::leak(Box::new(Sink::default()));
    let worker = open(config, sink);
    let new = envio_store::NewOutboxItem {
        send_after_ms: 0,
        account: seeded.account.clone(),
        message_id: "<a@example.org>".into(),
        queued_at_ms: 1,
        from: "me@example.org".into(),
        recipients: vec!["ada@example.org".into()],
    };
    let id = worker.store.outbox.enqueue(new, b"x").unwrap().id;
    // Seeding only (the grep test allows one `set_state` call).
    let seed_state = envio_store::Outbox::set_state;
    seed_state(
        &worker.store.outbox,
        &id,
        envio_store::SendState::MayHaveBeenSent,
    )
    .unwrap();
    worker.store.outbox.set_resolve(&id, 1, at).unwrap();
    let mut worker = worker;
    worker.live.get_mut(&seeded.account).unwrap().idle_capable = None;
    (worker, seeded.account)
}

#[test]
fn a_due_look_for_a_sent_copy_wakes_the_loop_and_asks_for_a_file_run() {
    let now = now_ms();
    let (mut worker, account) = with_resolve_due(now + 10_000);
    assert_eq!(worker.wake_at(now), Some(now + 10_000));
    worker.on_timers(now);
    assert!(!worker.live[&account].wants.file, "not yet");
    worker.on_timers(now + 10_000);
    assert!(worker.live[&account].wants.file);
    // Acted on: the same time does not wake the loop again.
    assert_eq!(worker.wake_at(now + 10_000), None);
}
