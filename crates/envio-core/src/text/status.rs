//! The words for an account's status.

use crate::health::AccountStatus;
use crate::i18n::{fill, tr, tr_n};
use crate::problem::CertDetails;
use crate::servers::Service;
use crate::text::count;
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
        AccountStatus::LocalError { .. } => {
            tr("Can't read the saved mail on this phone. Retrying.")
        }
        AccountStatus::SignInFailed => tr("Sign-in failed, update password"),
        AccountStatus::Certificate(Some(details)) => certificate(details),
        AccountStatus::Certificate(None) => tr("The server certificate needs your attention"),
        AccountStatus::DiskFull => tr("Storage is full, downloads stopped"),
        AccountStatus::Removing => tr("Removing account"),
        AccountStatus::Broken(reason) => broken(reason),
    }
}

/// The Outbox count after the offline text, when mail is waiting.
fn with_waiting(offline: String, waiting: u64) -> String {
    if waiting == 0 {
        return offline;
    }
    fill(
        // TRANSLATORS: {offline} is the offline sentence; {waiting} is how
        // many messages wait to be sent, e.g. "2 messages waiting to send".
        &tr("{offline} {waiting}"),
        &[("offline", &offline), ("waiting", &count::waiting(waiting))],
    )
}

/// The banner on an account's own tab or folder.
pub fn offline_here(waiting: u64) -> String {
    with_waiting(tr("Offline."), waiting)
}

/// The banner on the unified tab: the offline accounts by address. Empty
/// for none.
pub fn offline_accounts(emails: &[String], waiting: u64) -> String {
    let n = emails.len() as u64;
    let text = match emails {
        [] => return String::new(),
        [one] => fill(&tr("{email} is offline."), &[("email", one)]),
        _ => fill(
            &tr_n(
                "{n} account is offline: {emails}",
                "{n} accounts are offline: {emails}",
                n,
            ),
            &[("n", &n.to_string()), ("emails", &emails.join(", "))],
        ),
    };
    with_waiting(text, waiting)
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
            status(&AccountStatus::LocalError { retry_at_ms: 5 }),
            "Can't read the saved mail on this phone. Retrying."
        );
        assert_eq!(
            status(&AccountStatus::SignInFailed),
            "Sign-in failed, update password"
        );
    }

    fn emails(list: &[&str]) -> Vec<String> {
        list.iter().map(|e| e.to_string()).collect()
    }

    #[test]
    fn one_offline_account_is_named() {
        assert_eq!(
            offline_accounts(&emails(&["a@example.org"]), 0),
            "a@example.org is offline."
        );
        assert_eq!(offline_accounts(&[], 3), "");
    }

    #[test]
    fn several_offline_accounts_are_counted_and_named() {
        assert_eq!(
            offline_accounts(&emails(&["a@example.org", "b@example.org"]), 0),
            "2 accounts are offline: a@example.org, b@example.org"
        );
    }

    #[test]
    fn waiting_mail_is_added_to_the_offline_text() {
        assert_eq!(
            offline_accounts(&emails(&["a@example.org"]), 2),
            "a@example.org is offline. 2 messages waiting to send"
        );
        assert_eq!(offline_here(1), "Offline. 1 message waiting to send");
    }

    #[test]
    fn offline_here_is_the_plain_text() {
        assert_eq!(offline_here(0), "Offline.");
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
