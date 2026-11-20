//! Sorts failures into the spec's kinds, so the rest of the core decides
//! what to do from one small enum.

use crate::servers::Service;
use envio_net::{Fingerprint, NetError, Untrusted};
use envio_store::StoreError;
use envio_sync::{ErrorKind, SyncError};

/// What the user may need to look at for a certificate problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertDetails {
    pub service: Service,
    pub pinned: Option<Fingerprint>,
    pub presented: Fingerprint,
    pub reason: Option<Untrusted>,
    /// The server the certificate came from. Empty until the worker fills
    /// it in for the app (`Worker::report`).
    pub host: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    Temporary,
    SignIn,
    /// `None` until a connect has seen the certificate.
    Certificate(Option<CertDetails>),
    Refused(String),
    DiskFull,
    Damaged,
    Local,
}

/// The log line for a protocol error: the server's text, masked.
fn protocol_line(text: &str) -> String {
    format!("protocol error: {}", crate::mask::server_text(text))
}

pub fn from_net(error: &NetError, service: Service) -> Problem {
    match error {
        NetError::Io(_) | NetError::Timeout | NetError::Busy(_) => Problem::Temporary,
        NetError::Protocol(text) => {
            log::warn!("{}", protocol_line(text));
            Problem::Temporary
        }
        NetError::SignIn(_) => Problem::SignIn,
        NetError::Untrusted { presented, reason } => Problem::Certificate(Some(CertDetails {
            service,
            pinned: None,
            presented: *presented,
            reason: Some(*reason),
            host: String::new(),
        })),
        NetError::PinChanged { pinned, presented } => Problem::Certificate(Some(CertDetails {
            service,
            pinned: Some(*pinned),
            presented: *presented,
            reason: None,
            host: String::new(),
        })),
        NetError::Tls(_) | NetError::NoStartTls => Problem::Certificate(None),
        NetError::Refused(text) => Problem::Refused(text.clone()),
    }
}

/// `from_net` for a failure at `stage`, written to the log first.
pub(crate) fn from_net_at(stage: &str, error: &NetError, service: Service) -> Problem {
    crate::cause::log_net(stage, error);
    from_net(error, service)
}

pub fn from_store(error: &StoreError) -> Problem {
    if error.is_disk_full() {
        Problem::DiskFull
    } else if error.is_damage() {
        Problem::Damaged
    } else {
        Problem::Local
    }
}

pub fn from_sync(error: &SyncError, service: Service) -> Problem {
    match error {
        SyncError::Net(error) => {
            crate::cause::log_net(
                match service {
                    Service::Imap => "IMAP command during sync",
                    Service::Smtp => "SMTP command during send",
                },
                error,
            );
            from_net(error, service)
        }
        SyncError::Store(error) => from_store(error),
    }
}

/// For results that carry only a kind: no details, no server text.
pub fn from_kind(kind: ErrorKind) -> Problem {
    match kind {
        ErrorKind::Temporary | ErrorKind::Protocol => Problem::Temporary,
        ErrorKind::SignIn => Problem::SignIn,
        ErrorKind::Certificate => Problem::Certificate(None),
        ErrorKind::Refused => Problem::Refused(String::new()),
        // A replayed action whose retry failed locally (disk or database).
        ErrorKind::Local => Problem::Local,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_sign_in_pause_at_a_stage_emits_its_cause_line() {
        crate::cause::take_seen();
        let error = NetError::SignIn("AUTHENTICATE failed".into());
        let at = |stage| super::from_net_at(stage, &error, Service::Imap);
        assert_eq!(at(crate::cause::IDLE), Problem::SignIn);
        assert_eq!(at(crate::cause::OPEN_INBOX), Problem::SignIn);
        let sync = SyncError::from(NetError::SignIn("no".into()));
        assert_eq!(super::from_sync(&sync, Service::Imap), Problem::SignIn);
        let seen = crate::cause::take_seen();
        assert_eq!(seen.len(), 3, "{seen:?}");
        assert!(seen[0].starts_with("pause cause: IMAP IDLE: sign-in refused"));
        assert!(seen[1].starts_with("pause cause: IMAP open of the Inbox"));
        assert!(seen[2].starts_with("pause cause: IMAP command during sync"));
    }

    #[test]
    fn a_protocol_error_line_holds_no_secret_or_address() {
        let tok = "EwBwA8l6BAAUbDba3x2OMJElkF7gJ4z_VbCPEz0AAfj0fmHWqWbTB2xt";
        let line = super::protocol_line(&format!("bad reply for bob@outlook.com {tok}"));
        assert!(!line.contains(tok) && !line.contains("bob@"), "{line}");
        assert!(line.starts_with("protocol error: "), "{line}");
    }

    use super::*;

    fn fp(n: u8) -> Fingerprint {
        Fingerprint::from_bytes([n; 32])
    }

    #[test]
    fn each_net_error_has_its_kind() {
        let io = NetError::Io(std::io::Error::other("down"));
        let cases = [
            (io, Problem::Temporary),
            (NetError::Timeout, Problem::Temporary),
            (NetError::Busy("later".into()), Problem::Temporary),
            (NetError::Protocol("odd".into()), Problem::Temporary),
            (NetError::SignIn("no".into()), Problem::SignIn),
            (
                NetError::Untrusted {
                    presented: fp(1),
                    reason: Untrusted::Expired,
                },
                Problem::Certificate(Some(CertDetails {
                    service: Service::Imap,
                    pinned: None,
                    presented: fp(1),
                    reason: Some(Untrusted::Expired),
                    host: String::new(),
                })),
            ),
            (NetError::Tls("bad".into()), Problem::Certificate(None)),
            (NetError::NoStartTls, Problem::Certificate(None)),
            (
                NetError::Refused("quota".into()),
                Problem::Refused("quota".into()),
            ),
        ];
        for (error, want) in cases {
            assert_eq!(from_net(&error, Service::Imap), want, "{error}");
        }
    }

    #[test]
    fn a_changed_pin_keeps_both_fingerprints() {
        let error = NetError::PinChanged {
            pinned: fp(1),
            presented: fp(2),
        };
        assert_eq!(
            from_net(&error, Service::Smtp),
            Problem::Certificate(Some(CertDetails {
                service: Service::Smtp,
                pinned: Some(fp(1)),
                presented: fp(2),
                reason: None,
                host: String::new(),
            }))
        );
    }

    #[test]
    fn store_disk_full_and_damage_are_told_apart() {
        let full = StoreError::Io(std::io::Error::from(std::io::ErrorKind::StorageFull));
        let damaged = StoreError::Damaged("bad page".into());
        let other = StoreError::NotFound;
        assert_eq!(from_store(&full), Problem::DiskFull);
        assert_eq!(from_store(&damaged), Problem::Damaged);
        assert_eq!(from_store(&other), Problem::Local);
    }

    #[test]
    fn protocol_is_temporary_for_the_user() {
        assert_eq!(from_kind(ErrorKind::Protocol), Problem::Temporary);
        let sync = SyncError::Net(NetError::Protocol("odd".into()));
        assert_eq!(from_sync(&sync, Service::Imap), Problem::Temporary);
    }

    #[test]
    fn from_kind_has_no_details() {
        assert_eq!(from_kind(ErrorKind::Temporary), Problem::Temporary);
        assert_eq!(from_kind(ErrorKind::SignIn), Problem::SignIn);
        assert_eq!(
            from_kind(ErrorKind::Certificate),
            Problem::Certificate(None)
        );
        assert_eq!(
            from_kind(ErrorKind::Refused),
            Problem::Refused(String::new())
        );
        assert_eq!(from_kind(ErrorKind::Local), Problem::Local);
    }
}
