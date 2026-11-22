//! The move's driver over a temp store: no server is asked, so what is
//! checked is the saved step, the cache, the tokens and what survives.

use envio_net::oauth::ProviderKind;
use envio_store::{
    ActionKind, BackendKind, MessageRef, Migration, NewOutboxItem, Outbox, SendState,
};
use secrecy::SecretString;

use crate::api::event::Event;
use crate::api::view::{MigrationStage, MigrationState};
use crate::clock::now_ms;
use crate::core::Config;
use crate::migrate_graph::plan;
use crate::worker::Worker;
use crate::worker::testing::{Seeded, Sink, open, seed};

struct T {
    _dir: tempfile::TempDir,
    config: Config,
    seeded: Seeded,
}

/// An Outlook account on IMAP at `step`, with a cached message and its body.
fn moving(step: u8) -> T {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    {
        let w = open(&config, &sink);
        let key = &seeded.account;
        let mut settings = w.store.accounts.get(key).unwrap().settings.clone();
        settings.auth = Some("microsoft".into());
        settings.migration = Some(Migration { step, failures: 0 });
        let mut w = w;
        w.store.accounts.update(key, settings).unwrap();
        w.store
            .tokens
            .borrow_mut()
            .set(key, ProviderKind::Microsoft, SecretString::from("imap-rt"))
            .unwrap();
        w.store
            .bodies
            .write(seeded.folder, 1, b"Subject: hi\r\n\r\nbody")
            .unwrap();
    }
    T {
        _dir: dir,
        config,
        seeded,
    }
}

fn pending(w: &mut Worker<'_>, account: &str) {
    let target = MessageRef {
        account: account.into(),
        folder: "INBOX".into(),
        uidvalidity: 1,
        uid: 1,
        message_id: None,
    };
    w.store.journal.push(1, target, ActionKind::Delete).unwrap();
}

fn outbox_item(w: &Worker<'_>, account: &str, state: SendState) -> String {
    let new = NewOutboxItem {
        send_after_ms: 0,
        account: account.into(),
        message_id: "<a@example.org>".into(),
        queued_at_ms: 1,
        from: "me@example.org".into(),
        recipients: vec!["ada@example.org".into()],
    };
    let id = w
        .store
        .outbox
        .enqueue(new, b"Subject: x\r\n\r\n")
        .unwrap()
        .id;
    let seed_state = Outbox::set_state;
    seed_state(&w.store.outbox, &id, state).unwrap();
    id
}

fn draft(w: &Worker<'_>, account: &str) -> String {
    let draft = envio_store::Draft {
        account: account.into(),
        subject: "unsent".into(),
        ..envio_store::Draft::default()
    };
    w.store.drafts.create(&draft).unwrap()
}

fn settings(w: &Worker<'_>, key: &str) -> envio_store::AccountSettings {
    w.store.accounts.get(key).unwrap().settings.clone()
}

fn tick(w: &mut Worker<'_>) {
    w.migrate_all(now_ms());
}

fn cleared(w: &Worker<'_>, t: &T) -> bool {
    w.store.db.folders(&t.seeded.account).unwrap().is_empty()
        && w.store.bodies.read(t.seeded.folder, 1).is_err()
}

fn states(sink: &Sink) -> Vec<MigrationState> {
    sink.events()
        .into_iter()
        .filter_map(|e| match e {
            Event::Migration { state, .. } => state,
            _ => None,
        })
        .collect()
}

fn done(sink: &Sink) -> Vec<MigrationState> {
    states(sink)
        .into_iter()
        .filter(|s| s.stage == MigrationStage::Done)
        .collect()
}

#[test]
fn pending_actions_are_replayed_before_anything_is_cleared() {
    let t = moving(1);
    let sink = Sink::default();
    let mut w = open(&t.config, &sink);
    pending(&mut w, &t.seeded.account);
    tick(&mut w);
    assert!(w.live[&t.seeded.account].wants.replay);
    assert_eq!(settings(&w, &t.seeded.account).migration.unwrap().step, 1);
    assert!(!cleared(&w, &t));
    assert_eq!(settings(&w, &t.seeded.account).backend, BackendKind::Imap);
}

/// An action inside its Undo window is not due: no replay is asked for, the
/// turn is told when to look again, and at that time the replay is asked.
#[test]
fn nothing_replays_before_the_actions_are_due() {
    let t = moving(1);
    let sink = Sink::default();
    let mut w = open(&t.config, &sink);
    let key = t.seeded.account.clone();
    let target = MessageRef {
        account: key.clone(),
        folder: "INBOX".into(),
        uidvalidity: 1,
        uid: 1,
        message_id: None,
    };
    let at = 1_000_000;
    w.store
        .journal
        .push(at, target, ActionKind::Delete)
        .unwrap();
    let due = at + envio_sync::UNDO_MS;
    w.migrate_all(at + 1);
    assert!(!w.live[&key].wants.replay, "spun before the due time");
    assert_eq!(w.migration_wake(at + 1), Some(due));
    w.migrate_all(due);
    assert!(w.live[&key].wants.replay);
    assert_eq!(w.migration_wake(due), None);
}

#[test]
fn an_empty_journal_runs_all_steps_and_the_account_is_graph() {
    let t = moving(1);
    let sink = Sink::default();
    let mut w = open(&t.config, &sink);
    tick(&mut w);
    let key = &t.seeded.account;
    let s = settings(&w, key);
    assert_eq!(s.backend, BackendKind::Graph);
    assert!(s.imap.is_none() && s.smtp.is_none() && s.migration.is_none());
    assert!(cleared(&w, &t));
    assert!(
        w.store
            .tokens
            .borrow()
            .get(key, ProviderKind::Microsoft)
            .is_none()
    );
    assert_eq!(done(&sink).len(), 1);
}

#[test]
fn the_outbox_and_drafts_survive_the_move() {
    let t = moving(1);
    let sink = Sink::default();
    let mut w = open(&t.config, &sink);
    let key = t.seeded.account.clone();
    let queued = outbox_item(&w, &key, SendState::Queued);
    let failed = outbox_item(&w, &key, SendState::Failed { reason: "x".into() });
    let unsent = draft(&w, &key);
    tick(&mut w);
    assert!(cleared(&w, &t));
    let ids: Vec<String> = w
        .store
        .outbox
        .items()
        .unwrap()
        .into_iter()
        .map(|i| i.id)
        .collect();
    assert!(ids.contains(&queued) && ids.contains(&failed));
    assert!(w.store.drafts.load(&unsent).is_ok());
}

#[test]
fn an_accepted_item_holds_step_one_until_it_is_filed() {
    let t = moving(1);
    let sink = Sink::default();
    let mut w = open(&t.config, &sink);
    let key = t.seeded.account.clone();
    let id = outbox_item(&w, &key, SendState::Accepted);
    tick(&mut w);
    assert!(w.live[&key].wants.file, "filing is asked for");
    assert_eq!(settings(&w, &key).migration.unwrap().step, 1);
    assert!(!cleared(&w, &t), "nothing is cleared while it is unfiled");
    w.store.outbox.remove(&id).unwrap();
    tick(&mut w);
    assert!(cleared(&w, &t));
    assert_eq!(settings(&w, &key).backend, BackendKind::Graph);
}

/// One worker turn's look at the move: filing is asked for or not, and the
/// run that would follow takes the want (as `pick` does).
fn turn_files(w: &mut Worker<'_>, key: &str, now: i64) -> bool {
    w.migrate_all(now);
    std::mem::take(&mut w.live.get_mut(key).unwrap().wants.file)
}

#[test]
fn a_refused_filing_run_is_not_asked_for_again_until_the_next_tick() {
    let t = moving(1);
    let sink = Sink::default();
    let mut w = open(&t.config, &sink);
    let key = t.seeded.account.clone();
    outbox_item(&w, &key, SendState::Accepted);
    let at = 1_000_000;
    assert!(turn_files(&mut w, &key, at), "the first look asks");
    w.migrate_file_failed(&key);
    for turn in 1..=5 {
        assert!(!turn_files(&mut w, &key, at + turn), "spun on turn {turn}");
    }
    let tick = at + plan::FILE_TICK_MS;
    assert_eq!(w.migration_wake(at + 5), Some(tick), "the wake is the tick");
    assert!(turn_files(&mut w, &key, tick), "the tick asks again");
    assert_eq!(w.migration_wake(tick), Some(tick + plan::FILE_TICK_MS));
}

#[test]
fn with_an_empty_journal_stuck_needs_three_ticks_not_three_turns() {
    let t = moving(1);
    let sink = Sink::default();
    let mut w = open(&t.config, &sink);
    let key = t.seeded.account.clone();
    outbox_item(&w, &key, SendState::Accepted);
    let mut now = 1_000_000;
    let mut tries = 0;
    for _ in 0..20 {
        // Twenty turns inside one tick: one try at most.
        now += 1;
        if turn_files(&mut w, &key, now) {
            tries += 1;
            w.migrate_file_failed(&key);
        }
    }
    assert_eq!(tries, 1, "tries within one tick");
    assert_eq!(settings(&w, &key).migration.unwrap().failures, 1);
    assert!(
        states(&sink)
            .iter()
            .all(|s| s.stage != MigrationStage::Stuck)
    );
    for _ in 0..2 {
        now += plan::FILE_TICK_MS;
        assert!(turn_files(&mut w, &key, now));
        w.migrate_file_failed(&key);
    }
    w.migrate_all(now + 1);
    let stuck = states(&sink).pop().unwrap();
    assert_eq!(stuck.stage, MigrationStage::Stuck);
    assert_eq!(settings(&w, &key).migration.unwrap().failures, 3);
}

#[test]
fn filing_does_not_rerun_each_turn_while_an_action_is_held_in_undo() {
    let t = moving(1);
    let sink = Sink::default();
    let mut w = open(&t.config, &sink);
    let key = t.seeded.account.clone();
    outbox_item(&w, &key, SendState::Accepted);
    let target = MessageRef {
        account: key.clone(),
        folder: "INBOX".into(),
        uidvalidity: 1,
        uid: 1,
        message_id: None,
    };
    let at = 1_000_000;
    w.store
        .journal
        .push(at, target, ActionKind::Delete)
        .unwrap();
    assert!(turn_files(&mut w, &key, at + 1));
    w.migrate_file_failed(&key);
    for turn in 2..20 {
        assert!(!turn_files(&mut w, &key, at + turn), "spun on turn {turn}");
        assert!(!w.live[&key].wants.replay, "replay is held by Undo");
    }
}

#[test]
fn a_refused_replay_waits_and_counts() {
    let t = moving(1);
    let sink = Sink::default();
    let mut w = open(&t.config, &sink);
    let key = t.seeded.account.clone();
    pending(&mut w, &key);
    w.migrate_failed(&key);
    assert_eq!(settings(&w, &key).migration.unwrap().failures, 1);
    tick(&mut w);
    assert!(!cleared(&w, &t));
    assert_eq!(settings(&w, &key).backend, BackendKind::Imap);
}

#[test]
fn failures_are_saved_and_survive_a_restart() {
    let t = moving(1);
    let sink = Sink::default();
    let key = t.seeded.account.clone();
    {
        let mut w = open(&t.config, &sink);
        pending(&mut w, &key);
        w.migrate_failed(&key);
        w.migrate_failed(&key);
    }
    let w = open(&t.config, &sink);
    assert_eq!(settings(&w, &key).migration.unwrap().failures, 2);
}

#[test]
fn failures_after_step_one_are_not_counted() {
    let t = moving(2);
    let sink = Sink::default();
    let mut w = open(&t.config, &sink);
    let key = t.seeded.account.clone();
    w.migrate_failed(&key);
    assert_eq!(settings(&w, &key).migration.unwrap().failures, 0);
}

#[test]
fn three_failures_offer_switch_anyway_and_ask_for_no_more_replays() {
    let t = moving(1);
    let sink = Sink::default();
    let mut w = open(&t.config, &sink);
    let key = t.seeded.account.clone();
    pending(&mut w, &key);
    for _ in 0..3 {
        w.migrate_failed(&key);
    }
    w.live.get_mut(&key).unwrap().wants.replay = false;
    tick(&mut w);
    assert!(!w.live[&key].wants.replay);
    let stuck = states(&sink).pop().unwrap();
    assert_eq!(stuck.stage, MigrationStage::Stuck);
    assert!(stuck.can_switch);
}

#[test]
fn switch_anyway_drops_the_actions_with_a_notice_and_keeps_unsent_work() {
    let t = moving(1);
    let sink = Sink::default();
    let mut w = open(&t.config, &sink);
    let key = t.seeded.account.clone();
    pending(&mut w, &key);
    let queued = outbox_item(&w, &key, SendState::Queued);
    let unsent = draft(&w, &key);
    for _ in 0..3 {
        w.migrate_failed(&key);
    }
    w.switch_anyway(&key);
    assert_eq!(settings(&w, &key).backend, BackendKind::Graph);
    assert_eq!(w.store.journal.pending().count(), 0);
    let done = done(&sink);
    assert!(
        !done[0].note_title.is_empty(),
        "the dropped action is named"
    );
    assert!(
        w.store
            .outbox
            .items()
            .unwrap()
            .iter()
            .any(|i| i.id == queued)
    );
    assert!(w.store.drafts.load(&unsent).is_ok());
}

#[test]
fn the_dropped_notice_survives_a_restart_and_is_told_once_more() {
    let t = moving(1);
    let key = t.seeded.account.clone();
    {
        let sink = Sink::default();
        let mut w = open(&t.config, &sink);
        pending(&mut w, &key);
        for _ in 0..3 {
            w.migrate_failed(&key);
        }
        w.switch_anyway(&key);
        assert_eq!(settings(&w, &key).dropped_note, Some(1));
        tick(&mut w);
        assert_eq!(
            settings(&w, &key).dropped_note,
            Some(1),
            "told in this session already: kept"
        );
    }
    let sink = Sink::default();
    {
        let mut w = open(&t.config, &sink);
        tick(&mut w);
        let done = done(&sink);
        assert_eq!(done.len(), 1, "told again after the restart");
        assert_eq!(done[0].note_title, crate::text::migrate::dropped_title(1));
        assert_eq!(settings(&w, &key).dropped_note, None, "and then cleared");
    }
    let again = Sink::default();
    let mut w = open(&t.config, &again);
    tick(&mut w);
    assert!(done(&again).is_empty());
}

#[test]
fn switch_anyway_needs_three_failures() {
    let t = moving(1);
    let sink = Sink::default();
    let mut w = open(&t.config, &sink);
    let key = t.seeded.account.clone();
    pending(&mut w, &key);
    w.migrate_failed(&key);
    w.switch_anyway(&key);
    assert_eq!(settings(&w, &key).backend, BackendKind::Imap);
    assert_eq!(w.store.journal.pending().count(), 1);
}

#[test]
fn a_crash_after_the_bodies_went_but_before_the_rows_resumes_clearing() {
    let t = moving(2);
    let sink = Sink::default();
    {
        let w = open(&t.config, &sink);
        w.store.bodies.remove_folder(t.seeded.folder).unwrap();
        assert!(!w.store.db.folders(&t.seeded.account).unwrap().is_empty());
    }
    let mut w = open(&t.config, &sink);
    tick(&mut w);
    assert!(cleared(&w, &t));
    assert_eq!(settings(&w, &t.seeded.account).backend, BackendKind::Graph);
}

#[test]
fn a_crash_at_step_two_with_rows_and_bodies_intact_resumes() {
    let t = moving(2);
    let sink = Sink::default();
    let mut w = open(&t.config, &sink);
    tick(&mut w);
    assert!(cleared(&w, &t));
    assert!(settings(&w, &t.seeded.account).migration.is_none());
}

#[test]
fn a_crash_at_step_three_with_the_token_left_finishes_the_switch() {
    let t = moving(3);
    let sink = Sink::default();
    let key = t.seeded.account.clone();
    {
        let w = open(&t.config, &sink);
        w.store.db.remove_account_cache(&key).unwrap();
    }
    let mut w = open(&t.config, &sink);
    tick(&mut w);
    let s = settings(&w, &key);
    assert_eq!(s.backend, BackendKind::Graph);
    assert!(s.imap.is_none() && s.migration.is_none());
    assert!(
        w.store
            .tokens
            .borrow()
            .get(&key, ProviderKind::Microsoft)
            .is_none()
    );
    assert_eq!(done(&sink).len(), 1);
}

#[test]
fn an_action_pushed_after_step_one_is_dropped_with_the_notice() {
    let t = moving(2);
    let sink = Sink::default();
    let mut w = open(&t.config, &sink);
    pending(&mut w, &t.seeded.account);
    tick(&mut w);
    assert_eq!(w.store.journal.pending().count(), 0);
    assert!(!done(&sink)[0].note_title.is_empty());
}

#[test]
fn the_switch_resets_the_live_state_and_asks_for_a_full_sync() {
    let t = moving(1);
    let sink = Sink::default();
    let mut w = open(&t.config, &sink);
    w.live.get_mut(&t.seeded.account).unwrap().relist = true;
    tick(&mut w);
    let live = &w.live[&t.seeded.account];
    assert!(live.wants.full && !live.relist);
}

#[test]
fn the_switch_drops_the_stale_token_cell() {
    let t = moving(3);
    let sink = Sink::default();
    let mut w = open(&t.config, &sink);
    let key = t.seeded.account.clone();
    let refresh: crate::worker::token::Refresher = std::rc::Rc::new(|_, _| unreachable!());
    w.store.token_cells = crate::worker::token::TokenCells::with(
        vec![envio_net::oauth::Provider::microsoft_graph_template()],
        refresh,
        std::rc::Rc::new(now_ms),
        std::rc::Rc::clone(&w.store.tokens),
    );
    let cell = w
        .store
        .token_cells
        .handle(&key, ProviderKind::MicrosoftGraph)
        .expect("the test config has a Graph provider");
    cell.set_imap_ok();
    tick(&mut w);
    assert_eq!(settings(&w, &key).backend, BackendKind::Graph);
    let again = w
        .store
        .token_cells
        .handle(&key, ProviderKind::MicrosoftGraph)
        .unwrap();
    assert!(
        !again.imap_ok(),
        "a cell cached before the move must not outlive the move"
    );
}

/// A crash after each stage of the move, then a restart: the Outbox and
/// the drafts are all still there, and the move ends on Graph. At step 1 the
/// pending action also survives (it is replayed first, not dropped).
#[test]
fn a_crash_after_each_stage_keeps_the_outbox_and_drafts() {
    // (step, remove bodies, remove rows, remove the IMAP token)
    let stages = [
        (1u8, false, false, false),
        (2, false, false, false),
        (2, true, false, false),
        (2, true, true, false),
        (3, true, true, false),
        (3, true, true, true),
    ];
    for (step, bodies, rows, token) in stages {
        let t = moving(step);
        let key = t.seeded.account.clone();
        let sink = Sink::default();
        let (queued, failed, unsent) = {
            let mut w = open(&t.config, &sink);
            let queued = outbox_item(&w, &key, SendState::Queued);
            let failed = outbox_item(&w, &key, SendState::Failed { reason: "x".into() });
            let unsent = draft(&w, &key);
            if step == 1 {
                pending(&mut w, &key);
            }
            if bodies {
                w.store.bodies.remove_folder(t.seeded.folder).unwrap();
            }
            if rows {
                w.store.db.remove_account_cache(&key).unwrap();
            }
            if token {
                w.store
                    .tokens
                    .borrow_mut()
                    .remove(&key, ProviderKind::Microsoft)
                    .unwrap();
            }
            (queued, failed, unsent)
        };
        let mut w = open(&t.config, &sink);
        tick(&mut w);
        let at = format!("crash at step {step} bodies={bodies} rows={rows} token={token}");
        let ids: Vec<String> = w
            .store
            .outbox
            .items()
            .unwrap()
            .into_iter()
            .map(|i| i.id)
            .collect();
        assert!(ids.contains(&queued) && ids.contains(&failed), "{at}");
        assert!(w.store.drafts.load(&unsent).is_ok(), "{at}");
        if step == 1 {
            // Replay is asked first; nothing was dropped.
            assert_eq!(w.store.journal.pending().count(), 1, "{at}");
            assert_eq!(settings(&w, &key).backend, BackendKind::Imap, "{at}");
        } else {
            assert_eq!(settings(&w, &key).backend, BackendKind::Graph, "{at}");
            assert!(cleared(&w, &t), "{at}");
        }
    }
}

/// The IDLE future that was running at the switch ends afterwards and hands
/// its IMAP session back: it must not become the Graph account's backend.
#[test]
fn a_late_idle_does_not_hand_the_imap_session_to_graph() {
    use crate::worker::backend::{Backend, fake::Kind, fake_backend};
    use crate::worker::conn::Conn;

    let t = moving(1);
    let sink = Sink::default();
    let mut w = open(&t.config, &sink);
    let key = t.seeded.account.clone();
    let stop = std::rc::Rc::new(tokio::sync::Notify::new());
    w.live.get_mut(&key).unwrap().conn = Conn::Idling(stop);
    tick(&mut w);
    assert_eq!(settings(&w, &key).backend, BackendKind::Graph);
    let (Backend::Imap(session), _runtime) = fake_backend(Kind::Imap) else {
        unreachable!()
    };
    w.idle_done(&key, Ok((session, envio_net::imap::Woke::Stopped)));
    assert!(
        !matches!(w.live[&key].conn, Conn::Ready(Backend::Imap(_))),
        "the old IMAP session became a Graph account's connection"
    );
}

/// Sync refilled the cache after CLEAR (step 3 saved, rows back): the
/// switch empties it again.
#[test]
fn a_cache_refilled_since_the_clear_is_emptied_at_the_switch() {
    let t = moving(3);
    let sink = Sink::default();
    let mut w = open(&t.config, &sink);
    assert!(!cleared(&w, &t), "the seeded rows are the refilled cache");
    tick(&mut w);
    assert!(cleared(&w, &t));
    assert_eq!(settings(&w, &t.seeded.account).backend, BackendKind::Graph);
}

/// Open lists and the inline-image cache follow the clear.
#[test]
fn the_clear_refreshes_open_lists_and_drops_inline_images() {
    let t = moving(2);
    let sink = Sink::default();
    let mut w = open(&t.config, &sink);
    let dir = w.store.paths.inline().join("1");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.png"), b"x").unwrap();
    tick(&mut w);
    let events = sink.events();
    assert!(
        events.iter().any(
            |e| matches!(e, Event::FoldersChanged { account } if *account == t.seeded.account)
        )
    );
    assert!(events.iter().any(
        |e| matches!(e, Event::MessagesChanged { folders } if folders.contains(&t.seeded.folder))
    ));
    assert!(!dir.exists(), "the inline images stayed");
}

/// The switch leaves Graph's own `idle_capable`: no IDLE, so the poll timer
/// runs (`Live::for_account`).
#[test]
fn the_reset_live_state_is_a_graph_one() {
    let t = moving(1);
    let sink = Sink::default();
    let mut w = open(&t.config, &sink);
    tick(&mut w);
    assert_eq!(w.live[&t.seeded.account].idle_capable, Some(false));
}

#[test]
fn begin_starts_only_an_imap_account_not_already_moving() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut w = open(&config, &sink);
    assert!(w.migrate_begin(&seeded.account));
    assert_eq!(
        settings(&w, &seeded.account).migration,
        Some(Migration {
            step: 1,
            failures: 0
        })
    );
    assert!(!w.migrate_begin(&seeded.account), "already moving");
}
