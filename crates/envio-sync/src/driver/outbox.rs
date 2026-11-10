//! Sends queued Outbox items and files them in Sent. An item leaves the
//! Outbox only when the server accepted it and it is in Sent, or when its
//! Message-ID turns up in Sent. Nothing is ever resent on its own.

use envio_net::imap::Flag;
use envio_store::{FolderRole, OutboxItem, SendState};

use super::Local;
use crate::error::{ErrorKind, Result};
use crate::plan::outbox::{ItemStep, after_send, item_step};
use crate::schedule::Key;
use crate::server::{MailSender, MailServer};

/// What a send run did, by Outbox item id.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Sent {
    pub accepted: Vec<String>,
    /// Refused for good: shown as failed with the server's reason.
    pub failed: Vec<String>,
    /// The connection broke after the message was handed over: it may have
    /// been sent. `file_sent` looks for it in Sent.
    pub unconfirmed: Vec<String>,
    /// Temporary: the item waits out its backoff. Sign-in or certificate:
    /// the account pauses.
    pub stopped: Option<ErrorKind>,
}

/// Sends the account's due Queued items, oldest first. Stops at the first
/// error: the SMTP session is not reused after one.
pub async fn send_outbox(
    sender: &mut impl MailSender,
    local: &mut Local<'_>,
    account: &str,
    now_ms: i64,
) -> Result<Sent> {
    let mut sent = Sent::default();
    for item in items(local, account)? {
        let key = Key::Outbox(item.id.clone());
        if item_step(&item.meta.state) != ItemStep::Send || !local.schedule.is_due(&key, now_ms) {
            continue;
        }
        let eml = local.outbox.eml(&item.id)?;
        local.outbox.set_state(&item.id, SendState::Sending)?;
        let result = sender
            .send(&item.meta.from, &item.meta.recipients, &eml)
            .await;
        let after = after_send(&result);
        local.outbox.set_state(&item.id, after.state.clone())?;
        match after.state {
            SendState::Accepted => sent.accepted.push(item.id.clone()),
            SendState::Failed { .. } => sent.failed.push(item.id.clone()),
            SendState::MayHaveBeenSent => sent.unconfirmed.push(item.id.clone()),
            SendState::Queued | SendState::Sending => {}
        }
        if after.retry {
            local.schedule.failed(key, now_ms);
            sent.stopped = Some(ErrorKind::Temporary);
        } else {
            local.schedule.succeeded(&key);
            sent.stopped = after.stop;
        }
        if result.is_err() {
            break;
        }
    }
    Ok(sent)
}

/// What filing did, by Outbox item id.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Filed {
    /// Done: in Sent and out of the Outbox.
    pub removed: Vec<String>,
    /// May have been sent and not found in Sent: the user decides (Send
    /// again, or Move to Sent).
    pub ask: Vec<String>,
}

/// Files accepted items in Sent (APPEND, unless the server does it itself
/// or the account has no Sent folder) and looks in Sent for items that may
/// have been sent. A retried APPEND first checks Sent, so a cut never
/// files a message twice.
pub async fn file_sent(
    server: &mut impl MailServer,
    local: &mut Local<'_>,
    account: &str,
    files_itself: bool,
) -> Result<Filed> {
    let sent_folder = local
        .db
        .folders(account)?
        .into_iter()
        .find(|folder| folder.role == FolderRole::Sent)
        .map(|folder| folder.path);
    let mut filed = Filed::default();
    for item in items(local, account)? {
        let step = item_step(&item.meta.state);
        if !matches!(step, ItemStep::File | ItemStep::Resolve) {
            continue;
        }
        let search = step == ItemStep::Resolve || !files_itself;
        let done = match &sent_folder {
            Some(path) if search => {
                server.select(path).await?;
                // Only an accepted item is appended; a May have been sent
                // one is only looked for.
                let eml = match step {
                    ItemStep::File => Some(local.outbox.eml(&item.id)?),
                    _ => None,
                };
                match settle(server, &item, eml.as_deref(), path).await {
                    Ok(done) => done,
                    // One item's refusal (quota, a Message-ID the server
                    // won't take) leaves it in the Outbox, untouched, and
                    // doesn't hold up the others.
                    Err(error) if error.kind() == ErrorKind::Refused => continue,
                    Err(error) => return Err(error),
                }
            }
            // Gmail files it itself; with no Sent folder there's nowhere
            // to file it. Only an accepted item counts as done then.
            _ => step == ItemStep::File,
        };
        if done {
            local.outbox.remove(&item.id)?;
            filed.removed.push(item.id);
        } else {
            filed.ask.push(item.id);
        }
    }
    Ok(filed)
}

/// Looks for the item in the selected Sent folder and appends an accepted
/// one (`append` is its text) that isn't there. True when it is now in Sent.
async fn settle(
    server: &mut impl MailServer,
    item: &OutboxItem,
    append: Option<&[u8]>,
    path: &str,
) -> Result<bool> {
    // Without a usable id there is nothing to look for: a substring search
    // for "" would match any message.
    let found = usable_message_id(&item.meta.message_id)
        && !server
            .search_message_id(&item.meta.message_id)
            .await?
            .is_empty();
    if let (false, Some(eml)) = (found, append) {
        server.append(path, &[Flag::Seen], eml).await?;
    }
    Ok(found || append.is_some())
}

/// `<x>` with something inside: the only shape searched for in Sent.
fn usable_message_id(id: &str) -> bool {
    id.trim()
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
        .is_some_and(|inner| !inner.is_empty())
}

/// The account's readable Outbox items, oldest first.
fn items(local: &Local<'_>, account: &str) -> Result<Vec<OutboxItem>> {
    let mut items: Vec<OutboxItem> = local
        .outbox
        .items()?
        .into_iter()
        .filter(|item| item.meta.account == account)
        .collect();
    items.sort_by_key(|item| item.meta.queued_at_ms);
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::usable_message_id;

    #[test]
    fn only_an_angle_bracketed_id_is_searched_for() {
        assert!(usable_message_id("<a@b>"));
        assert!(usable_message_id(" <a@b> "));
        for bad in ["", "  ", "<>", "a@b", "<a@b", "a@b>"] {
            assert!(!usable_message_id(bad), "{bad:?}");
        }
    }
}
