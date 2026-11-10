//! Sends due journal actions to the server, oldest first.

use std::collections::HashSet;

use envio_net::NetError;
use envio_store::{Action, ActionKind, FolderRole};

use super::Local;
use super::folders::apply_flags;
use crate::convert::net_flags;
use crate::error::{ErrorKind, Result, SyncError, net_kind};
use crate::plan::pending::Pending;
use crate::plan::replay::{
    Find, MoveRoute, Outcome, ServerOp, find, is_due, move_route, outcome, server_op,
};
use crate::schedule::Key;
use crate::server::MailServer;

/// What a replay run did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Replayed {
    /// Actions that reached the server (or whose message is gone).
    pub done: usize,
    /// Refused for good and undone locally; core shows a notice for each.
    pub reverted: Vec<Action>,
    /// Why the run stopped early, if it did. Temporary: the action waits
    /// out its backoff. Sign-in or certificate: the account pauses.
    pub stopped: Option<ErrorKind>,
}

/// Replays the account's due actions. Stops at the first error that isn't
/// the server refusing one action, since the session is gone after it.
/// With no folders cached at all (the folder list hasn't been fetched yet)
/// nothing is replayed and the actions stay pending: without a known Trash a
/// Delete would be sent as a permanent EXPUNGE.
pub async fn replay(
    server: &mut impl MailServer,
    local: &mut Local<'_>,
    account: &str,
    now_ms: i64,
) -> Result<Replayed> {
    let folders = local.db.folders(account)?;
    if folders.is_empty() {
        return Ok(Replayed::default());
    }
    let due = due_actions(local, account, now_ms);
    let trash = folders
        .into_iter()
        .find(|folder| folder.role == FolderRole::Trash)
        .map(|folder| folder.path);
    let mut replayed = Replayed::default();
    for action in due {
        let result = apply(server, &action, trash.as_deref()).await;
        let key = Key::Action(action.seq);
        match outcome(result.as_ref().copied()) {
            Outcome::Done => {
                local.journal.done(action.seq)?;
                local.schedule.succeeded(&key);
                replayed.done += 1;
            }
            Outcome::Revert => {
                local.journal.done(action.seq)?;
                local.schedule.succeeded(&key);
                // The action is done either way; a failed revert only ends
                // the run (the next sync reads the flags again).
                let reverted = revert(server, local, &action).await;
                replayed.reverted.push(action);
                if let Err(error) = reverted {
                    replayed.stopped = Some(match &error {
                        SyncError::Net(error) => net_kind(error),
                        SyncError::Store(_) => return Err(error),
                    });
                    break;
                }
            }
            Outcome::Retry => {
                local.schedule.failed(key, now_ms);
                replayed.stopped = Some(ErrorKind::Temporary);
                break;
            }
            Outcome::Stop(kind) => {
                replayed.stopped = Some(kind);
                break;
            }
        }
    }
    if replayed.done > 0 || !replayed.reverted.is_empty() {
        local.journal.compact()?;
    }
    Ok(replayed)
}

/// Identifies a message across actions: its Message-ID, else its place.
#[derive(Debug, PartialEq, Eq, Hash)]
enum MessageKey<'a> {
    Id(&'a str),
    Place(&'a str, u32, u32),
}

fn message_key(action: &Action) -> MessageKey<'_> {
    let target = &action.target;
    match &target.message_id {
        Some(id) => MessageKey::Id(id),
        None => MessageKey::Place(&target.folder, target.uidvalidity, target.uid),
    }
}

/// The account's actions that may go now, oldest first. One that waits (its
/// Undo window or a backoff) holds back later actions on the same message,
/// so they never overtake it.
fn due_actions(local: &Local<'_>, account: &str, now_ms: i64) -> Vec<Action> {
    let mut held = HashSet::new();
    let mut due = Vec::new();
    for action in local.journal.pending() {
        if action.target.account != account {
            continue;
        }
        let key = message_key(action);
        if held.contains(&key) {
            continue;
        }
        if is_due(action, local.schedule, now_ms) {
            due.push(action.clone());
        } else {
            held.insert(key);
        }
    }
    due
}

/// Runs one action. A message or folder that's gone counts as done: there
/// is nothing left to change.
async fn apply(
    server: &mut impl MailServer,
    action: &Action,
    trash: Option<&str>,
) -> std::result::Result<(), NetError> {
    let state = match server.select(&action.target.folder).await {
        Err(NetError::Refused(_)) => return Ok(()),
        selected => selected?,
    };
    let uid = match find(&action.target, state.uid_validity) {
        Find::Uid(uid) => uid,
        Find::ByMessageId(id) => match server.search_message_id(&id).await?.first() {
            Some(uid) => *uid,
            None => return Ok(()),
        },
        Find::Lost => return Ok(()),
    };
    match server_op(action, trash) {
        ServerOp::SetFlags { add, remove } => {
            for (set, flags) in [(true, add), (false, remove)] {
                let flags = net_flags(flags);
                if !flags.is_empty() {
                    server.store_flags(&[uid], set, &flags).await?;
                }
            }
            Ok(())
        }
        ServerOp::Move { to } => move_message(server, action, uid, &to).await,
        ServerOp::Expunge => server.delete(&[uid]).await,
    }
}

/// Without MOVE, a retry after a cut between COPY and the delete would
/// copy twice, so look in the target first.
async fn move_message(
    server: &mut impl MailServer,
    action: &Action,
    uid: u32,
    to: &str,
) -> std::result::Result<(), NetError> {
    let route = move_route(server.caps().r#move, action.target.message_id.as_deref());
    let MoveRoute::CheckTarget(message_id) = route else {
        return server.move_to(&[uid], to).await;
    };
    server.select(to).await?;
    let copied = !server.search_message_id(&message_id).await?.is_empty();
    server.select(&action.target.folder).await?;
    if copied {
        server.delete(&[uid]).await
    } else {
        server.move_to(&[uid], to).await
    }
}

/// Puts the cache back after the server refused an action. A flag change
/// takes the server's flags again; a moved or deleted row comes back on
/// the folder's next sync, as nothing hides it any more.
async fn revert(server: &mut impl MailServer, local: &Local<'_>, action: &Action) -> Result<()> {
    let ActionKind::SetFlags { .. } = action.kind else {
        return Ok(());
    };
    let target = &action.target;
    let Some(folder) = local.db.folder_by_path(&target.account, &target.folder)? else {
        return Ok(());
    };
    if folder.sync.map(|sync| sync.uidvalidity) != Some(target.uidvalidity) {
        return Ok(());
    }
    let updates = server.fetch_flags(&[target.uid], None).await?;
    let pending = Pending::of(
        local.journal.pending(),
        &target.account,
        &target.folder,
        target.uidvalidity,
    );
    apply_flags(local, folder.id, &updates, &pending)?;
    Ok(())
}
