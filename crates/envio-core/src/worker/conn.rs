//! An account's connections: opening them, IDLE on the Inbox, and what the
//! foreground and background do to them.

use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::time::Duration;

use envio_net::Roots;
use envio_net::imap::{FolderState, IDLE_MAX, ImapSession, Woke};
use envio_net::smtp::SmtpSession;
use envio_store::{Account, FolderId, FolderRole, MessageRowId, SyncState};
use futures::stream::FuturesUnordered;
use tokio::sync::Notify;

use super::Worker;
use super::plan::Wants;
use crate::clock::now_ms;
use crate::health::{AccountStatus, Health};
use crate::problem::{self, Problem};
use crate::secrets::Secrets;
use crate::servers::{Service, credentials, endpoint};

// One session per account, moved in and out: its size does not matter.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Conn {
    Closed,
    Ready(ImapSession),
    /// The session is inside the IDLE future; the Notify ends it.
    Idling(Rc<Notify>),
}

/// What the worker knows about one account while it runs.
pub(crate) struct Live {
    pub health: Health,
    pub wants: Wants,
    pub next_full_ms: i64,
    pub conn: Conn,
    /// What the server's greeting said; unknown until a connection worked.
    pub idle_capable: Option<bool>,
    /// The Inbox was queued for a sync before this IDLE attempt: it is not
    /// queued again, so a sync that cannot finish never makes a loop.
    pub idle_synced_first: bool,
    /// The folder list was fetched during the current run.
    pub listed: bool,
    /// Filing in Sent was refused: the next run lists the folders even if it
    /// only sends, so a renamed or deleted Sent is found again.
    pub relist: bool,
    /// A folder with parts left: it is synced again before the others.
    pub continuing: Option<FolderId>,
    /// Messages whose body could not be prefetched: not tried again while
    /// the worker lives.
    pub prefetch_skip: BTreeSet<MessageRowId>,
    /// Folders the server refused to select for a prefetch: not prefetched
    /// again while the worker lives.
    pub prefetch_off: BTreeSet<FolderId>,
}

impl Live {
    pub fn new() -> Self {
        Self {
            health: Health::default(),
            wants: Wants::default(),
            next_full_ms: 0,
            conn: Conn::Closed,
            idle_capable: None,
            idle_synced_first: false,
            listed: false,
            relist: false,
            continuing: None,
            prefetch_skip: BTreeSet::new(),
            prefetch_off: BTreeSet::new(),
        }
    }
}

impl Live {
    /// An account as loaded: one that cannot be read is stopped for good.
    pub fn for_account(account: &Account) -> Self {
        let mut live = Self::new();
        if let Some(reason) = &account.broken {
            live.health.status = AccountStatus::Broken(reason.clone());
        }
        live
    }
}

pub(crate) type IdleFuture =
    Pin<Box<dyn Future<Output = (String, envio_net::Result<(ImapSession, Woke)>)>>>;

pub(crate) async fn connect_imap(
    account: &Account,
    secrets: &Secrets,
    roots: &Roots,
) -> Result<ImapSession, Problem> {
    let (endpoint, credentials) = login(account, secrets, Service::Imap)?;
    ImapSession::open(&endpoint, roots, &credentials)
        .await
        .map_err(|e| problem::from_net(&e, Service::Imap))
}

pub(crate) async fn connect_smtp(
    account: &Account,
    secrets: &Secrets,
    roots: &Roots,
) -> Result<SmtpSession, Problem> {
    let (endpoint, credentials) = login(account, secrets, Service::Smtp)?;
    let to_problem = |e: envio_net::NetError| problem::from_net(&e, Service::Smtp);
    let mut session = SmtpSession::connect(&endpoint, roots)
        .await
        .map_err(to_problem)?;
    session.login(&credentials).await.map_err(to_problem)?;
    Ok(session)
}

/// A bad stored pin never connects (it would trust anything); a missing
/// password is a sign-in problem.
fn login(
    account: &Account,
    secrets: &Secrets,
    service: Service,
) -> Result<(envio_net::Endpoint, envio_net::Credentials), Problem> {
    let server = match service {
        Service::Imap => &account.settings.imap,
        Service::Smtp => &account.settings.smtp,
    };
    let endpoint = endpoint(server).map_err(|_| Problem::Certificate(None))?;
    let password = secrets.get(&account.key).ok_or(Problem::SignIn)?;
    Ok((endpoint, credentials(&account.settings.username, password)))
}

/// The last step may have left another folder selected.
pub(crate) async fn prepare_idle(
    session: &mut ImapSession,
    inbox_path: &str,
) -> envio_net::Result<FolderState> {
    session.select(inbox_path).await
}

/// Whether the Inbox changed since its last sync, as the SELECT for IDLE
/// shows: IDLE only reports what happens after the SELECT, so mail that
/// came in between (while another folder was synced, say) would wait for
/// the next poll. Flag changes count only when both sides know CONDSTORE's
/// modseq.
pub(crate) fn changed_since_sync(synced: Option<SyncState>, now: &FolderState) -> bool {
    let Some(synced) = synced else {
        return true;
    };
    synced.uidvalidity != now.uid_validity
        || synced.uidnext != now.uid_next
        || matches!((synced.highestmodseq, now.highest_modseq), (Some(a), Some(b)) if a != b)
}

/// Whether the Inbox is synced before IDLE begins: only once per attempt,
/// whatever the sync did, then the poll is the backstop.
pub(crate) fn sync_first(failed: bool, behind: bool, already: bool) -> bool {
    (failed || behind) && !already
}

pub(crate) fn start_idle(
    account: String,
    session: ImapSession,
    until_ms: i64,
    now_ms: i64,
) -> (IdleFuture, Rc<Notify>) {
    let stop = Rc::new(Notify::new());
    let max = i64::try_from(IDLE_MAX.as_millis()).unwrap_or(i64::MAX);
    let wait = u64::try_from((until_ms - now_ms).clamp(1_000, max)).unwrap_or(1_000);
    let ended = Rc::clone(&stop);
    #[cfg(feature = "test-hooks")]
    let name = account.clone();
    let future = async move {
        let entered = || {
            #[cfg(feature = "test-hooks")]
            crate::hooks::reached(crate::hooks::Point::IdleEntered, &name);
        };
        let result = session
            .idle_with(Duration::from_millis(wait), ended.notified(), entered)
            .await;
        (account, result)
    };
    (Box::pin(future), stop)
}

impl Worker<'_> {
    /// The account's IMAP session: the ready one, or a new one.
    pub(crate) async fn imap(&mut self, account: &str) -> Result<ImapSession, Problem> {
        let Some(live) = self.live.get_mut(account) else {
            return Err(Problem::Local);
        };
        match std::mem::replace(&mut live.conn, Conn::Closed) {
            Conn::Ready(session) => Ok(session),
            Conn::Closed => {
                let Some(settings) = self.store.accounts.get(account) else {
                    return Err(Problem::Local);
                };
                let session = connect_imap(settings, &self.store.secrets, &self.roots).await?;
                if let Some(live) = self.live.get_mut(account) {
                    live.idle_capable = Some(session.caps().idle);
                }
                Ok(session)
            }
            Conn::Idling(notify) => {
                log::error!("a step was picked for an account in IDLE");
                live.conn = Conn::Idling(notify);
                Err(Problem::Temporary)
            }
        }
    }

    /// Takes the account into IDLE on its Inbox.
    pub(crate) async fn begin_idle(&mut self, account: &str, until_ms: i64) -> Result<(), Problem> {
        let Some(inbox) = self.inbox(account)? else {
            return Ok(());
        };
        let mut session = self.imap(account).await?;
        if !session.caps().idle {
            // Learned just now: the planner stops asking for IDLE.
            if let Some(live) = self.live.get_mut(account) {
                live.conn = Conn::Ready(session);
            }
            return Ok(());
        }
        #[cfg(feature = "test-hooks")]
        crate::hooks::reached(crate::hooks::Point::IdleSelect, account);
        let state = prepare_idle(&mut session, &inbox.path)
            .await
            .map_err(|e| problem::from_net(&e, Service::Imap))?;
        let behind = changed_since_sync(inbox.sync, &state);
        let disk_full = self.disk_full(now_ms());
        if let Some(live) = self.live.get_mut(account) {
            let failed = live.health.failures > 0;
            if sync_first(failed, behind && !disk_full, live.idle_synced_first) {
                // Back after a failure, or the Inbox changed after its sync:
                // sync it first; IDLE starts on the next turn. The failures
                // stay: only an IDLE that ran clears them.
                live.idle_synced_first = true;
                live.wants.folders.insert(inbox.id);
                live.conn = Conn::Ready(session);
                return Ok(());
            }
            live.idle_synced_first = false;
        }
        let (future, notify) = start_idle(account.to_owned(), session, until_ms, now_ms());
        self.idles.push(future);
        if let Some(live) = self.live.get_mut(account) {
            live.conn = Conn::Idling(notify);
        }
        Ok(())
    }

    fn inbox(&self, account: &str) -> Result<Option<envio_store::Folder>, Problem> {
        let folders = self
            .store
            .db
            .folders(account)
            .map_err(|e| problem::from_store(&e))?;
        Ok(folders.into_iter().find(|f| f.role == FolderRole::Inbox))
    }

    /// An IDLE ended: the session comes back, or the connection is lost.
    pub(crate) fn idle_done(
        &mut self,
        account: &str,
        result: envio_net::Result<(ImapSession, Woke)>,
    ) {
        let before = self.status_of(account);
        // A background session stays only for an account that notifies: its
        // Inbox goes back to IDLE.
        let keep = self.foreground || self.notifies(account);
        let inbox = self.inbox_of(account);
        let disk_full = self.disk_full(now_ms());
        let Some(live) = self.live.get_mut(account) else {
            return;
        };
        match result {
            Ok((session, woke)) => {
                // The IDLE ran (it did not fail at once): the backoff ends.
                live.health.ok_step("idle", disk_full);
                live.conn = if keep {
                    Conn::Ready(session)
                } else {
                    Conn::Closed
                };
                if woke == Woke::Changed {
                    match inbox {
                        Some(id) => {
                            live.wants.folders.insert(id);
                        }
                        None => live.wants.full = true,
                    }
                }
            }
            Err(error) => {
                live.conn = Conn::Closed;
                let problem = problem::from_net(&error, Service::Imap);
                super::run::failed(self, account, "idle", &problem);
                // Nobody waiting for a body of this account gets it soon.
                self.open_failed(Some(account));
            }
        }
        self.report(account, &before);
    }

    /// The app moved to the foreground or the background.
    pub(crate) fn app_active(&mut self, active: bool) {
        self.foreground = active;
        self.poster.set_foreground(active);
        if !active {
            let notify_off = &self.store.settings.notify_off;
            for (account, live) in &mut self.live {
                // An account that notifies keeps Inbox IDLE (or the session
                // that is about to start it); the others stop.
                if !notify_off.contains(account) {
                    continue;
                }
                match &live.conn {
                    Conn::Idling(notify) => notify.notify_one(),
                    Conn::Ready(_) => live.conn = Conn::Closed,
                    Conn::Closed => {}
                }
            }
            return;
        }
        // The phone may have slept through any connection's death.
        self.idles = FuturesUnordered::new();
        for live in self.live.values_mut() {
            live.conn = Conn::Closed;
            live.idle_synced_first = false;
        }
        self.refresh(None);
    }

    /// Sync now; an Offline wait ends.
    pub(crate) fn refresh(&mut self, account: Option<&str>) {
        let accounts: Vec<String> = match account {
            Some(key) => vec![key.to_owned()],
            None => self.live.keys().cloned().collect(),
        };
        for account in accounts {
            let before = self.status_of(&account);
            if let Some(live) = self.live.get_mut(&account) {
                if matches!(live.health.status, AccountStatus::Offline { .. }) {
                    live.health.resume();
                }
                live.wants.full = true;
            }
            self.report(&account, &before);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use envio_store::{AccountSettings, Security, Server};
    use secrecy::SecretString;

    fn account(pin: Option<&str>) -> Account {
        let server = |pin: Option<&str>| Server {
            host: "127.0.0.1".into(),
            port: 1,
            security: Security::Tls,
            pin: pin.map(str::to_owned),
        };
        Account {
            key: "a1".into(),
            settings: AccountSettings {
                name: "T".into(),
                email: "t@example.org".into(),
                username: "t".into(),
                imap: server(pin),
                smtp: server(None),
            },
            broken: None,
        }
    }

    fn synced(uidnext: u32, modseq: Option<u64>) -> Option<SyncState> {
        Some(SyncState {
            uidvalidity: 7,
            uidnext,
            highestmodseq: modseq,
        })
    }

    fn selected(uid_next: u32, modseq: Option<u64>) -> FolderState {
        FolderState {
            uid_validity: 7,
            uid_next,
            exists: 0,
            highest_modseq: modseq,
        }
    }

    #[test]
    fn the_inbox_is_synced_first_once_per_idle_attempt() {
        assert!(sync_first(true, false, false));
        assert!(sync_first(false, true, false));
        assert!(!sync_first(false, false, false));
        // The sync did not fix it (disk full, refused step): IDLE, no loop.
        assert!(!sync_first(true, true, true));
        assert!(!sync_first(false, true, true));
    }

    #[test]
    fn the_inbox_is_behind_when_the_select_shows_more_than_the_sync() {
        assert!(!changed_since_sync(
            synced(5, Some(9)),
            &selected(5, Some(9))
        ));
        assert!(!changed_since_sync(synced(5, None), &selected(5, Some(9))));
        assert!(!changed_since_sync(synced(5, Some(9)), &selected(5, None)));
        assert!(changed_since_sync(
            synced(5, Some(9)),
            &selected(6, Some(9))
        ));
        assert!(changed_since_sync(
            synced(5, Some(9)),
            &selected(5, Some(10))
        ));
        assert!(changed_since_sync(None, &selected(5, None)));
        let other = FolderState {
            uid_validity: 8,
            ..selected(5, None)
        };
        assert!(changed_since_sync(synced(5, None), &other));
    }

    #[tokio::test]
    async fn a_malformed_pin_stops_with_a_certificate_problem() {
        let dir = tempfile::tempdir().unwrap();
        let mut secrets = Secrets::open(dir.path()).unwrap();
        secrets
            .set("a1", SecretString::from("pw".to_owned()))
            .unwrap();
        // Port 1 refuses connections: only the pin check gives this error.
        let result = connect_imap(&account(Some("zz")), &secrets, &Roots::empty()).await;
        assert!(matches!(result, Err(Problem::Certificate(None))));
    }

    #[tokio::test]
    async fn no_password_is_a_sign_in_problem() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Secrets::open(dir.path()).unwrap();
        let result = connect_imap(&account(None), &secrets, &Roots::empty()).await;
        assert!(matches!(result, Err(Problem::SignIn)));
    }

    /// Whether going to the background woke the account's IDLE, and whether
    /// it still idles afterwards.
    fn background_wakes(notify_off: bool) -> (bool, bool) {
        use crate::worker::testing::{Sink, open, seed};
        use futures::FutureExt;

        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        if notify_off {
            worker
                .store
                .settings
                .notify_off
                .insert(seeded.account.clone());
        }
        let stop = Rc::new(Notify::new());
        worker.live.get_mut(&seeded.account).unwrap().conn = Conn::Idling(stop.clone());
        worker.app_active(false);
        let woken = stop.notified().now_or_never().is_some();
        let idling = matches!(worker.live[&seeded.account].conn, Conn::Idling(_));
        (woken, idling)
    }

    #[test]
    fn background_keeps_inbox_idle_when_notify_is_on() {
        assert_eq!(background_wakes(false), (false, true));
    }

    #[test]
    fn background_closes_when_notify_is_off() {
        // Woken, so its session comes back through `idle_done` and closes.
        assert_eq!(background_wakes(true), (true, true));
    }

    #[test]
    fn resume_reconnects_every_account() {
        use crate::worker::testing::{Sink, open, seed};

        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        worker.live.get_mut(&seeded.account).unwrap().conn = Conn::Idling(Rc::new(Notify::new()));
        worker.app_active(false);
        worker.app_active(true);
        assert!(matches!(worker.live[&seeded.account].conn, Conn::Closed));
    }

    #[test]
    fn an_idle_that_fails_right_after_starting_keeps_backing_off() {
        use crate::worker::testing::{Sink, open, seed};

        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let account = seeded.account;
        let mut delays = Vec::new();
        for _ in 0..4 {
            worker.idle_done(&account, Err(envio_net::NetError::Timeout));
            let health = &mut worker.live.get_mut(&account).unwrap().health;
            delays.push(health.failures);
            // The reconnect and the Inbox sync before IDLE work.
            health.ok_step("folder", false);
        }
        assert_eq!(delays, [1, 2, 3, 4]);
    }

    #[test]
    fn an_unknown_idle_capability_is_tried() {
        assert!(super::super::plan::may_idle(None, true));
        assert!(super::super::plan::may_idle(Some(true), true));
        assert!(!super::super::plan::may_idle(Some(false), true));
        assert!(!super::super::plan::may_idle(None, false));
    }
}
