//! Counted text. Plural rules differ by language, so the screens never
//! build these themselves: they ask for the text with the count.

use crate::i18n::{fill, tr_n};

/// What a count is of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountKind {
    Selected,
    Results,
    Waiting,
    NewMail,
    /// Search's note on the sync window: mail older than this is on the server only.
    KeptDays,
    /// The All tab's count of accounts whose sync is stopped.
    SyncStopped,
    /// The All tab's count of offline accounts.
    Offline,
}

fn with_count(template: &str, n: u64) -> String {
    fill(template, &[("n", &n.to_string())])
}

pub fn selected(n: u64) -> String {
    with_count(&tr_n("{n} selected", "{n} selected", n), n)
}

pub fn results(n: u64) -> String {
    with_count(&tr_n("{n} result", "{n} results", n), n)
}

/// The Outbox banner.
pub fn waiting(n: u64) -> String {
    with_count(
        &tr_n(
            "{n} message waiting to send",
            "{n} messages waiting to send",
            n,
        ),
        n,
    )
}

pub fn new_mail(n: u64) -> String {
    with_count(&tr_n("{n} new message", "{n} new messages", n), n)
}

/// Search's note when only the last `n` days are kept on the phone.
pub fn kept_days(n: u64) -> String {
    with_count(
        &tr_n(
            "Finds mail older than the {n} day kept here",
            "Finds mail older than the {n} days kept here",
            n,
        ),
        n,
    )
}

pub fn sync_stopped(n: u64) -> String {
    with_count(&tr_n("{n} sync stopped", "{n} sync stopped", n), n)
}

pub fn offline(n: u64) -> String {
    with_count(&tr_n("{n} offline", "{n} offline", n), n)
}

pub fn count_text(kind: CountKind, n: u64) -> String {
    match kind {
        CountKind::Selected => selected(n),
        CountKind::Results => results(n),
        CountKind::Waiting => waiting(n),
        CountKind::NewMail => new_mail(n),
        CountKind::KeptDays => kept_days(n),
        CountKind::SyncStopped => sync_stopped(n),
        CountKind::Offline => offline(n),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_text_for_one_and_many() {
        let all = [
            CountKind::Selected,
            CountKind::Results,
            CountKind::Waiting,
            CountKind::NewMail,
            CountKind::KeptDays,
            CountKind::SyncStopped,
            CountKind::Offline,
        ];
        for kind in all {
            let one = count_text(kind, 1);
            let many = count_text(kind, 3);
            assert!(one.contains("1") && !one.contains("{n}"), "{one}");
            assert!(many.contains("3") && !many.contains("{n}"), "{many}");
            if !matches!(
                kind,
                CountKind::Selected | CountKind::SyncStopped | CountKind::Offline
            ) {
                assert_ne!(one.replace("1", "#"), many.replace("3", "#"), "{kind:?}");
            }
        }
        assert_eq!(results(1), "1 result");
        assert_eq!(results(0), "0 results");
        assert_eq!(waiting(2), "2 messages waiting to send");
        assert_eq!(new_mail(1), "1 new message");
        assert_eq!(sync_stopped(1), "1 sync stopped");
        assert_eq!(offline(2), "2 offline");
        assert_eq!(selected(4), "4 selected");
        assert_eq!(kept_days(90), "Finds mail older than the 90 days kept here");
    }
}
