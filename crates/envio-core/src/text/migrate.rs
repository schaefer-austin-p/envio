//! The words of the Outlook.com move (#342); board `carbon-provider`,
//! frames "Moving", "Stuck", "Done".

use crate::i18n::{fill, tr, tr_n};

fn count(n: usize, one: &str, many: &str) -> String {
    let n = u64::try_from(n).unwrap_or(u64::MAX);
    fill(&tr_n(one, many, n), &[("n", &n.to_string())])
}

/// The account row's status line while the move runs.
pub fn status() -> String {
    tr("Moving to the new connection")
}

pub fn moving_title() -> String {
    status()
}

pub fn moving_text(pending: usize, filing: usize) -> String {
    if pending > 0 {
        count(
            pending,
            "Finishing {n} change first. Envio sends it to Outlook.com, then loads your mail again.",
            "Finishing {n} changes first. Envio sends them to Outlook.com, then loads your mail again.",
        )
    } else if filing > 0 {
        count(
            filing,
            "Envio is still filing {n} sent message.",
            "Envio is still filing {n} sent messages.",
        )
    } else {
        tr("Envio is loading your mail again.")
    }
}

pub fn moving_hint() -> String {
    tr("You can leave this page. Envio carries on by itself.")
}

pub fn stuck_title(pending: usize) -> String {
    count(
        pending,
        "Still finishing {n} change",
        "Still finishing {n} changes",
    )
}

pub fn stuck_text() -> String {
    tr("Envio tried 3 times and could not send them. Check the connection, then try again.")
}

/// Stuck with nothing to replay: sent mail whose copy in Sent is refused.
pub fn stuck_filing_title(filing: usize) -> String {
    count(
        filing,
        "Still filing {n} sent message",
        "Still filing {n} sent messages",
    )
}

pub fn stuck_filing_text() -> String {
    tr("Envio tried 3 times and could not file them in Sent. The mail itself was sent.")
}

pub fn switch_filing_hint() -> String {
    tr(
        "“Switch anyway” moves on without filing them. Nothing is lost, and your Outbox and drafts stay.",
    )
}

pub fn sign_in_title() -> String {
    tr("Outlook.com refused the old sign-in")
}

pub fn sign_in_text(pending: usize) -> String {
    count(
        pending,
        "Envio could not finish {n} change. Sign in again, then Envio tries again.",
        "Envio could not finish {n} changes. Sign in again, then Envio tries again.",
    )
}

pub fn switch_hint(pending: usize) -> String {
    count(
        pending,
        "“Switch anyway” drops this change and moves on. Your Outbox and drafts stay.",
        "“Switch anyway” drops these {n} changes and moves on. Your Outbox and drafts stay.",
    )
}

pub fn done_title() -> String {
    tr("Outlook.com is up to date")
}

pub fn done_text() -> String {
    tr("Your mail is loading again. Your Outbox and drafts are as you left them.")
}

pub fn done_hint() -> String {
    tr("Mail can take a few minutes to show up on this phone.")
}

/// The notice above the result after changes were dropped.
pub fn dropped_title(dropped: usize) -> String {
    count(
        dropped,
        "{n} change was not made",
        "{n} changes were not made",
    )
}

pub fn dropped_text() -> String {
    tr("They were dropped when you chose Switch anyway. Check your folders on Outlook.com.")
}

/// Update sign-in with a work or school account.
pub fn not_personal() -> String {
    tr("Envio can't move it to the new connection. Your account keeps working as before")
}
