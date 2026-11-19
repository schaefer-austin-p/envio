//! An account's connections: opening them, IDLE on the Inbox, and what the
//! foreground and background do to them.

use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::time::Duration;

use envio_net::Roots;
use envio_net::imap::{FolderState, IDLE_MAX, ImapSession, Woke};
use envio_net::oauth::ProviderKind;
use envio_net::smtp::SmtpSession;
use envio_store::{Account, FolderId, FolderRole, MessageRowId, SyncState};
use envio_sync::driver::WindowMemo;
use futures::future::AbortHandle;
use futures::stream::FuturesUnordered;
use secrecy::ExposeSecret;
use tokio::sync::Notify;

use super::Worker;
use super::plan::Wants;
use super::token::{FRESH_MS, Granted, TokenCells, TokenHandle, bearer_send_error};
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
    /// The connect and the Inbox SELECT run in a future (`open.rs`); the
    /// handle cancels it, which drops the session it owns.
    Opening(AbortHandle),
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
    /// That folder's window search, reused by its next part (#98).
    pub window: Option<WindowMemo>,
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
            window: None,
            prefetch_skip: BTreeSet::new(),
            prefetch_off: BTreeSet::new(),
        }
    }
}

impl Live {
    /// The remembered window search for `id`'s next part. Another folder's
    /// memo stays where it is.
    pub fn take_window(&mut self, id: FolderId) -> Option<WindowMemo> {
        if self.window.as_ref().is_some_and(|memo| memo.folder == id) {
            self.window.take()
        } else {
            None
        }
    }

    /// Stores what `id`'s sync left (`None` once it finished). A finish
    /// never clears another folder's memo.
    pub fn keep_window(&mut self, id: FolderId, left: Option<WindowMemo>) {
        if left.is_some() || self.window.as_ref().is_some_and(|memo| memo.folder == id) {
            self.window = left;
        }
    }

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

/// What a connect needs, resolved without awaiting (clone-cheap handles, no
/// `&Worker` borrow), so a background future can own it.
pub(crate) enum LoginSource {
    Password(envio_net::Credentials),
    Token(TokenHandle),
}

pub(crate) struct Login {
    pub endpoint: envio_net::Endpoint,
    pub username: String,
    pub source: LoginSource,
}

/// Why an SMTP connect failed.
pub(crate) enum SmtpConnectError {
    Problem(Problem),
    /// A token account's SMTP sign-in was refused although the token is good
    /// (Authenticated SMTP is off for the mailbox): not a sign-in problem;
    /// the due Outbox items fail with this text.
    SendRefused(String),
}

impl From<Problem> for SmtpConnectError {
    fn from(problem: Problem) -> Self {
        Self::Problem(problem)
    }
}

pub(crate) async fn connect_imap(login: &Login, roots: &Roots) -> Result<ImapSession, Problem> {
    let to_problem = |e: envio_net::NetError| problem::from_net(&e, Service::Imap);
    let handle = match &login.source {
        LoginSource::Password(credentials) => {
            return ImapSession::open(&login.endpoint, roots, credentials)
                .await
                .map_err(to_problem);
        }
        LoginSource::Token(handle) => handle,
    };
    let mut refused: Option<Granted> = None;
    loop {
        let granted = match &refused {
            None => handle.access(false).await?,
            Some(used) => handle.renew(used).await?,
        };
        let credentials =
            envio_net::Credentials::bearer(&login.username, granted.token.expose_secret());
        match ImapSession::open(&login.endpoint, roots, &credentials).await {
            Ok(session) => {
                handle.set_imap_ok();
                return Ok(session);
            }
            // Only a sign-in refusal means the token may have expired; once.
            Err(envio_net::NetError::SignIn(_))
                if refused.is_none() && is_old(handle, granted.at_ms) =>
            {
                refused = Some(granted);
            }
            Err(error) => return Err(to_problem(error)),
        }
    }
}

pub(crate) async fn connect_smtp(
    login: &Login,
    roots: &Roots,
) -> Result<SmtpSession, SmtpConnectError> {
    let to_problem = |e: envio_net::NetError| problem::from_net(&e, Service::Smtp);
    let handle = match &login.source {
        LoginSource::Password(credentials) => {
            let mut session = SmtpSession::connect(&login.endpoint, roots)
                .await
                .map_err(to_problem)?;
            session.login(credentials).await.map_err(to_problem)?;
            return Ok(session);
        }
        LoginSource::Token(handle) => handle,
    };
    let mut refused: Option<Granted> = None;
    loop {
        let granted = match &refused {
            None => handle.access(false).await?,
            Some(used) => handle.renew(used).await?,
        };
        let credentials =
            envio_net::Credentials::bearer(&login.username, granted.token.expose_secret());
        let mut session = SmtpSession::connect(&login.endpoint, roots)
            .await
            .map_err(to_problem)?;
        let error = match session.login(&credentials).await {
            Ok(()) => return Ok(session),
            Err(error) => error,
        };
        let old = is_old(handle, granted.at_ms);
        let error = bearer_send_error(error, !old, handle.imap_ok());
        match error {
            envio_net::NetError::Refused(text) if is_send_refusal(&text) => {
                return Err(SmtpConnectError::SendRefused(text));
            }
            envio_net::NetError::SignIn(_) if refused.is_none() && old => refused = Some(granted),
            error => return Err(to_problem(error).into()),
        }
    }
}

/// The token the attempt used was obtained more than `FRESH_MS` ago.
fn is_old(handle: &TokenHandle, at_ms: i64) -> bool {
    handle.age_of(at_ms) > FRESH_MS
}

/// Whether `bearer_send_error` made this refusal (its text ends in the hint).
fn is_send_refusal(text: &str) -> bool {
    text.ends_with(&crate::text::outbox::smtp_disabled_hint())
}

/// A bad stored pin never connects (it would trust anything); a missing
/// password (or an OAuth account with no provider) is a sign-in problem.
pub(super) fn login(
    account: &Account,
    secrets: &Secrets,
    tokens: &TokenCells,
    service: Service,
) -> Result<Login, Problem> {
    let server = match service {
        Service::Imap => &account.settings.imap,
        Service::Smtp => &account.settings.smtp,
    };
    let endpoint = endpoint(server).map_err(|_| Problem::Certificate(None))?;
    let username = account.settings.username.clone();
    let source = if account.settings.is_oauth() {
        let handle = tokens
            .handle(&account.key, ProviderKind::Microsoft)
            .ok_or(Problem::SignIn)?;
        LoginSource::Token(handle)
    } else {
        let password = secrets.get(&account.key).ok_or(Problem::SignIn)?;
        LoginSource::Password(credentials(&username, password))
    };
    Ok(Login {
        endpoint,
        username,
        source,
    })
}

/// What a paused account's status says, again: asking it for a login changes
/// nothing about it.
pub(super) fn paused_problem(status: &AccountStatus) -> Option<Problem> {
    match status {
        AccountStatus::SignInFailed => Some(Problem::SignIn),
        AccountStatus::Certificate(details) => Some(Problem::Certificate(details.clone())),
        AccountStatus::Removing | AccountStatus::Broken(_) => Some(Problem::Local),
        _ => None,
    }
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
        #[cfg(feature = "test-hooks")]
        crate::hooks::reached(crate::hooks::Point::IdleBegin, &name);
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
            Conn::Ready(session) => return Ok(session),
            Conn::Closed => {}
            // A step needs the session: the background connect gives way
            // (a session it owns is dropped with it) and this one connects.
            Conn::Opening(handle) => handle.abort(),
            Conn::Idling(notify) => {
                log::error!("a step was picked for an account in IDLE");
                live.conn = Conn::Idling(notify);
                return Err(Problem::Temporary);
            }
        }
        let login = self.login_for(account, Service::Imap)?;
        let session = connect_imap(&login, &self.roots).await?;
        if let Some(live) = self.live.get_mut(account) {
            live.idle_capable = Some(session.caps().idle);
        }
        Ok(session)
    }

    /// The login for `service` of `account`. A paused account is not given
    /// one: nothing is refreshed or connected for it until `resume`.
    pub(crate) fn login_for(&self, account: &str, service: Service) -> Result<Login, Problem> {
        if let Some(problem) = self
            .live
            .get(account)
            .and_then(|live| paused_problem(&live.health.status))
        {
            return Err(problem);
        }
        let Some(settings) = self.store.accounts.get(account) else {
            return Err(Problem::Local);
        };
        login(
            settings,
            &self.store.secrets,
            &self.store.token_cells,
            service,
        )
    }

    pub(super) fn inbox(&self, account: &str) -> Result<Option<envio_store::Folder>, Problem> {
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
                live.health.idle_ran(disk_full);
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
        if active {
            self.poster.reset_unseen_all();
        }
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
                    Conn::Opening(handle) => {
                        handle.abort();
                        live.conn = Conn::Closed;
                    }
                    Conn::Ready(_) => live.conn = Conn::Closed,
                    Conn::Closed => {}
                }
            }
            return;
        }
        // The phone may have slept through any connection's death. A connect
        // in flight is dropped (nothing failed: no backoff), so the
        // foreground connects at once.
        self.idles = FuturesUnordered::new();
        self.opens = FuturesUnordered::new();
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
mod bearer_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use envio_store::{AccountSettings, Security, Server};
    use secrecy::SecretString;

    fn memo(folder: i64) -> WindowMemo {
        WindowMemo {
            folder: FolderId(folder),
            uidvalidity: 1,
            uidnext: 2,
            days: Some(30),
            uids: Arc::new(vec![1]),
        }
    }

    #[test]
    fn another_folders_sync_leaves_the_remembered_window_alone() {
        let mut live = Live::new();
        live.keep_window(FolderId(1), Some(memo(1)));
        // Folder 2 neither takes folder 1's memo nor clears it when done.
        assert_eq!(live.take_window(FolderId(2)), None);
        live.keep_window(FolderId(2), None);
        assert_eq!(live.take_window(FolderId(1)), Some(memo(1)));
        // The owner finishing clears it.
        live.keep_window(FolderId(1), Some(memo(1)));
        live.keep_window(FolderId(1), None);
        assert_eq!(live.window, None);
    }

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
                auth: None,
            },
            label: None,
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

    fn cells(dir: &std::path::Path) -> TokenCells {
        let tokens = crate::oauth_tokens::OAuthTokens::open(dir).unwrap();
        TokenCells::new(
            Vec::new(),
            Roots::empty(),
            std::rc::Rc::new(std::cell::RefCell::new(tokens)),
        )
    }

    #[tokio::test]
    async fn a_malformed_pin_stops_with_a_certificate_problem() {
        let dir = tempfile::tempdir().unwrap();
        let mut secrets = Secrets::open(dir.path()).unwrap();
        secrets
            .set("a1", SecretString::from("pw".to_owned()))
            .unwrap();
        // Port 1 refuses connections: only the pin check gives this error.
        let result = login(
            &account(Some("zz")),
            &secrets,
            &cells(dir.path()),
            Service::Imap,
        );
        assert!(matches!(result, Err(Problem::Certificate(None))));
    }

    #[tokio::test]
    async fn no_password_is_a_sign_in_problem() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Secrets::open(dir.path()).unwrap();
        let result = login(&account(None), &secrets, &cells(dir.path()), Service::Imap);
        assert!(matches!(result, Err(Problem::SignIn)));
    }

    #[test]
    fn a_paused_account_is_given_no_login() {
        use crate::worker::testing::{Sink, open, seed};

        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let key = &seeded.account;
        worker
            .store
            .secrets
            .set(key, secrecy::SecretString::from("pw"))
            .unwrap();
        worker.live.get_mut(key).unwrap().health.status = AccountStatus::SignInFailed;
        assert!(matches!(
            worker.login_for(key, Service::Smtp),
            Err(Problem::SignIn)
        ));
        worker.live.get_mut(key).unwrap().health.status = AccountStatus::Ready;
        assert!(worker.login_for(key, Service::Imap).is_ok());
    }

    /// The token cell of a worker with a Microsoft account, and the worker.
    fn token_worker<'s>(
        config: &mut crate::Config,
        sink: &'s crate::worker::testing::Sink,
        key: &str,
    ) -> (Worker<'s>, TokenHandle) {
        use envio_net::oauth::Provider;
        config.oauth = vec![Provider::microsoft_template()];
        let worker = crate::worker::testing::open(config, sink);
        let handle = worker
            .store
            .token_cells
            .handle(key, ProviderKind::Microsoft)
            .unwrap();
        (worker, handle)
    }

    #[test]
    fn a_failed_step_that_pauses_the_account_stops_its_token_refreshes() {
        use crate::worker::testing::{Sink, seed};
        use futures::FutureExt;

        let dir = tempfile::tempdir().unwrap();
        let (mut config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let key = &seeded.account;
        let (mut worker, handle) = token_worker(&mut config, &sink, key);
        crate::worker::run::failed(&mut worker, key, "test", &Problem::Certificate(None));
        // A refresh would end in SignIn (no stored token): this is the pause.
        let reply = handle.access(true).now_or_never();
        assert!(
            matches!(reply, Some(Err(Problem::Certificate(None)))),
            "{reply:?}"
        );
    }

    #[test]
    fn reporting_a_paused_account_stops_its_token_refreshes() {
        use crate::worker::testing::{Sink, seed};
        use futures::FutureExt;

        let dir = tempfile::tempdir().unwrap();
        let (mut config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let key = &seeded.account;
        let (mut worker, handle) = token_worker(&mut config, &sink, key);
        worker.live.get_mut(key).unwrap().health.status = AccountStatus::Certificate(None);
        worker.report(key, &AccountStatus::Ready);
        let reply = handle.access(true).now_or_never();
        assert!(
            matches!(reply, Some(Err(Problem::Certificate(None)))),
            "{reply:?}"
        );
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
    fn going_to_the_background_cancels_the_connect_of_an_account_that_does_not_notify() {
        use crate::worker::testing::{Sink, open, seed};

        let dir = tempfile::tempdir().unwrap();
        let (config, seeded) = seed(dir.path());
        let sink = Sink::default();
        let mut worker = open(&config, &sink);
        let account = seeded.account;
        let (_future, handle) = futures::future::abortable(std::future::pending::<()>());
        let seen = handle.clone();
        worker.live.get_mut(&account).unwrap().conn = Conn::Opening(handle);
        // One that notifies keeps its connect (it is about to start IDLE).
        worker.app_active(false);
        assert!(!seen.is_aborted());
        assert!(matches!(worker.live[&account].conn, Conn::Opening(_)));
        worker.store.settings.notify_off.insert(account.clone());
        worker.app_active(false);
        assert!(seen.is_aborted());
        assert!(matches!(worker.live[&account].conn, Conn::Closed));
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
