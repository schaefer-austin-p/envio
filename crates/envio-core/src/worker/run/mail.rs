//! The steps that send mail and file it in Sent.

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
    worker.asked.extend(filed.ask.iter().cloned());
    worker.unfiled.extend(filed.unfiled.iter().cloned());
    if !(filed.removed.is_empty() && filed.ask.is_empty() && filed.unfiled.is_empty()) {
        worker.emit(Event::OutboxChanged);
    }
    Ok(())
}
