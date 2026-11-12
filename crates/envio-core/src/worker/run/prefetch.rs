//! The step that downloads a batch of recent bodies ahead of the open
//! (#102).

use envio_net::imap::ImapSession;
use envio_store::{FolderId, MessageRowId};
use envio_sync::driver;

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
    let fetched = driver::prefetch_bodies(session, &mut worker.store.local(), folder, &ids)
        .await
        .map_err(|e| {
            let problem = imap_error(&e);
            // A refusal is the message's own: it is not asked for again. A
            // lost connection is not, and the normal retry applies.
            if matches!(problem, Problem::Refused(_)) {
                give_up(worker, account, &ids[..1]);
            }
            problem
        })?;
    let mut failed = Vec::new();
    for (id, eml) in &fetched {
        match eml {
            Some(eml) => index_body(worker, *id, eml),
            None => failed.push(*id),
        }
    }
    give_up(worker, account, &failed);
    if !fetched.is_empty() {
        worker.emit(Event::MessagesChanged {
            folders: vec![folder],
        });
    }
    Ok(())
}

/// These messages are not candidates again while the worker lives.
fn give_up(worker: &mut Worker<'_>, account: &str, ids: &[MessageRowId]) {
    if let Some(live) = worker.live.get_mut(account) {
        live.prefetch_skip.extend(ids.iter().copied());
    }
}
