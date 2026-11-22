//! The steps that send mail and file it in Sent.

use envio_mime::Address;
use envio_store::BackendKind;
use envio_sync::driver::{self, FileOpts};
use envio_sync::plan::outbox::files_itself;
use envio_sync::server::MailServer;

use crate::api::event::Event;
use crate::clock::now_ms;
use crate::problem::{self, Problem};
use crate::servers::Service;
use crate::worker::Worker;
use crate::worker::backend::Backend;
use crate::worker::conn::{Conn, SmtpConnectError, connect_smtp};

pub(super) async fn send(worker: &mut Worker<'_>, account: &str) -> Result<(), Problem> {
    let now = now_ms();
    let due = driver::has_due_mail(&worker.store.local(), account, now)
        .map_err(|e| problem::from_sync(&e, Service::Imap, worker.kind_of(account)))?;
    if !due {
        return Ok(());
    }
    if worker.kind_of(account) == BackendKind::Graph {
        return send_graph(worker, account, now).await;
    }
    let login = worker.login_for(account, Service::Smtp)?;
    let mut smtp = match connect_smtp(&login, &worker.roots).await {
        Ok(smtp) => smtp,
        Err(SmtpConnectError::Problem(problem)) => return Err(problem),
        Err(SmtpConnectError::SendRefused(reason)) => {
            // The server will not send for this mailbox: the due items fail
            // with its words, the account stays up and IMAP carries on.
            let sent = driver::refuse_outbox(&mut worker.store.local(), account, now, &reason)
                .map_err(|e| problem::from_sync(&e, Service::Smtp, BackendKind::Imap))?;
            if !sent.failed.is_empty() {
                worker.emit(Event::OutboxChanged);
                worker.emit_outbox();
                worker.reshow_open_invite();
            }
            return Ok(());
        }
    };
    let sent = driver::send_outbox(&mut smtp, &mut worker.store.local(), account, now).await;
    if let Err(error) = smtp.quit().await {
        log::debug!("closing SMTP: {error}");
    }
    let sent = sent.map_err(|e| problem::from_sync(&e, Service::Smtp, BackendKind::Imap))?;
    sent_notices(worker, account, sent)
}

/// A Graph account sends through the connection it syncs with: no SMTP
/// session. The connection goes back to the account after every outcome (an
/// unconfirmed send leaves it in place for the lookup that follows).
async fn send_graph(worker: &mut Worker<'_>, account: &str, now: i64) -> Result<(), Problem> {
    let mut backend = worker.backend(account).await?;
    let sent = match &mut backend {
        Backend::Graph(graph) => {
            driver::send_outbox(graph, &mut worker.store.local(), account, now).await
        }
        Backend::Imap(_) => return Err(Problem::Local),
    };
    if let Some(live) = worker.live.get_mut(account) {
        live.conn = Conn::Ready(backend);
    }
    let sent = sent.map_err(|e| problem::from_sync(&e, Service::Smtp, BackendKind::Graph))?;
    sent_notices(worker, account, sent)
}

/// What a send run changed: Sent is looked at next, the lists are told.
fn sent_notices(worker: &mut Worker<'_>, account: &str, sent: driver::Sent) -> Result<(), Problem> {
    if !(sent.accepted.is_empty() && sent.failed.is_empty() && sent.unconfirmed.is_empty()) {
        if let Some(live) = worker.live.get_mut(account) {
            // Accepted mail goes into Sent; unconfirmed is looked for there.
            live.wants.file = true;
        }
        worker.emit(Event::OutboxChanged);
        worker.emit_outbox();
        worker.reshow_open_invite();
    }
    problem::from_stopped("send or replay", sent.stopped)
}

/// Gmail and Graph file sent mail in Sent themselves.
fn files_itself_for(kind: BackendKind, smtp_host: &str) -> bool {
    kind == BackendKind::Graph || files_itself(smtp_host)
}

pub(super) async fn file(
    worker: &mut Worker<'_>,
    account: &str,
    server: &mut impl MailServer,
) -> Result<(), Problem> {
    let Some(settings) = worker.store.accounts.get(account) else {
        return Err(Problem::Local);
    };
    let kind = settings.settings.backend;
    let itself = files_itself_for(
        kind,
        settings
            .settings
            .smtp
            .as_ref()
            .map_or("", |s| s.host.as_str()),
    );
    let filed = driver::file_sent(
        server,
        &mut worker.store.local(),
        account,
        FileOpts {
            files_itself: itself,
            now_ms: now_ms(),
            retries: kind == BackendKind::Graph,
        },
    )
    .await
    .map_err(|e| problem::from_sync(&e, Service::Imap, kind))?;
    for id in &filed.removed {
        worker.asked.remove(id);
        worker.unfiled.remove(id);
    }
    if let Some(live) = worker.live.get_mut(account) {
        live.relist = filed.relist;
    }
    worker.asked.extend(filed.ask.iter().cloned());
    worker.unfiled.extend(filed.unfiled.iter().cloned());
    // A look that missed is made again at `resolve_after_ms`: the timer
    // (`on_timers`) asks for the next file run.
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
    server: &mut impl MailServer,
) -> Result<(), Problem> {
    sync_drafts_on(worker, account, server).await
}

/// `drafts` over any server. The list is told when what a draft row shows
/// changed, also when the run ends in an error: a refusal stored before a
/// lost connection must still reach it.
async fn sync_drafts_on(
    worker: &mut Worker<'_>,
    account: &str,
    server: &mut impl MailServer,
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
    let mut done = driver::DraftsSynced::default();
    let result = driver::sync_drafts_into(
        server,
        (&worker.store.db, &worker.store.drafts),
        &run,
        &mut done,
    )
    .await;
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
    if done.reasons_changed || !(done.uploaded.is_empty() && done.deleted.is_empty()) {
        worker.emit(Event::LocalDraftsChanged {
            account: account.to_owned(),
        });
    }
    result.map_err(|e| problem::from_sync(&e, Service::Imap, worker.kind_of(account)))
}

#[cfg(test)]
mod tests;
