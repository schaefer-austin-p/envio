//! The steps that bring the cache in line with the server.

use envio_net::imap::ImapSession;
use envio_store::{Action, FolderId, MessageRowId};
use envio_sync::driver::{self, Replayed};

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
    let eml = driver::fetch_body(session, &mut worker.store.local(), id)
        .await
        .map_err(|e| imap_error(&e))?;
    worker.emit(Event::MessagesChanged {
        folders: vec![folder],
    });
    worker.body_fetched(id, eml.as_deref());
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
    worker.report_replayed(account, &replayed);
    // A replay that stopped, Local included, is the account's problem.
    match replayed.stopped {
        Some(kind) => Err(problem::from_kind(kind)),
        None => Ok(()),
    }
}

impl Worker<'_> {
    /// Tells the user what a replay reverted or gave up, and has the
    /// folders of those messages synced so the screen shows the server.
    pub(super) fn report_replayed(&mut self, account: &str, replayed: &Replayed) {
        let mut changed = Vec::new();
        for action in &replayed.reverted {
            self.emit(Event::Notice {
                text: text::notice::reverted(action),
            });
            changed.extend(self.folder_of(account, action));
        }
        let mut resync = changed.clone();
        for action in &replayed.given_up {
            self.emit(Event::Notice {
                text: text::notice::given_up(action),
            });
            resync.extend(self.folder_of(account, action));
        }
        if let Some(live) = self.live.get_mut(account) {
            live.wants.folders.extend(resync);
        }
        if !changed.is_empty() {
            changed.sort();
            changed.dedup();
            self.emit(Event::MessagesChanged { folders: changed });
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::testing::{Sink, open, seed};
    use envio_store::{ActionKind, MessageRef};

    #[test]
    fn a_reverted_move_has_its_folder_synced() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let action = Action {
            seq: 1,
            at_ms: 0,
            target: MessageRef {
                account: seeded.account.clone(),
                folder: "INBOX".into(),
                uidvalidity: 1,
                uid: 1,
                message_id: None,
            },
            kind: ActionKind::Move { to: "Trash".into() },
        };
        let replayed = Replayed {
            reverted: vec![action],
            ..Replayed::default()
        };
        worker.report_replayed(&seeded.account, &replayed);
        let wants = &worker.live[&seeded.account].wants;
        assert!(wants.folders.contains(&seeded.folder));
        assert!(sink.events().iter().any(
            |e| matches!(e, Event::MessagesChanged { folders } if folders == &[seeded.folder])
        ));
    }
}
