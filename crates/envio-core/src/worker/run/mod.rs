//! Running one step of one account, and what a failure does to the account.

mod mail;
mod sync;

use envio_net::imap::ImapSession;
use envio_store::FolderId;

use super::Worker;
use super::conn::Conn;
use super::plan::{POLL_MS, Step};
use crate::api::event::Event;
use crate::clock::now_ms;
use crate::health::Response;
use crate::problem::Problem;
use crate::text;

/// Runs `step`, then records the outcome in the account's health and wants.
pub(crate) async fn run_one(worker: &mut Worker<'_>, account: &str, step: Step) {
    let before = worker.status_of(account);
    if let Some(live) = worker.live.get_mut(account)
        && live.health.status == crate::health::AccountStatus::Ready
    {
        live.health.status = crate::health::AccountStatus::Syncing;
    }
    match run_step(worker, account, step).await {
        Ok(()) => succeeded(worker, account, step),
        Err(problem) => {
            let response = failed(worker, account, &problem);
            if matches!(
                response,
                Response::ItemError | Response::StopDownloads | Response::Rebuild
            ) {
                // Waiting would only repeat it; the next full run tries again.
                finished(worker, account, step);
            }
            if !matches!(problem, Problem::Refused(_)) || matches!(step, Step::Body(_)) {
                worker.open_failed(Some(account));
            }
            if let Problem::Refused(reason) = &problem
                && !reason.is_empty()
            {
                worker.emit(Event::Notice {
                    text: text::notice::refused(reason),
                });
            }
            if response == Response::Rebuild {
                rebuild(worker);
            }
            if response == Response::StopDownloads {
                worker.disk_full_until = Some(now_ms() + POLL_MS);
                worker.emit(Event::Notice {
                    text: text::notice::disk_full(),
                });
            }
        }
    }
    worker.report(account, &before);
}

pub(crate) async fn run_step(
    worker: &mut Worker<'_>,
    account: &str,
    step: Step,
) -> Result<(), Problem> {
    if step == Step::Send {
        return mail::send(worker, account).await;
    }
    let mut session = worker.imap(account).await?;
    let result = match step {
        Step::Folders => sync::folders(worker, account, &mut session).await,
        Step::Body(id) => sync::body(worker, &mut session, id).await,
        Step::Replay => sync::replay(worker, account, &mut session).await,
        Step::File => mail::file(worker, account, &mut session).await,
        Step::Folder(id) => sync::folder(worker, &mut session, id).await,
        Step::Send => Ok(()),
    };
    if !matches!(&result, Err(problem) if !matches!(problem, Problem::Refused(_))) {
        let changed = match step {
            Step::Folder(id) if session.take_changes() => Some(id),
            _ => None,
        };
        keep(worker, account, session, changed);
    }
    result
}

/// A session lives on unless the step broke it (a refusal doesn't). If the
/// server reported a change while the step's folder was selected, that
/// folder is synced again.
fn keep(worker: &mut Worker<'_>, account: &str, session: ImapSession, changed: Option<FolderId>) {
    if let Some(live) = worker.live.get_mut(account) {
        if let Some(id) = changed {
            live.wants.folders.insert(id);
        }
        live.conn = Conn::Ready(session);
    }
}

fn succeeded(worker: &mut Worker<'_>, account: &str, step: Step) {
    finished(worker, account, step);
    let disk_full = worker.disk_full(now_ms());
    if let Some(live) = worker.live.get_mut(account) {
        if !disk_full {
            live.health.ok();
        }
        if live.wants.is_empty() {
            live.listed = false;
        } else if live.health.status == crate::health::AccountStatus::Ready {
            live.health.status = crate::health::AccountStatus::Syncing;
        }
    }
}

/// The step's want is met (or given up): clear it.
fn finished(worker: &mut Worker<'_>, account: &str, step: Step) {
    let folders = worker.store.db.folders(account).unwrap_or_default();
    let Some(live) = worker.live.get_mut(account) else {
        return;
    };
    match step {
        Step::Folders => {
            live.listed = true;
            live.wants.expand(&folders);
        }
        Step::Body(id) => live.wants.bodies.retain(|b| *b != id),
        Step::Replay => live.wants.replay = false,
        Step::Send => live.wants.send = false,
        Step::File => live.wants.file = false,
        Step::Folder(id) => {
            live.wants.folders.remove(&id);
        }
    }
}

/// Applies a problem to the account's health. Any problem but a refusal
/// loses the connection.
pub(crate) fn failed(worker: &mut Worker<'_>, account: &str, problem: &Problem) -> Response {
    let now = now_ms();
    let Some(live) = worker.live.get_mut(account) else {
        return Response::ItemError;
    };
    let response = live.health.failed(problem, account, now);
    if !matches!(problem, Problem::Refused(_)) {
        live.conn = Conn::Closed;
        live.listed = false;
    }
    log::warn!("account {account}: {response:?}");
    response
}

/// The cache is damaged: start over from the server.
fn rebuild(worker: &mut Worker<'_>) {
    match worker.store.rebuild() {
        Ok(()) => {
            // Row ids did not survive: nothing waiting can be answered.
            worker.open_failed(None);
            worker.emit(Event::Notice {
                text: text::notice::cache_rebuilt(),
            });
            for (key, live) in &mut worker.live {
                // Folder ids did not survive.
                live.wants = super::plan::Wants {
                    full: true,
                    ..super::plan::Wants::default()
                };
                worker.sink.emit(Event::FoldersChanged {
                    account: key.clone(),
                });
            }
        }
        Err(error) => log::error!("rebuilding the cache: {error}"),
    }
}
