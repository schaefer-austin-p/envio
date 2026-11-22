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
