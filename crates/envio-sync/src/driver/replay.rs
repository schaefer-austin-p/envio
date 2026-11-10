//! Sends due journal actions to the server, oldest first.

use envio_net::NetError;
use envio_store::{Action, ActionKind, FolderRole};

use super::Local;
use super::folders::apply_flags;
use crate::convert::net_flags;
use crate::error::{ErrorKind, Result};
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
pub async fn replay(
    server: &mut impl MailServer,
    local: &mut Local<'_>,
    account: &str,
    now_ms: i64,
) -> Result<Replayed> {
    let due: Vec<Action> = local
        .journal
        .pending()
        .filter(|action| action.target.account == account)
        .filter(|action| is_due(action, local.schedule, now_ms))
        .cloned()
        .collect();
    let trash = local
        .db
        .folders(account)?
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
                revert(server, local, &action).await?;
                replayed.reverted.push(action);
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
