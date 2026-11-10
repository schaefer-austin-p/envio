//! One account's health: what the user is told, and what the worker does
//! next after a failure.

use crate::problem::{CertDetails, Problem};
use envio_sync::schedule::backoff_ms;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountStatus {
    Ready,
    Syncing,
    Offline { retry_at_ms: i64 },
    SignInFailed,
    Certificate(Option<CertDetails>),
    DiskFull,
    Removing,
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
}

impl Default for Health {
    fn default() -> Self {
        Self {
            status: AccountStatus::Ready,
            failures: 0,
        }
    }
}

impl Health {
    /// A step worked. A paused account stays paused until `resume`.
    pub fn ok(&mut self) {
        if !self.is_paused() {
            self.status = AccountStatus::Ready;
            self.failures = 0;
        }
    }

    pub fn failed(&mut self, problem: &Problem, account: &str, now_ms: i64) -> Response {
        match problem {
            Problem::Temporary | Problem::Local => {
                self.failures = self.failures.saturating_add(1);
                let at_ms = now_ms + backoff_ms(self.failures, seed(account, now_ms));
                if !self.is_paused() {
                    self.status = AccountStatus::Offline { retry_at_ms: at_ms };
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
            AccountStatus::Offline { retry_at_ms } => retry_at_ms <= now_ms,
            AccountStatus::SignInFailed
            | AccountStatus::Certificate(_)
            | AccountStatus::Removing => false,
        }
    }

    pub fn is_paused(&self) -> bool {
        matches!(
            self.status,
            AccountStatus::SignInFailed | AccountStatus::Certificate(_) | AccountStatus::Removing
        )
    }

    /// The user fixed the cause (new password, accepted certificate).
    pub fn resume(&mut self) {
        self.status = AccountStatus::Ready;
        self.failures = 0;
    }

    /// When a waiting account may run again.
    pub fn wake_at(&self) -> Option<i64> {
        match self.status {
            AccountStatus::Offline { retry_at_ms } => Some(retry_at_ms),
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
        health.ok();
        assert_eq!(health.status, AccountStatus::SignInFailed);
        health.resume();
        health.failed(&Problem::Temporary, "a", 0);
        health.ok();
        assert_eq!(health, Health::default());
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
}
