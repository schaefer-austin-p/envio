//! The steps that send mail and file it in Sent.

use envio_mime::Address;
use envio_net::imap::ImapSession;
use envio_sync::driver;
use envio_sync::plan::outbox::files_itself;

use crate::api::event::Event;
use crate::clock::now_ms;
use crate::problem::{self, Problem};
use crate::servers::Service;
use crate::worker::Worker;
use crate::worker::conn::connect_smtp;

pub(super) async fn send(worker: &mut Worker<'_>, account: &str) -> Result<(), Problem> {
    let now = now_ms();
    let due = driver::has_due_mail(&worker.store.local(), account, now)
        .map_err(|e| problem::from_sync(&e, Service::Imap))?;
    if !due {
        return Ok(());
    }
    let Some(settings) = worker.store.accounts.get(account) else {
        return Err(Problem::Local);
    };
    let mut smtp = connect_smtp(settings, &worker.store.secrets, &worker.roots).await?;
    let sent = driver::send_outbox(&mut smtp, &mut worker.store.local(), account, now).await;
    if let Err(error) = smtp.quit().await {
        log::debug!("closing SMTP: {error}");
    }
    let sent = sent.map_err(|e| problem::from_sync(&e, Service::Smtp))?;
    if !(sent.accepted.is_empty() && sent.failed.is_empty() && sent.unconfirmed.is_empty()) {
        if let Some(live) = worker.live.get_mut(account) {
            // Accepted mail goes into Sent; unconfirmed is looked for there.
            live.wants.file = true;
        }
        worker.emit(Event::OutboxChanged);
        worker.emit_outbox();
        worker.reshow_open_invite();
    }
    match sent.stopped {
        Some(kind) => Err(problem::from_kind(kind)),
        None => Ok(()),
    }
}

pub(super) async fn file(
    worker: &mut Worker<'_>,
    account: &str,
    session: &mut ImapSession,
) -> Result<(), Problem> {
    let Some(settings) = worker.store.accounts.get(account) else {
        return Err(Problem::Local);
    };
    let itself = files_itself(&settings.settings.smtp.host);
    let filed = driver::file_sent(session, &mut worker.store.local(), account, itself)
        .await
        .map_err(|e| problem::from_sync(&e, Service::Imap))?;
    for id in &filed.removed {
        worker.asked.remove(id);
        worker.unfiled.remove(id);
    }
    if let Some(live) = worker.live.get_mut(account) {
        live.relist = filed.relist;
    }
    worker.asked.extend(filed.ask.iter().cloned());
    worker.unfiled.extend(filed.unfiled.iter().cloned());
    if !(filed.removed.is_empty() && filed.ask.is_empty() && filed.unfiled.is_empty()) {
        worker.emit(Event::OutboxChanged);
        worker.emit_outbox();
        worker.reshow_open_invite();
    }
    Ok(())
}

/// Uploads changed drafts to the account's Drafts folder and deletes the
/// server copies of sent or discarded ones.
pub(super) async fn drafts(
    worker: &mut Worker<'_>,
    account: &str,
    session: &mut ImapSession,
) -> Result<(), Problem> {
    let Some(settings) = worker.store.accounts.get(account) else {
        return Err(Problem::Local);
    };
    let name = Some(&settings.settings.name).filter(|n| !n.is_empty());
    let from = Address::new(name.map(String::as_str), &settings.settings.email);
    let open = worker.book.open_drafts();
    let run = driver::DraftRun {
        account,
        from: &from,
        open: &open,
        now_ms: now_ms(),
    };
    let done = driver::sync_drafts(session, &worker.store.db, &worker.store.drafts, &run)
        .await
        .map_err(|e| problem::from_sync(&e, Service::Imap))?;
    if done.no_folder {
        log::info!("account {account}: no Drafts folder, drafts are not uploaded");
    }
    if !(done.refused.is_empty() && done.failed.is_empty()) {
        log::warn!(
            "account {account}: the server refused {} draft(s) and failed {}",
            done.refused.len(),
            done.failed.len()
        );
    }
    if let Some(live) = worker.live.get_mut(account)
        && done.relist
    {
        live.relist = true;
    }
    Ok(())
}
