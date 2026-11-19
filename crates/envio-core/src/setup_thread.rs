//! The network half of adding an account, on its own thread so the worker
//! keeps answering commands and polling IDLE while a server is slow.
//!
//! The thread has its own current-thread runtime and `block_on`s one job at
//! a time; no future crosses a thread. The worker keeps everything with
//! state (SRV hosts, the confirm rule, accepted pins, adding the account);
//! a job only carries what the probe needs and brings back what it found.

use std::collections::VecDeque;
use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use envio_net::autoconfig::{self, Found, Internet};
use envio_net::imap::ImapSession;
use envio_net::oauth::{self, Provider, TokenError, Tokens};
use envio_net::smtp::SmtpSession;
use envio_net::{Endpoint, Fingerprint, NetError, Roots, Untrusted};
use secrecy::SecretString;
use tokio::sync::mpsc::UnboundedSender;

use crate::api::command::{AccountSetup, ConnectRequest};
use crate::api::event::Event;
use crate::health::AccountStatus;
use crate::mask;
use crate::problem::{self, Problem};
use crate::servers::{ServerAddr, Service, credentials};
use crate::text;
use crate::worker::Input;

static TICKETS: AtomicU64 = AtomicU64::new(1);

/// A number for a new job. A result is used only if it carries the newest
/// ticket the worker gave for its email (a restarted worker has given none).
pub(crate) fn next_ticket() -> u64 {
    TICKETS.fetch_add(1, Ordering::Relaxed)
}

pub(crate) struct ProbeJob {
    pub ticket: u64,
    pub request: ConnectRequest,
    /// The fingerprints the user accepted, already checked.
    pub accepted: Vec<(Service, Fingerprint)>,
    pub roots: Roots,
}

pub(crate) enum SetupJob {
    Detect {
        email: String,
        ticket: u64,
    },
    Probe(Box<ProbeJob>),
    /// Trades the sign-in code for tokens.
    Exchange(Box<ExchangeJob>),
}

pub(crate) struct ExchangeJob {
    pub email: String,
    pub ticket: u64,
    pub provider: Provider,
    pub roots: Roots,
    pub code: SecretString,
    pub verifier: SecretString,
}

pub(crate) struct ExchangeResult {
    pub ticket: u64,
    pub result: Result<Tokens, TokenError>,
}

pub(crate) struct DetectResult {
    pub email: String,
    pub ticket: u64,
    pub result: Result<Found, Problem>,
}

pub(crate) struct ProbeResult {
    pub ticket: u64,
    pub request: ConnectRequest,
    pub accepted: Vec<(Service, Fingerprint)>,
    /// The event that tells the app why not: the trust prompt or a problem.
    pub outcome: Result<(), Box<Event>>,
}

pub(crate) enum SetupDone {
    Detected(Box<DetectResult>),
    Probed(Box<ProbeResult>),
    Exchanged(Box<ExchangeResult>),
}

impl SetupJob {
    fn email(&self) -> &str {
        match self {
            SetupJob::Detect { email, .. } => email,
            SetupJob::Probe(job) => &job.request.setup.email,
            SetupJob::Exchange(job) => &job.email,
        }
    }

    /// Whether `other` makes this one pointless: the same kind of job for
    /// the same address.
    fn replaced_by(&self, other: &SetupJob) -> bool {
        matches!(
            (self, other),
            (SetupJob::Detect { .. }, SetupJob::Detect { .. })
                | (SetupJob::Probe(_), SetupJob::Probe(_))
                | (SetupJob::Exchange(_), SetupJob::Exchange(_))
        ) && self.email() == other.email()
    }
}

/// Puts `job` last, in the place of a queued one it replaces.
fn enqueue(queue: &mut VecDeque<SetupJob>, job: SetupJob) {
    match queue.iter_mut().find(|queued| queued.replaced_by(&job)) {
        Some(slot) => *slot = job,
        None => queue.push_back(job),
    }
}

pub(crate) struct SetupThread {
    jobs: Sender<SetupJob>,
}

impl SetupThread {
    /// Starts the thread. Results go into the worker's channel; the thread
    /// ends when this handle is dropped or the worker's channel is gone.
    pub fn start(roots: Roots, results: UnboundedSender<Input>) -> io::Result<Self> {
        let (jobs, queue) = channel();
        thread::Builder::new()
            .name("envio-setup".into())
            .spawn(move || run(&roots, &queue, &results))?;
        Ok(Self { jobs })
    }

    pub fn send(&self, job: SetupJob) {
        if self.jobs.send(job).is_err() {
            log::warn!("the setup thread has stopped; a job was dropped");
        }
    }
}

fn run(roots: &Roots, jobs: &Receiver<SetupJob>, results: &UnboundedSender<Input>) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => return log::error!("the setup thread has no runtime: {error}"),
    };
    let mut queue: VecDeque<SetupJob> = VecDeque::new();
    loop {
        if queue.is_empty() {
            match jobs.recv() {
                Ok(job) => enqueue(&mut queue, job),
                Err(_) => return,
            }
        }
        while let Ok(job) = jobs.try_recv() {
            enqueue(&mut queue, job);
        }
        let Some(job) = queue.pop_front() else {
            continue;
        };
        let done =
            match panic::catch_unwind(AssertUnwindSafe(|| runtime.block_on(do_job(roots, &job)))) {
                Ok(done) => done,
                Err(_) => {
                    log::error!("a setup job panicked");
                    failed(job)
                }
            };
        if results.send(Input::Setup(done)).is_err() {
            return;
        }
    }
}

/// The answer for a job that could not run.
fn failed(job: SetupJob) -> SetupDone {
    match job {
        SetupJob::Detect { email, ticket } => SetupDone::Detected(Box::new(DetectResult {
            email,
            ticket,
            result: Err(Problem::Local),
        })),
        SetupJob::Exchange(job) => SetupDone::Exchanged(Box::new(ExchangeResult {
            ticket: job.ticket,
            result: Err(TokenError::Temporary),
        })),
        SetupJob::Probe(job) => {
            let email = job.request.setup.email.clone();
            let event = Event::SetupProblem {
                email,
                text: text::setup::unreachable(),
            };
            SetupDone::Probed(Box::new(ProbeResult {
                ticket: job.ticket,
                request: job.request,
                accepted: job.accepted,
                outcome: Err(Box::new(event)),
            }))
        }
    }
}

async fn do_job(roots: &Roots, job: &SetupJob) -> SetupDone {
    #[cfg(feature = "test-hooks")]
    let fake = crate::hooks::setup_started(job.email());
    #[cfg(feature = "test-hooks")]
    if let Some(fake) = &fake {
        tokio::time::sleep(fake.delay).await;
    }
    match job {
        SetupJob::Detect { email, ticket } => {
            log::info!("detecting servers for {}", mask::address(email));
            #[cfg(feature = "test-hooks")]
            let faked = fake.and_then(|fake| fake.found);
            #[cfg(not(feature = "test-hooks"))]
            let faked: Option<Found> = None;
            let result = match faked {
                Some(found) => Ok(found),
                None => autoconfig::detect(email, &Internet::new(roots.clone()))
                    .await
                    .map_err(|e| {
                        log::warn!("detect failed: {e}");
                        problem::from_net(&e, Service::Imap)
                    }),
            };
            SetupDone::Detected(Box::new(DetectResult {
                email: email.clone(),
                ticket: *ticket,
                result,
            }))
        }
        SetupJob::Exchange(job) => {
            let result =
                oauth::exchange_code(&job.provider, &job.roots, &job.code, &job.verifier).await;
            SetupDone::Exchanged(Box::new(ExchangeResult {
                ticket: job.ticket,
                result,
            }))
        }
        SetupJob::Probe(job) => {
            let outcome = probe(&job.request, &job.accepted, &job.roots).await;
            SetupDone::Probed(Box::new(ProbeResult {
                ticket: job.ticket,
                request: job.request.clone(),
                accepted: job.accepted.clone(),
                outcome,
            }))
        }
    }
}

/// The accepted pin for `service`, if any.
pub(crate) fn pin_for(
    accepted: &[(Service, Fingerprint)],
    service: Service,
) -> Option<Fingerprint> {
    accepted
        .iter()
        .find(|(s, _)| *s == service)
        .map(|(_, fingerprint)| *fingerprint)
}

fn endpoint_of(addr: &ServerAddr, pin: Option<Fingerprint>) -> Endpoint {
    Endpoint {
        host: addr.host.clone(),
        port: addr.port,
        security: addr.security,
        pin,
    }
}

/// The event that tells the app why a server did not work out. With no
/// trusted roots every certificate is "unknown", so a fingerprint dialog
/// would be asked about everything: that is a problem of the device.
fn setup_event(
    setup: &AccountSetup,
    service: Service,
    error: &NetError,
    roots_empty: bool,
) -> Event {
    let email = setup.email.clone();
    let host = match service {
        Service::Imap => &setup.imap.host,
        Service::Smtp => &setup.smtp.host,
    };
    let (presented, reason) = match error {
        NetError::Untrusted { .. } if roots_empty => {
            return Event::SetupProblem {
                email,
                text: text::setup::no_roots(),
            };
        }
        NetError::Untrusted { presented, reason } => (*presented, *reason),
        // A pin the user accepted is no longer what the server shows.
        NetError::PinChanged { presented, .. } => (*presented, Untrusted::Other),
        _ => {
            let text = match problem::from_net(error, service) {
                Problem::SignIn => text::setup::sign_in_failed(),
                Problem::Certificate(_) => {
                    text::status::status(&AccountStatus::Certificate(None), false)
                }
                Problem::Refused(reason) => text::notice::refused(&reason),
                _ if dropped(error) => text::setup::dropped(),
                _ => text::setup::unreachable(),
            };
            return Event::SetupProblem { email, text };
        }
    };
    Event::TrustPrompt {
        email,
        service,
        host: host.clone(),
        fingerprint: presented.display(),
        hex: presented.to_hex(),
        title: text::setup::trust_title(service),
        text: text::setup::trust(service, host, reason),
    }
}

/// The connection was closed or reset on us (a rate limit, a firewall, a
/// plaintext port behind implicit TLS), as opposed to never connecting.
fn dropped(error: &NetError) -> bool {
    matches!(
        error,
        NetError::Io(e) if matches!(
            e.kind(),
            io::ErrorKind::ConnectionAborted
                | io::ErrorKind::ConnectionReset
                | io::ErrorKind::UnexpectedEof
        )
    )
}

/// Opens IMAP, then SMTP, with the request's pins.
async fn probe(
    request: &ConnectRequest,
    accepted: &[(Service, Fingerprint)],
    roots: &Roots,
) -> Result<(), Box<Event>> {
    let setup = &request.setup;
    let empty = roots.is_empty();
    let login = credentials(&setup.username, &request.password);
    let imap = endpoint_of(&setup.imap, pin_for(accepted, Service::Imap));
    let session = ImapSession::open(&imap, roots, &login)
        .await
        .map_err(|e| Box::new(setup_event(setup, Service::Imap, &e, empty)))?;
    // Logging out is a courtesy; the check has passed.
    let _ = session.logout().await;
    let on_smtp = |e: NetError| Box::new(setup_event(setup, Service::Smtp, &e, empty));
    let smtp = endpoint_of(&setup.smtp, pin_for(accepted, Service::Smtp));
    let mut session = SmtpSession::connect(&smtp, roots).await.map_err(on_smtp)?;
    session.login(&login).await.map_err(on_smtp)?;
    let _ = session.quit().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(email: &str, ticket: u64) -> SetupJob {
        SetupJob::Detect {
            email: email.into(),
            ticket,
        }
    }

    fn tickets(queue: &VecDeque<SetupJob>) -> Vec<u64> {
        queue
            .iter()
            .map(|job| match job {
                SetupJob::Detect { ticket, .. } => *ticket,
                SetupJob::Probe(job) => job.ticket,
                SetupJob::Exchange(job) => job.ticket,
            })
            .collect()
    }

    fn text_of(error: &NetError) -> String {
        let addr = ServerAddr {
            host: "mail.example.org".into(),
            port: 465,
            security: envio_net::Security::Tls,
        };
        let setup = AccountSetup {
            name: String::new(),
            email: "a@example.org".into(),
            username: "a".into(),
            imap: addr.clone(),
            smtp: addr,
            source: None,
            needs_confirm: false,
        };
        match setup_event(&setup, Service::Smtp, error, false) {
            Event::SetupProblem { text, .. } => text,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_dropped_connection_is_not_called_unreachable() {
        for kind in [
            io::ErrorKind::ConnectionAborted,
            io::ErrorKind::ConnectionReset,
            io::ErrorKind::UnexpectedEof,
        ] {
            let text = text_of(&NetError::Io(io::Error::new(kind, "peer closed")));
            assert_eq!(text, text::setup::dropped(), "{kind:?}");
            assert!(!text.contains("Could not reach"), "{text}");
        }
    }

    #[test]
    fn a_refused_connection_is_still_unreachable() {
        let error = NetError::Io(io::Error::from(io::ErrorKind::ConnectionRefused));
        assert_eq!(text_of(&error), text::setup::unreachable());
    }

    #[test]
    fn a_job_replaces_a_queued_one_of_the_same_kind_and_address() {
        let mut queue = VecDeque::new();
        enqueue(&mut queue, detect("a@example.org", 1));
        enqueue(&mut queue, detect("b@example.org", 2));
        enqueue(&mut queue, detect("a@example.org", 3));
        // The new one keeps the old one's place in line.
        assert_eq!(tickets(&queue), [3, 2]);
    }
}
