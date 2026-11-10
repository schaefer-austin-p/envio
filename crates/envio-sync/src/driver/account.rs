//! One account as a whole: a sync run, and removing the account.

use envio_store::{Drafts, FolderId, SendState};

use super::Local;
use super::folders::{Synced, sync_folder, sync_folders};
use super::replay::{Replayed, replay};
use crate::error::Result;
use crate::plan::folders::sync_order;
use crate::plan::outbox::{ItemStep, item_step};
use crate::schedule::Key;
use crate::server::MailServer;

/// What a sync run did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AccountSynced {
    pub replayed: Replayed,
    /// In the order they were synced.
    pub folders: Vec<(FolderId, Synced)>,
}

/// One sync run over IMAP: the folder list, then the user's due actions
/// (so the server shows them before folders are compared), then each
/// folder, Inbox first. A replay that stopped ends the run: the session is
/// gone or the account is paused.
pub async fn sync_account(
    server: &mut impl MailServer,
    local: &mut Local<'_>,
    account: &str,
    now_ms: i64,
) -> Result<AccountSynced> {
    let folders = sync_folders(server, local, account).await?;
    let mut synced = AccountSynced {
        replayed: replay(server, local, account, now_ms).await?,
        folders: Vec::new(),
    };
    if synced.replayed.stopped.is_some() {
        return Ok(synced);
    }
    for id in sync_order(&folders) {
        let one = sync_folder(server, local, id, now_ms).await?;
        synced.folders.push((id, one));
    }
    Ok(synced)
}

/// Whether a run needs SMTP: an Outbox item of the account is Queued and
/// past its backoff.
pub fn has_due_mail(local: &Local<'_>, account: &str, now_ms: i64) -> Result<bool> {
    Ok(local.outbox.items()?.iter().any(|item| {
        item.meta.account == account
            && item_step(&item.meta.state) == ItemStep::Send
            && local.schedule.is_due(&Key::Outbox(item.id.clone()), now_ms)
    }))
}

/// The account's unsent work, for the warning before it's removed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Unsent {
    /// Outbox items not yet accepted by the server (Queued, Sending, May
    /// have been sent, Failed). Accepted ones only wait for filing.
    pub outbox: usize,
    pub drafts: usize,
    /// Actions not yet on the server.
    pub actions: usize,
}

pub fn unsent(local: &Local<'_>, drafts: &Drafts, account: &str) -> Result<Unsent> {
    Ok(Unsent {
        outbox: local
            .outbox
            .items()?
            .iter()
            .filter(|item| item.meta.account == account)
            .filter(|item| item.meta.state != SendState::Accepted)
            .count(),
        drafts: drafts
            .list()?
            .iter()
            .filter(|(_, draft)| draft.account == account)
            .count(),
        actions: local
            .journal
            .pending()
            .filter(|action| action.target.account == account)
            .count(),
    })
}

/// Removes everything Envio holds for an account: its pending actions,
/// Outbox items and drafts (core asks first, showing `unsent`), then its
/// cache and bodies. The password and pins are core's to remove.
pub fn remove_account(local: &mut Local<'_>, drafts: &Drafts, account: &str) -> Result<()> {
    let actions: Vec<u64> = local
        .journal
        .pending()
        .filter(|action| action.target.account == account)
        .map(|action| action.seq)
        .collect();
    for seq in actions {
        local.journal.done(seq)?;
        local.schedule.succeeded(&Key::Action(seq));
    }
    local.journal.compact()?;
    for item in local.outbox.items()? {
        if item.meta.account == account {
            local.outbox.remove(&item.id)?;
            local.schedule.succeeded(&Key::Outbox(item.id));
        }
    }
    for (id, draft) in drafts.list()? {
        if draft.account == account {
            drafts.delete(&id)?;
        }
    }
    for folder in local.db.step(|db| db.remove_account_cache(account))? {
        local.bodies.remove_folder(folder)?;
    }
    Ok(())
}
