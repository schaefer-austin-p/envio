//! The steps that bring the cache in line with the server.

use envio_net::imap::ImapSession;
use envio_store::{Action, FolderId, MessageRowId};
use envio_sync::driver;

use crate::api::event::Event;
use crate::clock::now_ms;
use crate::problem::{self, Problem};
use crate::servers::Service;
use crate::text;
use crate::worker::Worker;

fn imap_error(error: &envio_sync::SyncError) -> Problem {
    problem::from_sync(error, Service::Imap)
}

fn store_error(error: &envio_store::StoreError) -> Problem {
    problem::from_store(error)
}

pub(super) async fn folders(
    worker: &mut Worker<'_>,
    account: &str,
    session: &mut ImapSession,
) -> Result<(), Problem> {
    let before = worker
        .store
        .db
        .folders(account)
        .map_err(|e| store_error(&e))?;
    let after = driver::sync_folders(session, &mut worker.store.local(), account)
        .await
        .map_err(|e| imap_error(&e))?;
    if before != after {
        worker.emit(Event::FoldersChanged {
            account: account.to_owned(),
        });
    }
    Ok(())
}

pub(super) async fn folder(
    worker: &mut Worker<'_>,
    session: &mut ImapSession,
    id: FolderId,
) -> Result<(), Problem> {
    let synced = driver::sync_folder(session, &mut worker.store.local(), id, now_ms())
        .await
        .map_err(|e| imap_error(&e))?;
    if synced.fetched + synced.removed + synced.flags_changed > 0 {
        worker.emit(Event::MessagesChanged { folders: vec![id] });
    }
    Ok(())
}

pub(super) async fn body(
    worker: &mut Worker<'_>,
    session: &mut ImapSession,
    id: MessageRowId,
) -> Result<(), Problem> {
    let folder = worker
        .store
        .db
        .message(id)
        .map_err(|e| store_error(&e))?
        .folder;
    driver::fetch_body(session, &mut worker.store.local(), id)
        .await
        .map_err(|e| imap_error(&e))?;
    worker.emit(Event::MessagesChanged {
        folders: vec![folder],
    });
    Ok(())
}

pub(super) async fn replay(
    worker: &mut Worker<'_>,
    account: &str,
    session: &mut ImapSession,
) -> Result<(), Problem> {
    let replayed = driver::replay(session, &mut worker.store.local(), account, now_ms())
        .await
        .map_err(|e| imap_error(&e))?;
    let mut changed = Vec::new();
    for action in &replayed.reverted {
        worker.emit(Event::Notice {
            text: text::notice::reverted(action),
        });
        changed.extend(worker.folder_of(account, action));
    }
    for action in &replayed.given_up {
        worker.emit(Event::Notice {
            text: text::notice::given_up(action),
        });
        // Show what the server has.
        if let Some(id) = worker.folder_of(account, action)
            && let Some(live) = worker.live.get_mut(account)
        {
            live.wants.folders.insert(id);
        }
    }
    if !changed.is_empty() {
        changed.dedup();
        worker.emit(Event::MessagesChanged { folders: changed });
    }
    // A replay that stopped, Local included, is the account's problem.
    match replayed.stopped {
        Some(kind) => Err(problem::from_kind(kind)),
        None => Ok(()),
    }
}

impl Worker<'_> {
    /// The cached folder an action's message was in.
    fn folder_of(&self, account: &str, action: &Action) -> Option<FolderId> {
        match self.store.db.folder_by_path(account, &action.target.folder) {
            Ok(folder) => folder.map(|f| f.id),
            Err(error) => {
                log::warn!("finding an action's folder: {error}");
                None
            }
        }
    }
}
