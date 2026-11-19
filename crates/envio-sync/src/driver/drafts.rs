//! Keeps the server's Drafts folder in step with the local drafts: a saved
//! draft that was left is appended (flags \Draft and \Seen), its older copy
//! is deleted after that, and the copy of a draft that was sent or
//! discarded is deleted. Local drafts are not touched here beyond the
//! note of which server copy they have, except by `forget_deleted_drafts`,
//! which removes a draft whose server copy the user deleted. A failure
//! leaves everything to be retried. Copies are found by Message-ID, so no UID is kept.

use std::collections::{BTreeMap, BTreeSet};

use envio_mime::{Address, new_message_id};
use envio_net::NetError;
use envio_net::imap::Flag;
use envio_store::{Db, Draft, Drafts, FolderRole, Journal, RefusalKind, StoreError};

use crate::convert::bracketed;
use crate::draft_mail::draft_eml;
use crate::error::{ErrorKind, Result, SyncError};
use crate::plan::drafts::{can_judge, cut_copy, drafts_to_forget, needs_upload, replaced};
use crate::plan::message_id::search_id;
use crate::plan::refusal::classify;
use crate::server::MailServer;

/// What one account's draft run needs besides the server and the store.
pub struct DraftRun<'a> {
    pub account: &'a str,
    /// The sender written into the copies.
    pub from: &'a Address,
    /// Drafts of compose windows still open: not uploaded until closed.
    pub open: &'a BTreeSet<String>,
    pub now_ms: i64,
}

/// What a run did, by draft id.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DraftsSynced {
    pub uploaded: Vec<String>,
    /// Server copies of sent or discarded drafts that are gone now.
    pub deleted: Vec<String>,
    /// The server refused this one (a NO: quota, a busy mailbox): it is
    /// tried again on later runs, after a growing wait.
    pub refused: Vec<String>,
    /// The server answered this one with nonsense (a BAD): the run went on
    /// with the others; the draft is tried again after the same growing
    /// wait.
    pub failed: Vec<String>,
    /// Some draft's `(refusal, failed_at_ms)` differs after the run: what
    /// its row shows may have changed (the kind alone would miss a second
    /// refusal of the same kind after a newer save).
    pub reasons_changed: bool,
    /// The account has no Drafts folder: nothing is uploaded (core logs it).
    pub no_folder: bool,
    /// Drafts could not be selected: the cached folder list may be stale.
    pub relist: bool,
}

/// Uploads the account's changed drafts and deletes the server copies of
/// removed ones. With no folders cached yet nothing happens (the folder
/// list comes first in a run). A failure of one draft's upload is recorded
/// on that draft and the run goes on; only a lost connection (or a local
/// or sign-in problem) ends it and is returned. What was done before is
/// kept. For the change signal to survive such an error, use
/// `sync_drafts_into`.
pub async fn sync_drafts(
    server: &mut impl MailServer,
    db: &Db,
    drafts: &Drafts,
    run: &DraftRun<'_>,
) -> Result<DraftsSynced> {
    let mut done = DraftsSynced::default();
    sync_drafts_into(server, (db, drafts), run, &mut done).await?;
    Ok(done)
}

/// `sync_drafts` that writes what it did into `done` as it goes, so a run
/// that ends in an error still reports the drafts it uploaded or refused
/// and `reasons_changed` (a refusal stored before a lost connection must
/// still reach the Drafts list).
pub async fn sync_drafts_into(
    server: &mut impl MailServer,
    (db, drafts): (&Db, &Drafts),
    run: &DraftRun<'_>,
    done: &mut DraftsSynced,
) -> Result<()> {
    let before = account_reasons(drafts, run.account)?;
    let result = run_drafts(server, (db, drafts), run, done).await;
    // Judged even when the run failed; a store error here adds nothing to
    // the one already in hand.
    match account_reasons(drafts, run.account) {
        Ok(after) => done.reasons_changed = after != before,
        Err(error) if result.is_ok() => return Err(error),
        Err(_) => {}
    }
    result
}

async fn run_drafts(
    server: &mut impl MailServer,
    (db, drafts): (&Db, &Drafts),
    run: &DraftRun<'_>,
    done: &mut DraftsSynced,
) -> Result<()> {
    let folders = db.folders(run.account)?;
    if folders.is_empty() {
        return Ok(());
    }
    let notes: Vec<_> = drafts
        .deletes()?
        .into_iter()
        .filter(|note| note.account == run.account)
        .collect();
    let Some(folder) = folders.iter().find(|f| f.role == FolderRole::Drafts) else {
        // Nothing can be deleted on a server with no such folder.
        for note in &notes {
            drafts.deleted_on_server(&note.id)?;
        }
        done.no_folder = true;
        return Ok(());
    };
    let mut due: Vec<(String, Draft)> = drafts
        .list()?
        .into_iter()
        .filter(|(id, draft)| {
            draft.account == run.account
                && !run.open.contains(id)
                && needs_upload(draft, run.now_ms)
        })
        .collect();
    // The least recently tried first: a draft whose upload drops the
    // connection goes to the back, and cannot starve the others.
    due.sort_by_key(|(_, draft)| draft.server.tried_ms);
    if due.is_empty() && notes.is_empty() {
        return Ok(());
    }
    match server.select(&folder.path).await {
        Ok(_) => {}
        Err(envio_net::NetError::Refused(_)) => {
            done.relist = true;
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    }
    for (id, draft) in due {
        match upload(server, drafts, (&folder.path, &id, &draft), run).await {
            Ok(true) => done.uploaded.push(id),
            Ok(false) => {}
            Err(error) => match error.kind() {
                ErrorKind::Refused => {
                    let kind = match &error {
                        SyncError::Net(NetError::Refused(text)) => classify(text),
                        _ => RefusalKind::Other,
                    };
                    drafts.mark_failed(&id, draft.updated_at_ms, 1, kind, run.now_ms)?;
                    done.refused.push(id);
                }
                ErrorKind::Protocol => {
                    let kind = RefusalKind::Other;
                    drafts.mark_failed(&id, draft.updated_at_ms, 1, kind, run.now_ms)?;
                    done.failed.push(id);
                }
                // A lost connection ends the run and is not counted against
                // the draft: the next run retries it, after the others.
                ErrorKind::Temporary => {
                    drafts.mark_tried(&id, run.now_ms)?;
                    return Err(error);
                }
                _ => return Err(error),
            },
        }
    }
    // After the uploads: a draft deleted while one was running is covered.
    for note in drafts.deletes()? {
        if note.account != run.account {
            continue;
        }
        match remove_copies(server, &note.message_ids).await {
            Ok(()) => {
                drafts.deleted_on_server(&note.id)?;
                done.deleted.push(note.id);
            }
            Err(error) if error.kind() == ErrorKind::Refused => done.refused.push(note.id),
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// What the account's draft rows show of a refusal: the kind and when it
/// happened, by draft id.
fn account_reasons(
    drafts: &Drafts,
    account: &str,
) -> Result<BTreeMap<String, (Option<RefusalKind>, i64)>> {
    Ok(drafts
        .list()?
        .into_iter()
        .filter(|(_, d)| d.account == account)
        .map(|(id, d)| (id, (d.server.refusal, d.server.failed_at_ms)))
        .collect())
}

/// Appends the draft's text, then deletes what it replaces. The draft file
/// names the copy only after both: a cut in between leaves `pending_id`, so
/// the retry removes the appended copy and starts again, never keeping two.
/// False when the draft was deleted meanwhile (its note takes the copies).
async fn upload(
    server: &mut impl MailServer,
    drafts: &Drafts,
    (path, id, draft): (&str, &str, &Draft),
    run: &DraftRun<'_>,
) -> Result<bool> {
    if let Some(cut) = cut_copy(&draft.server) {
        remove_copies(server, &[cut.to_owned()]).await?;
    }
    let new = bracketed(&new_message_id(&run.from.email));
    if !drafts.mark_uploading(id, &new)? {
        return Ok(false);
    }
    let eml = draft_eml(draft, run.from.clone(), &new, run.now_ms);
    server
        .append(path, &[Flag::Draft, Flag::Seen], &eml)
        .await?;
    if let Some(old) = replaced(&draft.server, &new) {
        remove_copies(server, &[old.to_owned()]).await?;
    }
    Ok(drafts.mark_uploaded(id, &new, draft.updated_at_ms)?)
}

/// Deletes every live message with one of these Message-IDs from the
/// selected folder (UID EXPUNGE with UIDPLUS, else only marked \Deleted).
async fn remove_copies(server: &mut impl MailServer, ids: &[String]) -> Result<()> {
    for id in ids {
        let Some(id) = search_id(id) else { continue };
        // Drafts folders are small: a server that cannot search headers is
        // scanned whole, so a miss really is "gone".
        let uids = server.search_message_id_deep(id).await?;
        if !uids.is_empty() {
            server.delete(&uids).await?;
        }
    }
    Ok(())
}

/// Deletes the local drafts whose Drafts-folder copy a committed Delete or
/// Move has deleted or moved to Trash (see `drafts_to_forget`). The actions
/// come from the journal's forget intents, which survive a crash; they are
/// settled once every draft they name is handled, and kept for the next run
/// when one could not be removed. Returns the ids removed.
pub fn forget_deleted_drafts(
    (db, drafts): (&Db, &Drafts),
    journal: &mut Journal,
    account: &str,
    open: &BTreeSet<String>,
) -> Result<Vec<String>> {
    let committed = journal.to_forget(account);
    if committed.is_empty() {
        return Ok(Vec::new());
    }
    let folders = db.folders(account)?;
    let all = drafts.list()?;
    let mut removed = Vec::new();
    let mut settled = Vec::new();
    for action in &committed {
        // Not yet judgeable (cache rebuilt): the intent waits.
        if !can_judge(action, &folders) {
            continue;
        }
        let mut retry = false;
        let none = BTreeSet::new();
        for (id, message_id) in
            drafts_to_forget(std::slice::from_ref(action), &folders, &all, &none)
        {
            // An open window keeps its draft; the intent waits for the
            // window to close, when the uploaded_ms check judges it again.
            if open.contains(&id) {
                retry = true;
                continue;
            }
            match drafts.forget_if_uploaded(&id, &message_id) {
                Ok(true) => removed.push(id),
                Ok(false) => {}
                // A retry cannot make a corrupt file readable; it stays.
                Err(error @ StoreError::BadRecord { .. }) => {
                    log::warn!("draft {id}: left in place, it cannot be read: {error}");
                }
                Err(error) => {
                    retry = true;
                    log::warn!("draft {id}: removing it after its copy was deleted: {error}");
                }
            }
        }
        if !retry {
            settled.push(action.seq);
        }
    }
    journal.settle(&settled)?;
    Ok(removed)
}
