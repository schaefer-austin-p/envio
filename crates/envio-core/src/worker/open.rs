//! The connect and the Inbox SELECT before IDLE, as a future the turn loop
//! selects on: commands are handled while it is in flight, and a step that
//! needs the session cancels it.

use std::future::Future;
use std::pin::Pin;

use envio_net::imap::{FolderState, ImapSession};
use futures::StreamExt;
use futures::future::{Aborted, abortable};
use futures::stream::FuturesUnordered;

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

/// What an `OpenFuture` ends with.
pub(crate) type OpenEnd = (String, Result<Result<Opened, Problem>, Aborted>);

/// Awaits `run` while the connects in `opens` are polled too. Their results
/// come back with it, not applied: the caller does that after `run`.
pub(crate) async fn polling<T>(
    opens: &mut FuturesUnordered<OpenFuture>,
    run: impl Future<Output = T>,
) -> (T, Vec<OpenEnd>) {
    let mut ended = Vec::new();
    tokio::pin!(run);
    loop {
        tokio::select! {
            out = &mut run => return (out, ended),
            Some(end) = opens.next(), if !opens.is_empty() => ended.push(end),
        }
    }
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

#[cfg(test)]
mod tests {
    use futures::future::{AbortHandle, abortable};

    use super::*;
    use crate::worker::testing::{Sink, open, seed};

    /// A handle of a connect still in flight.
    fn in_flight() -> AbortHandle {
        let (_future, handle) = abortable(std::future::pending::<()>());
        handle
    }

    /// The `Aborted` a cancelled connect ends with.
    fn aborted() -> Aborted {
        let (future, handle) = abortable(std::future::pending::<()>());
        handle.abort();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        runtime.block_on(future).unwrap_err()
    }

    #[test]
    fn a_cancelled_connect_leaves_health_alone() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let account = seeded.account;
        worker.live.get_mut(&account).unwrap().conn = Conn::Opening(in_flight());
        let seen = sink.events().len();
        worker.open_done(&account, Err(aborted()));
        let live = &worker.live[&account];
        assert_eq!(live.health.failures, 0);
        assert!(live.health.can_run(0));
        assert_eq!(sink.events().len(), seen);
    }

    #[test]
    fn a_failed_connect_backs_off_and_closes() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let account = seeded.account;
        worker.live.get_mut(&account).unwrap().conn = Conn::Opening(in_flight());
        worker.open_done(&account, Ok(Err(Problem::Temporary)));
        let live = &worker.live[&account];
        assert_eq!(live.health.failures, 1);
        assert!(live.health.wake_at().is_some());
        assert!(matches!(live.conn, Conn::Closed));
    }

    #[test]
    fn a_result_after_the_connect_was_given_up_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let account = seeded.account;
        // Not `Opening` any more: a step took the connection over.
        worker.open_done(&account, Ok(Err(Problem::Temporary)));
        assert_eq!(worker.live[&account].health.failures, 0);
    }

    #[test]
    fn removing_an_account_cancels_its_connect() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let account = seeded.account;
        let handle = in_flight();
        let seen = handle.clone();
        worker.live.get_mut(&account).unwrap().conn = Conn::Opening(handle);
        assert!(!seen.is_aborted());
        worker.remove_account(&account, true);
        assert!(seen.is_aborted());
    }

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(future)
    }

    #[test]
    fn a_step_that_needs_imap_cancels_the_connect() {
        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let account = seeded.account;
        let handle = in_flight();
        let seen = handle.clone();
        worker.live.get_mut(&account).unwrap().conn = Conn::Opening(handle);
        // No password is stored: it ends at once, after giving way.
        let _ = block_on(worker.imap(&account));
        assert!(seen.is_aborted());
    }

    #[test]
    fn a_send_step_leaves_the_connect_running() {
        use crate::worker::plan::Step;

        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let account = seeded.account;
        let handle = in_flight();
        let seen = handle.clone();
        worker.live.get_mut(&account).unwrap().conn = Conn::Opening(handle);
        let _ = block_on(crate::worker::run::run_step(
            &mut worker,
            &account,
            &Step::Send,
        ));
        assert!(!seen.is_aborted());
        assert!(matches!(worker.live[&account].conn, Conn::Opening(_)));
    }

    #[test]
    fn connects_are_polled_while_a_step_runs() {
        use std::cell::Cell;
        use std::rc::Rc;
        use std::time::Duration;

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            // Paused: the clock only moves when everything waits, so the
            // 10 ms timer always fires before the 60 ms one.
            .start_paused(true)
            .build()
            .unwrap();
        let connected = Rc::new(Cell::new(false));
        let flag = Rc::clone(&connected);
        let (connect, _handle) = abortable(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            flag.set(true);
            Err(Problem::Temporary)
        });
        let mut opens = FuturesUnordered::new();
        opens.push(Box::pin(async move { ("b".to_owned(), connect.await) }) as OpenFuture);
        let seen = Rc::clone(&connected);
        // The step takes longer than the connect and never polls it itself.
        let step = async move {
            tokio::time::sleep(Duration::from_millis(60)).await;
            seen.get()
        };
        let (connected_in_step, ended) = runtime.block_on(polling(&mut opens, step));
        // Polled to its end during the step, and handed back, not applied.
        assert!(connected_in_step);
        assert_eq!(ended.len(), 1);
        assert_eq!(ended[0].0, "b");
        assert!(matches!(ended[0].1, Ok(Err(Problem::Temporary))));
        assert!(opens.is_empty());
    }

    #[test]
    fn a_connect_timeout_is_seen_while_a_long_step_runs() {
        use std::cell::Cell;
        use std::rc::Rc;
        use std::time::Duration;

        use tokio::time::{Instant, timeout};

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .unwrap();
        runtime.block_on(async {
            let began = Instant::now();
            let timed_out_at = Rc::new(Cell::new(None));
            let record = Rc::clone(&timed_out_at);
            // A connect that stalls: the 20 s of `CONNECT_TIMEOUT`.
            let (connect, _handle) = abortable(async move {
                let stalled = timeout(Duration::from_secs(20), std::future::pending::<()>());
                let result = stalled.await;
                record.set(Some(began.elapsed()));
                let _ = result;
                Err::<Opened, _>(Problem::Temporary)
            });
            let mut opens = FuturesUnordered::new();
            opens.push(Box::pin(async move { ("b".to_owned(), connect.await) }) as OpenFuture);
            // Another account's step runs for 90 s.
            let step = tokio::time::sleep(Duration::from_secs(90));
            let ((), ended) = polling(&mut opens, step).await;
            // The timeout fired at 20 s, not when the step ended, and the
            // failure is handed back to be applied.
            assert_eq!(timed_out_at.get(), Some(Duration::from_secs(20)));
            assert_eq!(ended.len(), 1);
            assert!(matches!(ended[0].1, Ok(Err(Problem::Temporary))));
        });
    }
}
