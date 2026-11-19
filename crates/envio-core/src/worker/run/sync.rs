//! The steps that bring the cache in line with the server.

use envio_net::imap::ImapSession;
use envio_store::{Action, FolderId, MessageRowId};
use envio_sync::driver::{self, Options, Replayed};

use crate::api::event::{Event, NewMail};
use crate::clock::now_ms;
use crate::problem::{self, Problem};
use crate::servers::Service;
use crate::text;
use crate::worker::Worker;
use crate::worker::alerts::own_addresses;

pub(super) fn imap_error(error: &envio_sync::SyncError) -> Problem {
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
    account: &str,
    session: &mut ImapSession,
    id: FolderId,
) -> Result<(), Problem> {
    // One time for the whole step.
    let now = now_ms();
    let window = worker.store.settings.sync_window();
    let floor = worker.floors.get(&id).cloned();
    let validity = |worker: &Worker<'_>| {
        let folder = worker.store.db.folder(id).ok()?;
        folder.sync.map(|sync| sync.uidvalidity)
    };
    let before = validity(worker);
    let own = worker
        .store
        .accounts
        .get(account)
        .map(|a| own_addresses(&a.settings))
        .unwrap_or_default();
    let options = Options {
        now_ms: now,
        window_days: window,
        kept: floor,
        own: &own,
        window: worker
            .live
            .get_mut(account)
            .and_then(|live| live.take_window(id)),
    };
    let synced = driver::sync_folder_as(session, &mut worker.store.local(), id, options)
        .await
        .map_err(|e| imap_error(&e))?;
    // A new UIDVALIDITY reset the folder: the floor named UIDs that are gone.
    let reset = before.is_some() && validity(worker) != before;
    if reset {
        worker.floors.remove(&id);
    }
    // Also after a window change: this sync dropped the rows past the window.
    if worker.reset_pending.remove(&id) || reset {
        let event = worker.older_reset(id);
        worker.emit(event);
    }
    folder_synced(worker, account, (id, synced.more));
    if let Some(live) = worker.live.get_mut(account) {
        live.keep_window(id, synced.window);
    }
    if synced.fetched + synced.removed + synced.flags_changed > 0 {
        worker.emit(Event::MessagesChanged { folders: vec![id] });
    }
    // The sync's own count, reported once, on the call that finishes it.
    if synced.new_mail.count > 0 {
        worker.new_mail(NewMail {
            account: account.to_owned(),
            count: synced.new_mail.count,
            first: synced.new_mail.first,
        });
    }
    Ok(())
}

/// A sync of the folder worked: it has parts left or not, and a prefetch
/// the server refused for it is tried again.
pub(super) fn folder_synced(worker: &mut Worker<'_>, account: &str, (id, more): (FolderId, bool)) {
    if let Some(live) = worker.live.get_mut(account) {
        live.continuing = more.then_some(id);
        live.prefetch_off.remove(&id);
    }
}

pub(super) async fn search_server(
    worker: &mut Worker<'_>,
    account: &str,
    session: &mut ImapSession,
    (folder, query): (FolderId, &str),
) -> Result<(), Problem> {
    let ids = driver::search_server(session, &mut worker.store.local(), folder, query)
        .await
        .map_err(|e| imap_error(&e))?;
    // Mail found there may be new to the cache.
    worker.emit(Event::MessagesChanged {
        folders: vec![folder],
    });
    worker.search_found(account, query, &ids);
    Ok(())
}

/// Local search finds words in bodies: the text of the message, an HTML one
/// through its text conversion. A failure only costs that.
pub(super) fn index_body(worker: &Worker<'_>, id: MessageRowId, eml: &[u8]) {
    if let Err(e) = worker.store.db.index_body(id, &envio_mime::quote_text(eml)) {
        log::warn!("indexing a body: {e}");
    }
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
    #[cfg(feature = "test-hooks")]
    if let Some(account) = worker.account_of(id) {
        crate::hooks::reached(crate::hooks::Point::BodyFetch, &account);
    }
    let whole = worker
        .account_of(id)
        .and_then(|account| worker.live.get(&account))
        .is_some_and(|live| live.wants.whole.contains(&id));
    // The whole message of a big one is not cached (see `fetch_whole_body`),
    // so it is not indexed either.
    let kept = !whole
        || !worker
            .store
            .db
            .message(id)
            .is_ok_and(|m| driver::is_big(m.size));
    let eml = if whole {
        driver::fetch_whole_body(session, &mut worker.store.local(), id).await
    } else {
        driver::fetch_body(session, &mut worker.store.local(), id).await
    }
    .map_err(|e| imap_error(&e))?;
    if let (true, Some(eml)) = (kept, &eml) {
        index_body(worker, id, eml);
    }
    worker.emit(Event::MessagesChanged {
        folders: vec![folder],
    });
    worker.body_fetched(id, eml.as_deref(), whole);
    Ok(())
}

/// Fetches one attachment of a partial message and writes it where the user
/// asked (see `Worker::part_fetched`).
pub(super) async fn part(
    worker: &mut Worker<'_>,
    session: &mut ImapSession,
    id: MessageRowId,
    index: usize,
) -> Result<(), Problem> {
    let want = worker
        .account_of(id)
        .and_then(|account| worker.live.get(&account))
        .and_then(|live| {
            live.wants
                .parts
                .iter()
                .find(|p| p.id == id && p.index == index)
                .cloned()
        });
    let Some(want) = want else {
        return Ok(());
    };
    let raw = driver::fetch_part(
        session,
        &mut worker.store.local(),
        id,
        (&want.section, want.size),
    )
    .await
    .map_err(|e| imap_error(&e))?;
    worker.part_fetched(&want, raw.as_deref());
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
    worker.forget_deleted_drafts(account, &replayed);
    // A replay that stopped, Local included, is the account's problem.
    match replayed.stopped {
        Some(kind) => Err(problem::from_kind(kind)),
        None => Ok(()),
    }
}

impl Worker<'_> {
    /// Removes the local drafts whose Drafts-folder copy the replay has
    /// just deleted or moved to Trash, except drafts open in a window.
    pub(super) fn forget_deleted_drafts(&mut self, account: &str, replayed: &Replayed) {
        let open = self.book.open_drafts();
        if let Err(error) = driver::forget_deleted_drafts(
            (&self.store.db, &self.store.drafts),
            account,
            &replayed.committed,
            &open,
        ) {
            log::warn!("account {account}: removing drafts deleted on the server: {error}");
        }
    }

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
    fn a_sync_that_changes_flags_tells_the_app_the_count_once() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let worker = open(&config, &sink);
        let counts = |sink: &Sink| -> Vec<u32> {
            sink.events()
                .iter()
                .filter_map(|e| match e {
                    Event::AccountUnread { unread, .. } => Some(*unread),
                    _ => None,
                })
                .collect()
        };
        // What a folder sync does: write the server's flags, then tell.
        worker
            .store
            .db
            .set_flags(seeded.folder, 1, envio_store::Flags::SEEN)
            .unwrap();
        let changed = || Event::MessagesChanged {
            folders: vec![seeded.folder],
        };
        worker.emit(changed());
        worker.emit(changed());
        assert_eq!(counts(&sink), [0]);
    }

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
    #[test]
    fn a_replayed_delete_in_drafts_forgets_the_accounts_closed_drafts_only() {
        use crate::api::command::{ComposeId, DraftInput};
        use crate::compose::DRAFT_SAVE_MS;
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        worker
            .store
            .db
            .upsert_folder(&seeded.account, "Drafts", envio_store::FolderRole::Drafts)
            .unwrap();
        let draft = |account: &str, body: &str| envio_store::Draft {
            account: account.into(),
            body: body.into(),
            updated_at_ms: 5,
            ..envio_store::Draft::default()
        };
        let uploaded = |account: &str, body: &str| {
            let id = worker.store.drafts.create(&draft(account, body)).unwrap();
            worker.store.drafts.mark_uploaded(&id, "<m@x>", 5).unwrap();
            id
        };
        let closed = uploaded(&seeded.account, "closed");
        let other = uploaded("someone-else", "other account");
        // A draft in an open window, with the same Message-ID.
        let input = DraftInput {
            account: seeded.account.clone(),
            body: "open".into(),
            ..DraftInput::default()
        };
        worker.book.changed(ComposeId(7), input, 0);
        worker.book.save_due(DRAFT_SAVE_MS);
        let open_id = worker.book.open_drafts().into_iter().next().unwrap();
        let updated = worker.store.drafts.load(&open_id).unwrap().updated_at_ms;
        worker
            .store
            .drafts
            .mark_uploaded(&open_id, "<m@x>", updated)
            .unwrap();
        let replayed = Replayed {
            committed: vec![Action {
                seq: 1,
                at_ms: 0,
                target: MessageRef {
                    account: seeded.account.clone(),
                    folder: "Drafts".into(),
                    uidvalidity: 1,
                    uid: 1,
                    message_id: Some("<m@x>".into()),
                },
                kind: ActionKind::Delete,
            }],
            ..Replayed::default()
        };
        worker.forget_deleted_drafts(&seeded.account, &replayed);
        assert!(worker.store.drafts.load(&closed).is_err(), "forgotten");
        assert!(worker.store.drafts.load(&open_id).is_ok(), "open: kept");
        assert!(worker.store.drafts.load(&other).is_ok(), "other account");
    }
}
