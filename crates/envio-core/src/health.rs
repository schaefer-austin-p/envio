//! One account's health: what the user is told, and what the worker does
//! next after a failure.

use crate::problem::{CertDetails, Problem};
use envio_sync::schedule::backoff_ms;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountStatus {
    Ready,
    Syncing,
    Offline {
        retry_at_ms: i64,
    },
    /// The saved mail (database or files on this phone) could not be read or
    /// written; retried like Offline, but the network is not to blame.
    LocalError {
        retry_at_ms: i64,
    },
    SignInFailed,
    Certificate(Option<CertDetails>),
    DiskFull,
    Removing,
    /// The stored entry cannot be read: never connected, never retried.
    /// The text is the store's fixed reason.
    Broken(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Response {
    Retry { at_ms: i64 },
    Pause,
    ItemError,
    Rebuild,
    StopDownloads,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Health {
    pub status: AccountStatus,
    pub failures: u32,
    /// The kind of step that failed last (see `Step::kind`): the failure
    /// count only resets when a step of this kind works, so one step that
    /// keeps failing between steps that work still backs off.
    pub failing: Option<&'static str>,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            status: AccountStatus::Ready,
            failures: 0,
            failing: None,
        }
    }
}

impl Health {
    /// A step worked. A paused account stays paused until `resume`.
    /// A full disk is not a failure that went away: with `disk_full` the
    /// status stays DiskFull.
    pub fn ok(&mut self, disk_full: bool) {
        if !self.is_paused() {
            self.status = if disk_full {
                AccountStatus::DiskFull
            } else {
                AccountStatus::Ready
            };
            self.failures = 0;
            self.failing = None;
        }
    }

    /// A step of `kind` worked. The failure count (and so the backoff) only
    /// resets when `kind` is the kind that is failing.
    pub fn ok_step(&mut self, kind: &'static str, disk_full: bool) {
        // Any step working shows the connection is back.
        let keep = self
            .failing
            .filter(|failing| *failing != kind && *failing != "connect");
        let failures = self.failures;
        self.ok(disk_full);
        if let Some(failing) = keep {
            self.failures = failures;
            self.failing = Some(failing);
        }
    }

    /// A connection (and the Inbox SELECT) worked: a failure to connect is
    /// over (and a full disk shows again). A failure of any other kind
    /// stays, with its status.
    pub fn connected(&mut self, disk_full: bool) {
        if matches!(self.failing, None | Some("connect")) {
            self.ok_step("connect", disk_full);
        }
    }

    /// An IDLE ran without failing: only an IDLE failure is cleared, so
    /// the status of a sync under way is not overwritten.
    pub fn idle_ran(&mut self, disk_full: bool) {
        if self.failing == Some("idle") {
            self.ok_step("idle", disk_full);
        }
    }

    /// `failed`, for a step of `kind`.
    pub fn failed_step(
        &mut self,
        kind: &'static str,
        problem: &Problem,
        (account, now_ms): (&str, i64),
    ) -> Response {
        let response = self.failed(problem, account, now_ms);
        if matches!(response, Response::Retry { .. }) {
            self.failing = Some(kind);
        }
        response
    }

    pub fn failed(&mut self, problem: &Problem, account: &str, now_ms: i64) -> Response {
        match problem {
            Problem::Temporary | Problem::Local => {
                self.failures = self.failures.saturating_add(1);
                let at_ms = now_ms + backoff_ms(self.failures, seed(account, now_ms));
                if !self.is_paused() {
                    self.status = if matches!(problem, Problem::Local) {
                        AccountStatus::LocalError { retry_at_ms: at_ms }
                    } else {
                        AccountStatus::Offline { retry_at_ms: at_ms }
                    };
                }
                Response::Retry { at_ms }
            }
            Problem::SignIn => {
                self.status = AccountStatus::SignInFailed;
                Response::Pause
            }
            Problem::Certificate(details) => {
                self.status = AccountStatus::Certificate(details.clone());
                Response::Pause
            }
            Problem::Refused(_) => Response::ItemError,
            Problem::DiskFull => {
                if !self.is_paused() {
                    self.status = AccountStatus::DiskFull;
                }
                Response::StopDownloads
            }
            Problem::Damaged => Response::Rebuild,
        }
    }

    pub fn can_run(&self, now_ms: i64) -> bool {
        match self.status {
            AccountStatus::Ready | AccountStatus::Syncing | AccountStatus::DiskFull => true,
            AccountStatus::Offline { retry_at_ms } | AccountStatus::LocalError { retry_at_ms } => {
                retry_at_ms <= now_ms
            }
            AccountStatus::SignInFailed
            | AccountStatus::Certificate(_)
            | AccountStatus::Removing
            | AccountStatus::Broken(_) => false,
        }
    }

    pub fn is_paused(&self) -> bool {
        matches!(
            self.status,
            AccountStatus::SignInFailed
                | AccountStatus::Certificate(_)
                | AccountStatus::Removing
                | AccountStatus::Broken(_)
        )
    }

    /// The user fixed the cause (new password, accepted certificate).
    pub fn resume(&mut self) {
        self.status = AccountStatus::Ready;
        self.failures = 0;
        self.failing = None;
    }

    /// When a waiting account may run again.
    pub fn wake_at(&self) -> Option<i64> {
        match self.status {
            AccountStatus::Offline { retry_at_ms } | AccountStatus::LocalError { retry_at_ms } => {
                Some(retry_at_ms)
            }
            _ => None,
        }
    }
}

/// FNV-1a of the account key xor the time, so accounts do not retry in
/// lockstep.
fn seed(account: &str, now_ms: i64) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in account.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    h ^ now_ms.unsigned_abs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use envio_net::Fingerprint;

    fn offline_for(health: &Health, now: i64) -> i64 {
        match health.status {
            AccountStatus::Offline { retry_at_ms } => retry_at_ms - now,
            ref other => panic!("not offline: {other:?}"),
        }
    }

    #[test]
    fn temporary_failures_back_off_from_one_second_to_five_minutes() {
        let mut health = Health::default();
        let now = 1_000_000;
        let mut waits = Vec::new();
        for _ in 0..12 {
            let response = health.failed(&Problem::Temporary, "a@x.org", now);
            let Response::Retry { at_ms } = response else {
                panic!("{response:?}")
            };
            assert_eq!(offline_for(&health, now), at_ms - now);
            waits.push(at_ms - now);
        }
        assert!((750..=1_000).contains(&waits[0]), "{waits:?}");
        assert!((1_500..=2_000).contains(&waits[1]), "{waits:?}");
        assert!((225_000..=300_000).contains(&waits[11]), "{waits:?}");
        assert!(waits.iter().all(|w| *w <= 300_000));
        assert_eq!(health.failures, 12);
    }

    #[test]
    fn sign_in_pauses_without_retry() {
        let mut health = Health::default();
        assert_eq!(health.failed(&Problem::SignIn, "a", 0), Response::Pause);
        assert_eq!(health.status, AccountStatus::SignInFailed);
        assert!(health.is_paused());
        assert!(!health.can_run(i64::MAX));
        assert_eq!(health.wake_at(), None);
    }

    #[test]
    fn certificate_pauses_until_resume() {
        let mut health = Health::default();
        let details = CertDetails {
            service: crate::servers::Service::Imap,
            pinned: None,
            presented: Fingerprint::from_bytes([3; 32]),
            reason: None,
            host: String::new(),
        };
        let problem = Problem::Certificate(Some(details.clone()));
        assert_eq!(health.failed(&problem, "a", 0), Response::Pause);
        assert_eq!(health.status, AccountStatus::Certificate(Some(details)));
        assert!(health.is_paused());
        health.resume();
        assert_eq!(health.status, AccountStatus::Ready);
        assert!(!health.is_paused());
        assert_eq!(health.failures, 0);
    }

    #[test]
    fn ok_does_not_unpause() {
        let mut health = Health::default();
        health.failed(&Problem::SignIn, "a", 0);
        health.ok(false);
        assert_eq!(health.status, AccountStatus::SignInFailed);
        health.resume();
        health.failed(&Problem::Temporary, "a", 0);
        health.ok(false);
        assert_eq!(health, Health::default());
    }

    #[test]
    fn ok_after_a_temporary_failure_keeps_disk_full() {
        let mut health = Health::default();
        health.failed(&Problem::DiskFull, "a", 0);
        health.failed(&Problem::Temporary, "a", 0);
        assert!(matches!(health.status, AccountStatus::Offline { .. }));
        health.ok(true);
        assert_eq!(health.status, AccountStatus::DiskFull);
        assert_eq!(health.failures, 0);
        health.ok(false);
        assert_eq!(health.status, AccountStatus::Ready);
    }

    #[test]
    fn a_broken_account_never_runs_and_stays_paused() {
        let mut health = Health {
            status: AccountStatus::Broken("x".into()),
            ..Health::default()
        };
        assert!(!health.can_run(i64::MAX));
        assert!(health.is_paused());
        health.ok(false);
        assert_eq!(health.status, AccountStatus::Broken("x".into()));
    }

    #[test]
    fn refused_leaves_the_status() {
        let mut health = Health::default();
        let response = health.failed(&Problem::Refused("no".into()), "a", 0);
        assert_eq!(response, Response::ItemError);
        assert_eq!(health, Health::default());
        assert_eq!(health.failed(&Problem::Damaged, "a", 0), Response::Rebuild);
        assert_eq!(health, Health::default());
        let full = health.failed(&Problem::DiskFull, "a", 0);
        assert_eq!(full, Response::StopDownloads);
        assert_eq!(health.status, AccountStatus::DiskFull);
        assert!(health.can_run(0));
    }

    #[test]
    fn offline_can_run_after_its_retry_time() {
        let mut health = Health::default();
        let Response::Retry { at_ms } = health.failed(&Problem::Local, "a", 5_000) else {
            panic!("not a retry")
        };
        assert!(!health.can_run(at_ms - 1));
        assert!(health.can_run(at_ms));
        assert_eq!(health.wake_at(), Some(at_ms));
    }

    #[test]
    fn a_step_that_keeps_failing_backs_off_while_others_succeed() {
        let mut health = Health::default();
        let mut waits = Vec::new();
        for _ in 0..8 {
            health.ok_step("replay", false);
            let Response::Retry { at_ms } =
                health.failed_step("folder", &Problem::Local, ("a@x.org", 0))
            else {
                panic!("not a retry")
            };
            waits.push(at_ms);
        }
        assert!(waits.windows(2).all(|w| w[1] > w[0]), "{waits:?}");
        assert_eq!(health.failures, 8);
        // The failing kind working is what resets it.
        health.ok_step("folder", false);
        assert_eq!(health, Health::default());
    }

    #[test]
    fn a_local_failure_is_not_offline() {
        let mut health = Health::default();
        health.failed(&Problem::Local, "a", 0);
        assert!(matches!(health.status, AccountStatus::LocalError { .. }));
        health.ok(false);
        health.failed(&Problem::Temporary, "a", 0);
        assert!(matches!(health.status, AccountStatus::Offline { .. }));
    }

    #[test]
    fn a_connect_clears_a_connect_failure_but_not_an_idle_one() {
        let problem = Problem::Temporary;
        let mut health = Health::default();
        health.failed_step("connect", &problem, ("a", 0));
        health.connected(true);
        assert_eq!(health.status, AccountStatus::DiskFull);
        assert_eq!(health.failures, 0);

        let mut health = Health::default();
        health.failed_step("idle", &problem, ("a", 0));
        health.failed_step("idle", &problem, ("a", 0));
        health.connected(false);
        assert_eq!(health.failures, 2);
        assert_eq!(health.failing, Some("idle"));
    }

    #[test]
    fn a_connect_keeps_the_status_of_another_kinds_failure() {
        let mut health = Health::default();
        health.failed_step("folder", &Problem::Local, ("a", 0));
        let before = health.clone();
        health.connected(false);
        assert_eq!(health, before);
        assert!(matches!(health.status, AccountStatus::LocalError { .. }));
    }

    #[test]
    fn an_idle_that_ran_clears_only_an_idle_failure() {
        let mut health = Health {
            status: AccountStatus::Syncing,
            ..Health::default()
        };
        health.idle_ran(false);
        assert_eq!(health.status, AccountStatus::Syncing);

        let mut health = Health::default();
        health.failed_step("idle", &Problem::Temporary, ("a", 0));
        health.idle_ran(false);
        assert_eq!(health.status, AccountStatus::Ready);
        assert_eq!(health.failures, 0);
    }
}
