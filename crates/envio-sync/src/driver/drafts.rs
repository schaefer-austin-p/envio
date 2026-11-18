//! Keeps the server's Drafts folder in step with the local drafts: a saved
//! draft that was left is appended (flags \Draft and \Seen), its older copy
//! is deleted after that, and the copy of a draft that was sent or
//! discarded is deleted. Local drafts are never touched here beyond the
//! note of which server copy they have; a failure leaves everything to be
//! retried. Copies are found by Message-ID, so no UID is kept.

use std::collections::BTreeSet;

use envio_mime::{Address, new_message_id};
use envio_net::imap::Flag;
use envio_store::{Db, Draft, Drafts, FolderRole};

use crate::convert::bracketed;
use crate::draft_mail::draft_eml;
use crate::error::{ErrorKind, Result};
use crate::plan::drafts::{cut_copy, needs_upload, replaced};
use crate::plan::message_id::search_id;
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
    /// tried again on later runs, and left alone after `GIVE_UP` refusals
    /// of the same save.
    pub refused: Vec<String>,
    /// The server answered this one with nonsense (a BAD): the run went on
    /// with the others; the draft is tried again, a few times, then left
    /// until it changes.
    pub failed: Vec<String>,
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
/// kept.
pub async fn sync_drafts(
    server: &mut impl MailServer,
    db: &Db,
    drafts: &Drafts,
    run: &DraftRun<'_>,
) -> Result<DraftsSynced> {
    let mut done = DraftsSynced::default();
    let folders = db.folders(run.account)?;
    if folders.is_empty() {
        return Ok(done);
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
        return Ok(done);
    };
    let mut due: Vec<(String, Draft)> = drafts
        .list()?
        .into_iter()
        .filter(|(id, draft)| {
            draft.account == run.account && !run.open.contains(id) && needs_upload(draft)
        })
        .collect();
    // The least recently tried first: a draft whose upload drops the
    // connection goes to the back, and cannot starve the others.
    due.sort_by_key(|(_, draft)| draft.server.tried_ms);
    if due.is_empty() && notes.is_empty() {
        return Ok(done);
    }
    match server.select(&folder.path).await {
        Ok(_) => {}
        Err(envio_net::NetError::Refused(_)) => {
            done.relist = true;
            return Ok(done);
        }
        Err(error) => return Err(error.into()),
    }
    for (id, draft) in due {
        match upload(server, drafts, (&folder.path, &id, &draft), run).await {
            Ok(true) => done.uploaded.push(id),
            Ok(false) => {}
            Err(error) => match error.kind() {
                ErrorKind::Refused => {
                    drafts.mark_failed(&id, draft.updated_at_ms, 1)?;
                    done.refused.push(id);
                }
                ErrorKind::Protocol => {
                    drafts.mark_failed(&id, draft.updated_at_ms, 1)?;
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
    Ok(done)
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
