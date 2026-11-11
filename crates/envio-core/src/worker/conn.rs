//! An account's connections: opening them, IDLE on the Inbox, and what the
//! foreground and background do to them.

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::time::Duration;

use envio_net::Roots;
use envio_net::imap::{IDLE_MAX, ImapSession, Woke};
use envio_net::smtp::SmtpSession;
use envio_store::{Account, FolderRole};
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
    pub idle_capable: bool,
    /// The folder list was fetched during the current run.
    pub listed: bool,
}

impl Live {
    pub fn new() -> Self {
        Self {
            health: Health::default(),
            wants: Wants::default(),
            next_full_ms: 0,
            conn: Conn::Closed,
            idle_capable: false,
            listed: false,
        }
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
) -> envio_net::Result<()> {
    session.select(inbox_path).await.map(drop)
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
    let future = async move {
        let result = session
            .idle(Duration::from_millis(wait), ended.notified())
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
                    live.idle_capable = session.caps().idle;
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
        prepare_idle(&mut session, &inbox.path)
            .await
            .map_err(|e| problem::from_net(&e, Service::Imap))?;
        if let Some(live) = self.live.get_mut(account)
            && live.health.failures > 0
        {
            // Back after a failure: mail may have arrived meanwhile, so
            // sync the Inbox first; IDLE starts on the next turn.
            live.health.ok();
            live.wants.folders.insert(inbox.id);
            live.conn = Conn::Ready(session);
            return Ok(());
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
        let foreground = self.foreground;
        let inbox = self.inbox_of(account);
        let Some(live) = self.live.get_mut(account) else {
            return;
        };
        match result {
            Ok((session, woke)) => {
                live.conn = if foreground {
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
                super::run::failed(self, account, &problem);
            }
        }
        self.report(account, &before);
    }

    /// The app moved to the foreground or the background.
    pub(crate) fn app_active(&mut self, active: bool) {
        self.foreground = active;
        if !active {
            for live in self.live.values_mut() {
                match &live.conn {
                    Conn::Idling(notify) => notify.notify_one(),
                    Conn::Ready(_) => live.conn = Conn::Closed,
                    Conn::Closed => {}
                }
            }
            self.flush_drafts();
            return;
        }
        // The phone may have slept through any connection's death.
        self.idles = FuturesUnordered::new();
        for live in self.live.values_mut() {
            live.conn = Conn::Closed;
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
        }
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
}
