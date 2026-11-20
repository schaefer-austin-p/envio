//! A manual refresh lifts the draft retry wait; a foreground resume does not.

use envio_store::{Draft, RefusalKind};

use super::*;
use crate::worker::testing::{Sink, open, seed};

/// A draft whose save was refused twice, the last time at 9 000 ms.
fn waiting_draft(w: &Worker<'_>, account: &str) -> String {
    let draft = Draft {
        account: account.into(),
        to: vec!["Bob <bob@example.org>".into()],
        body: "body".into(),
        updated_at_ms: 5_000,
        ..Draft::default()
    };
    let id = w.store.drafts.create(&draft).unwrap();
    w.store
        .drafts
        .mark_failed(&id, 5_000, 2, RefusalKind::OverQuota, 9_000)
        .unwrap();
    assert_eq!(failed_at(w, &id), 9_000);
    id
}

fn failed_at(w: &Worker<'_>, id: &str) -> i64 {
    w.store.drafts.load(id).unwrap().server.failed_at_ms
}

fn refresh(w: &mut Worker<'_>, account: Option<String>) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    runtime.block_on(handle(w, Command::Refresh { account }, 0));
}

#[test]
fn a_manual_refresh_lifts_the_wait_and_keeps_the_count_and_kind() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut w = open(&config, &sink);
    let id = waiting_draft(&w, &seeded.account);
    let other = waiting_draft(&w, "elsewhere");
    refresh(&mut w, Some(seeded.account.clone()));
    assert_eq!(failed_at(&w, &id), 0);
    let kept = w.store.drafts.load(&id).unwrap().server;
    assert_eq!(
        (kept.failures, kept.refusal),
        (2, Some(RefusalKind::OverQuota))
    );
    assert_eq!(failed_at(&w, &other), 9_000, "another account is left");
}

#[test]
fn a_refresh_of_every_account_lifts_the_wait_too() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut w = open(&config, &sink);
    let id = waiting_draft(&w, &seeded.account);
    refresh(&mut w, None);
    assert_eq!(failed_at(&w, &id), 0);
}

#[test]
fn a_foreground_resume_leaves_the_wait() {
    let dir = tempfile::tempdir().unwrap();
    let (config, seeded) = seed(dir.path());
    let sink = Sink::default();
    let mut w = open(&config, &sink);
    let id = waiting_draft(&w, &seeded.account);
    w.refresh(None);
    w.refresh(Some(&seeded.account));
    assert_eq!(failed_at(&w, &id), 9_000);
}
