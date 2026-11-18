//! Hearing about changes: what the server reported between commands, and
//! IDLE (RFC 2177) to wait for the next change in the selected folder.

use std::future::Future;
use std::pin::pin;
use std::time::Duration;

use async_imap::extensions::idle::IdleResponse;
use async_imap::imap_proto::{MailboxDatum, Response};
use async_imap::types::UnsolicitedResponse;
use futures::future::{Either, select};
use tokio::time::Instant;

use super::ImapSession;
use super::client::map_error;
use crate::connect;
use crate::error::Result;

/// RFC 2177: servers may drop an IDLE after 30 minutes, so it is renewed
/// before that.
pub const IDLE_MAX: Duration = Duration::from_secs(29 * 60);

/// The size of async-imap's unsolicited-response channel.
const UNSOLICITED_CAPACITY: usize = 100;

/// Why `idle` returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Woke {
    /// The selected folder changed (new mail, expunge, flags).
    Changed,
    /// `wait` passed with no change.
    Quiet,
    /// The caller's `stop` future finished (the user did something).
    Stopped,
}

impl ImapSession {
    /// Whether the server reported a change in the selected folder since
    /// the last call (EXISTS, EXPUNGE, FETCH sent along with other
    /// replies). Drains what was reported. async-imap keeps at most 100 and
    /// drops the rest, so a full channel counts as changed.
    #[must_use]
    pub fn take_changes(&mut self) -> bool {
        let mut changed = std::mem::take(&mut self.missed_changes);
        let mut drained = 0;
        while let Ok(response) = self.session.unsolicited_responses.try_recv() {
            drained += 1;
            // STATUS is about a folder's counts, sent by some servers
            // (Stalwart) on their own; RECENT comes with EXISTS; an
            // untagged OK/NO/BAD/BYE (an IDLE keepalive caught by DONE,
            // say) changes nothing in the folder.
            changed |= !match &response {
                UnsolicitedResponse::Status { .. } | UnsolicitedResponse::Recent(_) => true,
                UnsolicitedResponse::Other(data) => matches!(data.parsed(), Response::Data { .. }),
                _ => false,
            };
        }
        changed || drained >= UNSOLICITED_CAPACITY
    }

    /// Waits in IDLE until the selected folder changes, `wait` passes
    /// (capped at [`IDLE_MAX`]) or `stop` finishes. Consumes the session
    /// and gives it back, since nothing else can run during IDLE. Check
    /// `caps().idle` first; without IDLE, poll with `noop`.
    pub async fn idle(
        self,
        wait: Duration,
        stop: impl Future<Output = ()>,
    ) -> Result<(Self, Woke)> {
        self.idle_with(wait, stop, || {}).await
    }

    /// [`idle`](Self::idle), calling `entered` once the server has accepted
    /// IDLE: changes from then on are reported.
    pub async fn idle_with(
        self,
        wait: Duration,
        stop: impl Future<Output = ()>,
        entered: impl FnOnce(),
    ) -> Result<(Self, Woke)> {
        let Self {
            session,
            caps,
            exists,
            missed_changes,
        } = self;
        let mut handle = session.idle();
        connect::within(async { handle.init().await.map_err(map_error) }).await?;
        entered();
        let deadline = Instant::now() + wait.min(IDLE_MAX);
        let mut stop = pin!(stop);
        // async-imap restarts its own timer on every response, keepalives
        // included, so the deadline is enforced here.
        let mut expired = pin!(tokio::time::sleep_until(deadline));
        let woke = loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let (waiting, interrupt) = handle.wait_with_timeout(left);
            let heard = match select(pin!(waiting), select(stop.as_mut(), expired.as_mut())).await {
                Either::Left((heard, _)) => heard.map_err(map_error)?,
                Either::Right((Either::Left(((), _)), _)) => break Woke::Stopped,
                Either::Right((Either::Right(((), _)), _)) => break Woke::Quiet,
            };
            drop(interrupt);
            match heard {
                IdleResponse::NewData(data) if is_status(data.parsed()) => {}
                IdleResponse::NewData(_) => break Woke::Changed,
                IdleResponse::Timeout | IdleResponse::ManualInterrupt => break Woke::Quiet,
            }
        };
        let session = connect::within(async { handle.done().await.map_err(map_error) }).await?;
        let mut this = Self {
            session,
            caps,
            exists,
            missed_changes,
        };
        // DONE's reply may carry what changed while it was sent.
        let woke = if woke != Woke::Changed && this.take_changes() {
            Woke::Changed
        } else {
            woke
        };
        Ok((this, woke))
    }
}

fn is_status(response: &Response<'_>) -> bool {
    matches!(response, Response::MailboxData(MailboxDatum::Status { .. }))
}
