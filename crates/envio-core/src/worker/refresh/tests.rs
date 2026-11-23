use std::time::Duration;

use super::*;
use crate::health::AccountStatus;
use crate::worker::Input;
use crate::worker::testing::{Sink, add_account, open, seed};
use envio_store::FolderId;
use tokio::sync::mpsc::unbounded_channel;

fn health(status: AccountStatus) -> Health {
    Health {
        status,
        ..Health::default()
    }
}

fn full() -> Wants {
    Wants {
        full: true,
        ..Wants::default()
    }
}

fn folder() -> Wants {
    let mut wants = Wants::default();
    wants.folders.insert(FolderId(1));
    wants
}

#[test]
fn what_is_pending_wants_a_sync_and_can_run() {
    let now = 10_000;
    let ready = Health::default();
    assert!(refresh_pending(&full(), &ready, now), "full pass");
    assert!(refresh_pending(&folder(), &ready, now), "only a folder");
    assert!(!refresh_pending(&Wants::default(), &ready, now), "idle");
    // Not a sync: a send is not what a refresh waits for.
    let send = Wants {
        send: true,
        ..Wants::default()
    };
    assert!(!refresh_pending(&send, &ready, now));
    assert!(refresh_pending(
        &full(),
        &health(AccountStatus::Syncing),
        now
    ));
    assert!(refresh_pending(
        &full(),
        &health(AccountStatus::DiskFull),
        now
    ));
    let wait = |at| AccountStatus::Offline { retry_at_ms: at };
    assert!(
        !refresh_pending(&full(), &health(wait(now + 1)), now),
        "not due"
    );
    assert!(refresh_pending(&full(), &health(wait(now)), now), "due");
    let local = AccountStatus::LocalError {
        retry_at_ms: now + 1,
    };
    assert!(!refresh_pending(&full(), &health(local), now));
    for paused in [
        AccountStatus::SignInFailed,
        AccountStatus::Certificate(None),
        AccountStatus::Broken("x".into()),
        AccountStatus::Removing,
    ] {
        assert!(!refresh_pending(&full(), &health(paused), now));
    }
}

fn refreshed(sink: &Sink) -> usize {
    sink.events()
        .iter()
        .filter(|e| matches!(e, Event::Refreshed { .. }))
        .count()
}

#[test]
fn no_account_in_scope_acks_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let (config, _) = seed(dir.path());
    let sink = Sink::default();
    let mut w = open(&config, &sink);
    w.refresh(Some("nobody"), 0);
    assert_eq!(refreshed(&sink), 0, "not before the turn settles");
    w.settle_refreshes();
    assert_eq!(refreshed(&sink), 1);
    w.live.clear();
    w.refresh(None, 0);
    w.settle_refreshes();
    assert_eq!(refreshed(&sink), 2, "no accounts at all");
}

#[test]
fn a_paused_account_acks_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut w = open(&config, &sink);
    w.live.get_mut(&seeded.account).unwrap().health.status = AccountStatus::SignInFailed;
    w.refresh(Some(&seeded.account), 0);
    w.settle_refreshes();
    assert_eq!(refreshed(&sink), 1);
}

#[test]
fn an_offline_account_is_resumed_and_waits_for_its_pass() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut w = open(&config, &sink);
    let live = w.live.get_mut(&seeded.account).unwrap();
    live.health.status = AccountStatus::Offline {
        retry_at_ms: i64::MAX,
    };
    w.refresh(Some(&seeded.account), 0);
    w.settle_refreshes();
    assert_eq!(refreshed(&sink), 0);
}

#[test]
fn a_pending_account_acks_once_when_its_pass_ends() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut w = open(&config, &sink);
    w.refresh(Some(&seeded.account), 0);
    w.settle_refreshes();
    assert_eq!(refreshed(&sink), 0, "the pass has not run");
    // What `finished` does at the end of a pass.
    w.live.get_mut(&seeded.account).unwrap().wants.full = false;
    w.settle_refreshes();
    w.settle_refreshes();
    assert_eq!(refreshed(&sink), 1, "once, and settling again adds none");
}

#[test]
fn every_refresh_gets_its_own_ack() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut w = open(&config, &sink);
    w.refresh(Some(&seeded.account), 0);
    w.refresh(None, 0);
    w.settle_refreshes();
    assert_eq!(refreshed(&sink), 0);
    w.live.get_mut(&seeded.account).unwrap().wants.full = false;
    w.settle_refreshes();
    assert_eq!(refreshed(&sink), 2);
}

#[test]
fn a_refresh_waits_for_every_account_it_asked_for() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut w = open(&config, &sink);
    let other = add_account(&mut w, "other", 0);
    w.live
        .insert(other.clone(), crate::worker::conn::Live::new());
    w.refresh(None, 0);
    w.live.get_mut(&seeded.account).unwrap().wants.full = false;
    w.settle_refreshes();
    assert_eq!(refreshed(&sink), 0, "the other still syncs");
    w.live.get_mut(&other).unwrap().wants.full = false;
    w.settle_refreshes();
    assert_eq!(refreshed(&sink), 1);
}

#[test]
fn a_connect_failure_with_the_full_pass_pending_acks() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut w = open(&config, &sink);
    w.foreground = true;
    let (_keep, mut rx) = unbounded_channel::<Input>();
    w.live.get_mut(&seeded.account).unwrap().next_full_ms = i64::MAX;
    w.refresh(Some(&seeded.account), 0);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for _ in 0..6 {
        if refreshed(&sink) > 0 {
            break;
        }
        // Nothing listens on port 1: the sync's connect is refused.
        let _ = runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(10), w.turn(&mut rx)).await
        });
    }
    assert_eq!(refreshed(&sink), 1);
    let live = &w.live[&seeded.account];
    assert!(
        !refresh_pending(&live.wants, &live.health, crate::clock::now_ms()),
        "{:?}",
        live.health.status
    );
}

#[test]
fn an_account_removed_mid_pass_acks() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut w = open(&config, &sink);
    w.refresh(Some(&seeded.account), 0);
    w.settle_refreshes();
    assert_eq!(refreshed(&sink), 0);
    w.remove_account(&seeded.account, true);
    assert!(!w.live.contains_key(&seeded.account));
    w.settle_refreshes();
    assert_eq!(refreshed(&sink), 1);
}

#[test]
fn the_drafts_upload_is_a_pending_sync() {
    let now = 1_000;
    let drafts = Wants {
        drafts: true,
        ..Wants::default()
    };
    assert!(refresh_pending(&drafts, &health(AccountStatus::Ready), now));
    assert!(!refresh_pending(
        &Wants::default(),
        &health(AccountStatus::Ready),
        now
    ));
}

#[test]
fn the_ack_carries_the_ticket_of_its_refresh() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut w = open(&config, &sink);
    w.refresh(Some(&seeded.account), 3);
    w.refresh(Some(&seeded.account), 4);
    w.live.get_mut(&seeded.account).unwrap().wants.full = false;
    w.settle_refreshes();
    let tickets: Vec<u64> = sink
        .events()
        .iter()
        .filter_map(|e| match e {
            Event::Refreshed { ticket } => Some(*ticket),
            _ => None,
        })
        .collect();
    assert_eq!(tickets, vec![3, 4]);
}
