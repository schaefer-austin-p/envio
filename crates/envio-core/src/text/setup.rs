//! The words for adding and removing accounts.

use crate::api::view::UnsentCounts;
use crate::i18n::{fill, tr, tr_n};
use crate::text::status::untrusted_reason;
use envio_net::Untrusted;
use envio_net::autoconfig::Source;

/// Said before `Detect`: the domain of the address leaves the phone.
pub fn ispdb_note() -> String {
    tr(
        "To find your mail servers, Envio asks the Thunderbird configuration database and your mail provider. Only the part of your address after the @ is sent",
    )
}

pub fn found_by(source: Source) -> String {
    match source {
        Source::Ispdb => tr("Settings found in the Thunderbird configuration database"),
        Source::Provider => tr("Settings found at your mail provider"),
        Source::WellKnown => tr("Settings found on your mail domain"),
        Source::Srv => tr("Settings found in your domain DNS records"),
        Source::Guess => tr("No settings found. These are a guess, please check them"),
    }
}

pub fn confirm_servers(imap: &str, smtp: &str) -> String {
    fill(
        &tr(
            "Your domain DNS records point to these servers:\nIncoming: {imap}\nOutgoing: {smtp}\nYour password is sent to them. Only continue if you trust them",
        ),
        &[("imap", imap), ("smtp", smtp)],
    )
}

pub fn trust(host: &str, reason: Untrusted) -> String {
    fill(
        &tr(
            "The certificate of {host} is not trusted: {reason}. Check the fingerprint before you accept it",
        ),
        &[("host", host), ("reason", &untrusted_reason(reason))],
    )
}

/// Said instead of the trust prompt when no root certificates loaded: a
/// fingerprint dialog would then ask about every server.
pub fn no_roots() -> String {
    tr(
        "Envio has no trusted certificate authorities on this device, so it cannot check the server. Please report this problem",
    )
}

pub fn unconfirmed() -> String {
    tr("Confirm the servers before signing in")
}

pub fn duplicate() -> String {
    tr("This account is already added")
}

pub fn sign_in_failed() -> String {
    tr("Sign-in failed. Check the address, username and password")
}

pub fn bad_fingerprint() -> String {
    tr("That certificate fingerprint is not valid")
}

pub fn bad_address() -> String {
    tr("That does not look like an email address")
}

pub fn unreachable() -> String {
    tr("Could not reach the server. Check the settings and your connection")
}

pub fn not_saved() -> String {
    tr("Could not save the account settings. Check the free storage")
}

pub fn nothing_to_accept() -> String {
    tr("There is no certificate waiting to be accepted for this account")
}

pub fn remove_failed() -> String {
    tr("Could not remove the account. Restart Envio, which finishes the removal")
}

pub fn confirm_remove(unsent: &UnsentCounts) -> String {
    let count = |n: usize, one: &str, many: &str| {
        let n = u64::try_from(n).unwrap_or(u64::MAX);
        fill(&tr_n(one, many, n), &[("n", &n.to_string())])
    };
    let mut lines = vec![tr("This account still has work that is not on the server:")];
    if unsent.outbox > 0 {
        lines.push(count(
            unsent.outbox,
            "{n} message in the Outbox",
            "{n} messages in the Outbox",
        ));
    }
    if unsent.drafts > 0 {
        lines.push(count(unsent.drafts, "{n} draft", "{n} drafts"));
    }
    if unsent.actions > 0 {
        lines.push(count(
            unsent.actions,
            "{n} change not yet sent to the server",
            "{n} changes not yet sent to the server",
        ));
    }
    lines.push(tr(
        "Removing the account loses all of it, and the Sent copies of messages that were sent but not yet filed in Sent.",
    ));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_remove_question_counts_each_kind_of_unsent_work() {
        let text = confirm_remove(&UnsentCounts {
            outbox: 1,
            drafts: 2,
            actions: 0,
        });
        assert!(text.contains("1 message in the Outbox"), "{text}");
        assert!(text.contains("2 drafts"), "{text}");
        assert!(!text.contains("change"), "{text}");
        assert!(text.contains("Sent copies"), "{text}");
    }
}
