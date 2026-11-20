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

/// `oauth`: the account signs in with a token, so the fix is signing in
/// again, not a password.
pub fn status(status: &AccountStatus, oauth: bool) -> String {
    status_with(status, oauth, true)
}

/// `provider`: sign-in for an OAuth account is offered by this build. Without
/// it "Sign in again" would be a dead end (#381, #342), so the account is told
/// to be removed.
pub fn status_with(status: &AccountStatus, oauth: bool, provider: bool) -> String {
    match status {
        AccountStatus::SignInFailed if oauth && !provider => {
            crate::text::setup::microsoft_account_unsupported()
        }
        AccountStatus::Ready => tr("Ready"),
        AccountStatus::Syncing => tr("Syncing"),
        AccountStatus::Offline { .. } => tr("Offline, retrying"),
        AccountStatus::LocalError { .. } => {
            tr("Can't read the saved mail on this phone. Retrying.")
        }
        AccountStatus::SignInFailed if oauth => tr("Sign in again"),
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
    fn a_microsoft_account_without_sign_in_is_told_to_be_removed() {
        // #381: no provider, so "Sign in again" would be a dead end.
        let text = status_with(&AccountStatus::SignInFailed, true, false);
        assert!(text.starts_with("Microsoft accounts aren\u{2019}t supported yet"));
        assert!(text.contains("Remove"), "{text}");
        // With a provider, or for a password account, nothing changes.
        assert_eq!(
            status_with(&AccountStatus::SignInFailed, true, true),
            "Sign in again"
        );
        assert_eq!(
            status_with(&AccountStatus::SignInFailed, false, false),
            "Sign-in failed, update password"
        );
        assert_eq!(status_with(&AccountStatus::Ready, true, false), "Ready");
    }

    #[test]
    fn offline_text_is_offline_retrying() {
        let offline = AccountStatus::Offline { retry_at_ms: 5 };
        assert_eq!(status(&offline, false), "Offline, retrying");
        assert_eq!(
            status(&AccountStatus::LocalError { retry_at_ms: 5 }, false),
            "Can't read the saved mail on this phone. Retrying."
        );
        assert_eq!(
            status(&AccountStatus::SignInFailed, false),
            "Sign-in failed, update password"
        );
        assert_eq!(status(&AccountStatus::SignInFailed, true), "Sign in again");
        // Only the sign-in text depends on the kind.
        assert_eq!(status(&offline, true), "Offline, retrying");
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
            host: String::new(),
        });
        assert!(text.contains(&old.display()), "{text}");
        assert!(text.contains(&new.display()), "{text}");
    }
}
