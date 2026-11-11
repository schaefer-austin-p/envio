//! The words for an account's status.

use crate::health::AccountStatus;
use crate::i18n::{fill, tr};
use crate::problem::CertDetails;
use crate::servers::Service;
use envio_net::Untrusted;

/// An account that could not be loaded; `reason` is a fixed text from the
/// store (never a value from the file).
pub fn broken(reason: &str) -> String {
    fill(
        &tr("This account's settings cannot be read ({reason}). Remove it and add it again"),
        &[("reason", reason)],
    )
}

pub fn status(status: &AccountStatus) -> String {
    match status {
        AccountStatus::Ready => tr("Ready"),
        AccountStatus::Syncing => tr("Syncing"),
        AccountStatus::Offline { .. } => tr("Offline, retrying"),
        AccountStatus::SignInFailed => tr("Sign-in failed, update password"),
        AccountStatus::Certificate(Some(details)) => certificate(details),
        AccountStatus::Certificate(None) => tr("The server certificate needs your attention"),
        AccountStatus::DiskFull => tr("Storage is full, downloads stopped"),
        AccountStatus::Removing => tr("Removing account"),
        AccountStatus::Broken(reason) => broken(reason),
    }
}

pub fn certificate(details: &CertDetails) -> String {
    let service = match details.service {
        Service::Imap => tr("incoming mail"),
        Service::Smtp => tr("outgoing mail"),
    };
    let new = details.presented.display();
    match (&details.pinned, details.reason) {
        (Some(old), _) => fill(
            &tr(
                "The certificate of the {service} server changed.\nOld fingerprint: {old}\nNew fingerprint: {new}",
            ),
            &[
                ("service", &service),
                ("old", &old.display()),
                ("new", &new),
            ],
        ),
        (None, Some(reason)) => fill(
            &tr(
                "The certificate of the {service} server is not trusted: {reason}.\nFingerprint: {new}",
            ),
            &[
                ("service", &service),
                ("reason", &untrusted_reason(reason)),
                ("new", &new),
            ],
        ),
        (None, None) => fill(
            &tr("The certificate of the {service} server is not trusted.\nFingerprint: {new}"),
            &[("service", &service), ("new", &new)],
        ),
    }
}

pub fn untrusted_reason(reason: Untrusted) -> String {
    match reason {
        Untrusted::UnknownIssuer => tr("self-signed or from an unknown authority"),
        Untrusted::Expired => tr("expired"),
        Untrusted::WrongName => tr("issued for another server name"),
        Untrusted::Other => tr("not valid"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use envio_net::Fingerprint;

    #[test]
    fn offline_text_is_offline_retrying() {
        let offline = AccountStatus::Offline { retry_at_ms: 5 };
        assert_eq!(status(&offline), "Offline, retrying");
        assert_eq!(
            status(&AccountStatus::SignInFailed),
            "Sign-in failed, update password"
        );
    }

    #[test]
    fn certificate_text_has_old_and_new() {
        let old = Fingerprint::from_bytes([1; 32]);
        let new = Fingerprint::from_bytes([2; 32]);
        let text = certificate(&CertDetails {
            service: Service::Imap,
            pinned: Some(old),
            presented: new,
            reason: None,
        });
        assert!(text.contains(&old.display()), "{text}");
        assert!(text.contains(&new.display()), "{text}");
    }
}
