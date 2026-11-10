//! User actions: recorded in the journal and applied to the cache at once;
//! undone within the Undo window by marking them done and putting the
//! cache back.

use envio_store::{
    ActionKind, BodyState, Flags, FolderId, MessageRef, MessageRowId, NewMessage, StoreError,
};

use super::Local;
use crate::convert::row_of;
use crate::error::Result;

/// What `undo` needs to put the cache back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Undo {
    /// The journal action this undoes.
    pub seq: u64,
    restore: Restore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Restore {
    Flags {
        folder: FolderId,
        uid: u32,
        flags: Flags,
    },
    Row {
        folder: FolderId,
        row: NewMessage,
        downloaded: bool,
    },
}

/// Records a user action on a cached message: journal first (so it can't
/// be lost), then the cache, so the list shows the result at once.
pub fn record(
    local: &mut Local<'_>,
    id: MessageRowId,
    kind: ActionKind,
    now_ms: i64,
) -> Result<Undo> {
    let message = local.db.message(id)?;
    let folder = local.db.folder(message.folder)?;
    // A cached message's folder has begun syncing, so it has a UIDVALIDITY.
    let sync = folder.sync.ok_or(StoreError::NotFound)?;
    let target = MessageRef {
        account: folder.account.clone(),
        folder: folder.path.clone(),
        uidvalidity: sync.uidvalidity,
        uid: message.uid,
        message_id: message.envelope.message_id.clone(),
    };
    let action = local.journal.push(now_ms, target, kind)?;
    let restore = match &action.kind {
        ActionKind::SetFlags { add, remove } => {
            let flags = message.flags.with(*add).without(*remove);
            local
                .db
                .step(|db| db.set_flags(folder.id, message.uid, flags))?;
            Restore::Flags {
                folder: folder.id,
                uid: message.uid,
                flags: message.flags,
            }
        }
        ActionKind::Move { .. } | ActionKind::Delete => {
            local
                .db
                .step(|db| db.delete_messages(folder.id, &[message.uid]))?;
            Restore::Row {
                folder: folder.id,
                row: row_of(&message),
                downloaded: message.body == BodyState::Downloaded,
            }
        }
    };
    Ok(Undo {
        seq: action.seq,
        restore,
    })
}

/// Takes an action back if it hasn't gone to the server yet. Returns false
/// when it's too late (already replayed).
pub fn undo(local: &mut Local<'_>, undo: &Undo) -> Result<bool> {
    if !local.journal.pending().any(|action| action.seq == undo.seq) {
        return Ok(false);
    }
    local.journal.done(undo.seq)?;
    match &undo.restore {
        Restore::Flags { folder, uid, flags } => {
            local.db.step(|db| db.set_flags(*folder, *uid, *flags))?;
        }
        Restore::Row {
            folder,
            row,
            downloaded,
        } => {
            local.db.step(|db| {
                let id = db.upsert_message(*folder, row)?;
                if *downloaded {
                    db.set_body_state(id, BodyState::Downloaded)?;
                }
                Ok(())
            })?;
        }
    }
    Ok(true)
}
