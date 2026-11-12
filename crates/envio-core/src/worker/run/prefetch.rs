//! The step that downloads a batch of recent bodies ahead of the open
//! (#102).

use envio_net::imap::ImapSession;
use envio_store::{FolderId, MessageRowId};
use envio_sync::SyncError;
use envio_sync::driver::{self, Prefetched};

use super::sync::{imap_error, index_body};
use crate::api::event::Event;
use crate::clock::now_ms;
use crate::problem::Problem;
use crate::worker::Worker;

/// One batch of the folder's next bodies, in the session the step has. They
/// are stored and indexed like an opened one, with no message shown.
pub(super) async fn run(
    worker: &mut Worker<'_>,
    account: &str,
    session: &mut ImapSession,
    folder: FolderId,
) -> Result<(), Problem> {
    let ids = worker.prefetch_batch(account, folder, now_ms());
    if ids.is_empty() {
        return Ok(());
    }
    let outcome = driver::prefetch_bodies(session, &mut worker.store.local(), folder, &ids).await;
    settle(worker, (account, folder), &ids, outcome)
}

/// What the batch did: the bodies it got are indexed and announced, then a
/// failure fails the step like any other, so a connection that broke
/// mid-batch is dropped and not handed to the next step.
fn settle(
    worker: &mut Worker<'_>,
    (account, folder): (&str, FolderId),
    ids: &[MessageRowId],
    outcome: Result<Prefetched, SyncError>,
) -> Result<(), Problem> {
    let batch = match outcome {
        Ok(batch) => batch,
        Err(e) => {
            let problem = imap_error(&e);
            // The folder itself was refused, not a message in it: leave the
            // folder alone while the worker lives, so there is no loop and
            // no message is blamed.
            if matches!(problem, Problem::Refused(_)) {
                log::warn!("prefetch: the folder was refused, not prefetching it");
                if let Some(live) = worker.live.get_mut(account) {
                    live.prefetch_off.insert(folder);
                }
            }
            return Err(problem);
        }
    };
    let mut failed = Vec::new();
    let mut stored = false;
    for (id, eml) in &batch.done {
        match eml {
            Some(eml) => {
                index_body(worker, *id, eml);
                stored = true;
            }
            None => failed.push(*id),
        }
    }
    give_up(worker, account, &failed);
    if stored {
        worker.emit(Event::MessagesChanged {
            folders: vec![folder],
        });
    }
    let Some(error) = batch.error else {
        return Ok(());
    };
    let problem = imap_error(&error);
    // A refusal is the message's own (the one after the done ones): it is
    // not asked for again. A lost connection is not, and the normal retry
    // applies.
    if matches!(problem, Problem::Refused(_)) {
        let next: Vec<MessageRowId> = ids.iter().skip(batch.done.len()).take(1).copied().collect();
        give_up(worker, account, &next);
    }
    Err(problem)
}

/// These messages are not candidates again while the worker lives.
fn give_up(worker: &mut Worker<'_>, account: &str, ids: &[MessageRowId]) {
    if let Some(live) = worker.live.get_mut(account) {
        live.prefetch_skip.extend(ids.iter().copied());
    }
}

#[cfg(test)]
mod tests {
    use envio_net::NetError;
    use envio_store::{Envelope, Flags, NewMessage};

    use super::*;
    use crate::worker::testing::{Sink, open, seed};

    /// A worker whose Inbox holds two recent small messages, and their ids.
    fn two_candidates<'a>(
        dir: &std::path::Path,
        sink: &'a Sink,
    ) -> (Worker<'a>, String, FolderId, Vec<MessageRowId>) {
        let (config, seeded) = seed(dir);
        let mut worker = open(&config, sink);
        for uid in [1, 2] {
            let recent = NewMessage {
                uid,
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
        }
        worker.foreground = true;
        let ids = worker.prefetch_batch(&seeded.account, seeded.folder, now_ms());
        assert_eq!(ids.len(), 2);
        (worker, seeded.account, seeded.folder, ids)
    }

    fn next(worker: &Worker<'_>, account: &str) -> Option<FolderId> {
        let folders = worker.store.db.folders(account).unwrap();
        worker.prefetch_folder(account, &folders, now_ms())
    }

    fn batch(done: Vec<(MessageRowId, Option<Vec<u8>>)>, error: Option<NetError>) -> Prefetched {
        Prefetched {
            done,
            error: error.map(SyncError::from),
        }
    }

    fn changed(sink: &Sink) -> bool {
        let events = sink.events();
        events
            .iter()
            .any(|e| matches!(e, Event::MessagesChanged { .. }))
    }

    #[test]
    fn messages_gone_from_the_server_are_planned_no_more() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, account, folder, ids) = two_candidates(dir.path(), &sink);
        assert_eq!(next(&worker, &account), Some(folder));
        let gone = ids.iter().map(|id| (*id, None)).collect();
        let outcome = Ok(batch(gone, None));
        settle(&mut worker, (&account, folder), &ids, outcome).unwrap();
        // Every candidate failed: nothing is planned, so there is no loop.
        assert_eq!(next(&worker, &account), None);
        // Nothing was stored, so no list refresh is announced.
        assert!(!changed(&sink));
    }

    #[test]
    fn a_refused_select_stops_the_folder_and_blames_no_message() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, account, folder, ids) = two_candidates(dir.path(), &sink);
        let refused = SyncError::from(NetError::Refused("no".into()));
        let result = settle(&mut worker, (&account, folder), &ids, Err(refused));
        assert!(matches!(result, Err(Problem::Refused(_))));
        assert_eq!(next(&worker, &account), None, "no loop");
        let live = &worker.live[&account];
        assert!(live.prefetch_skip.is_empty());
        assert!(live.prefetch_off.contains(&folder));
    }

    #[test]
    fn a_refused_fetch_skips_that_message_only() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, account, folder, ids) = two_candidates(dir.path(), &sink);
        let done = vec![(ids[0], Some(b"x".to_vec()))];
        let stopped = batch(done, Some(NetError::Refused("no".into())));
        let result = settle(&mut worker, (&account, folder), &ids, Ok(stopped));
        assert!(matches!(result, Err(Problem::Refused(_))));
        let live = &worker.live[&account];
        assert_eq!(live.prefetch_skip, [ids[1]].into());
    }

    #[test]
    fn a_connection_error_mid_batch_fails_the_step_after_the_done_ones_are_announced() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Sink::default();
        let (mut worker, account, folder, ids) = two_candidates(dir.path(), &sink);
        let done = vec![(ids[0], Some(b"Subject: x\r\n\r\nbody".to_vec()))];
        let stopped = batch(done, Some(NetError::Timeout));
        let result = settle(&mut worker, (&account, folder), &ids, Ok(stopped));
        assert!(matches!(result, Err(Problem::Temporary)), "{result:?}");
        assert!(changed(&sink));
        assert!(worker.live[&account].prefetch_skip.is_empty());
    }
}
