//! The connect and the Inbox SELECT before IDLE, as a future the turn loop
//! selects on: commands are handled while it is in flight, and a step that
//! needs the session cancels it.

use std::future::Future;
use std::pin::Pin;

use envio_net::imap::{FolderState, ImapSession};
use futures::future::{Aborted, abortable};

use super::Worker;
use super::conn::{Conn, changed_since_sync, login, prepare_idle, start_idle, sync_first};
use super::plan::idle_problem;
use crate::clock::now_ms;
use crate::problem::{self, Problem};
use crate::servers::Service;

/// A session ready for IDLE: connected, and the Inbox selected (unless the
/// server has no IDLE).
pub(crate) struct Opened {
    session: ImapSession,
    state: Option<FolderState>,
    until_ms: i64,
}

/// `Err(Aborted)`: cancelled, not failed.
pub(crate) type OpenFuture =
    Pin<Box<dyn Future<Output = (String, Result<Result<Opened, Problem>, Aborted>)>>>;

/// What the future owns: it holds no borrow of the worker.
struct Job {
    have: Option<ImapSession>,
    login: (envio_net::Endpoint, envio_net::Credentials),
    roots: envio_net::Roots,
    inbox_path: String,
    until_ms: i64,
    #[cfg_attr(not(feature = "test-hooks"), allow(dead_code))]
    account: String,
}

async fn open_inbox(job: Job) -> Result<Opened, Problem> {
    let Job {
        have,
        login,
        roots,
        inbox_path,
        until_ms,
        account,
    } = job;
    #[cfg(not(feature = "test-hooks"))]
    drop(account);
    let to_problem = |e: envio_net::NetError| problem::from_net(&e, Service::Imap);
    let mut session = match have {
        Some(session) => session,
        None => ImapSession::open(&login.0, &roots, &login.1)
            .await
            .map_err(to_problem)?,
    };
    if !session.caps().idle {
        return Ok(Opened {
            session,
            state: None,
            until_ms,
        });
    }
    #[cfg(feature = "test-hooks")]
    crate::hooks::reached(crate::hooks::Point::IdleSelect, &account);
    let state = prepare_idle(&mut session, &inbox_path)
        .await
        .map_err(to_problem)?;
    Ok(Opened {
        session,
        state: Some(state),
        until_ms,
    })
}

impl Worker<'_> {
    /// Starts the connect and SELECT that take the account into IDLE on its
    /// Inbox. Returns at once; `open_done` follows.
    pub(crate) fn begin_open(&mut self, account: &str, until_ms: i64) -> Result<(), Problem> {
        let Some(inbox) = self.inbox(account)? else {
            return Ok(());
        };
        let Some(settings) = self.store.accounts.get(account) else {
            return Err(Problem::Local);
        };
        let credentials = login(settings, &self.store.secrets, Service::Imap)?;
        let Some(live) = self.live.get_mut(account) else {
            return Err(Problem::Local);
        };
        let have = match std::mem::replace(&mut live.conn, Conn::Closed) {
            Conn::Ready(session) => Some(session),
            Conn::Closed => None,
            other => {
                // In IDLE or already opening: nothing to start.
                live.conn = other;
                return Ok(());
            }
        };
        let work = open_inbox(Job {
            have,
            login: credentials,
            roots: self.roots.clone(),
            inbox_path: inbox.path,
            until_ms,
            account: account.to_owned(),
        });
        let (work, handle) = abortable(work);
        let name = account.to_owned();
        self.opens.push(Box::pin(async move { (name, work.await) }));
        live.conn = Conn::Opening(handle);
        Ok(())
    }

    /// The connect and SELECT ended.
    pub(crate) fn open_done(
        &mut self,
        account: &str,
        result: Result<Result<Opened, Problem>, Aborted>,
    ) {
        // Cancelled: a step took over or the account went. Not a failure, so
        // no backoff.
        let Ok(result) = result else {
            return;
        };
        // Finished just before it was cancelled: the result is stale.
        if !matches!(
            self.live.get(account).map(|live| &live.conn),
            Some(Conn::Opening(_))
        ) {
            return;
        }
        let before = self.status_of(account);
        match result {
            Ok(opened) => self.opened(account, opened),
            Err(problem) => {
                if let Some(live) = self.live.get_mut(account) {
                    live.conn = Conn::Closed;
                }
                let problem = idle_problem(problem, self.foreground);
                super::run::failed(self, account, "connect", &problem);
                self.open_failed(Some(account));
            }
        }
        self.report(account, &before);
    }

    fn opened(&mut self, account: &str, opened: Opened) {
        let Opened {
            session,
            state,
            until_ms,
        } = opened;
        let inbox = self.inbox(account).ok().flatten();
        let disk_full = self.disk_full(now_ms());
        let Some(live) = self.live.get_mut(account) else {
            return;
        };
        live.idle_capable = Some(session.caps().idle);
        let (Some(inbox), Some(state)) = (inbox, state) else {
            // No IDLE (learned just now: the planner stops asking for it),
            // or no Inbox any more. It did connect: a failure is over.
            live.health.connected(disk_full);
            live.conn = Conn::Ready(session);
            return;
        };
        let behind = changed_since_sync(inbox.sync, &state);
        let failed = live.health.failures > 0;
        live.health.connected(disk_full);
        if sync_first(failed, behind && !disk_full, live.idle_synced_first) {
            // Back after a failure, or the Inbox changed after its sync:
            // sync it first; IDLE starts on the next turn. The failures
            // stay: only an IDLE that ran clears them.
            live.idle_synced_first = true;
            live.wants.folders.insert(inbox.id);
            live.conn = Conn::Ready(session);
            return;
        }
        live.idle_synced_first = false;
        let (future, notify) = start_idle(account.to_owned(), session, until_ms, now_ms());
        self.idles.push(future);
        if let Some(live) = self.live.get_mut(account) {
            live.conn = Conn::Idling(notify);
        }
    }
}
