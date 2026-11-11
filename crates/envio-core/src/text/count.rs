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

pub fn count_text(kind: CountKind, n: u64) -> String {
    match kind {
        CountKind::Selected => selected(n),
        CountKind::Results => results(n),
        CountKind::Waiting => waiting(n),
        CountKind::NewMail => new_mail(n),
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
        ];
        for kind in all {
            let one = count_text(kind, 1);
            let many = count_text(kind, 3);
            assert!(one.contains("1") && !one.contains("{n}"), "{one}");
            assert!(many.contains("3") && !many.contains("{n}"), "{many}");
            if kind != CountKind::Selected {
                assert_ne!(one.replace("1", "#"), many.replace("3", "#"), "{kind:?}");
            }
        }
        assert_eq!(results(1), "1 result");
        assert_eq!(results(0), "0 results");
        assert_eq!(waiting(2), "2 messages waiting to send");
        assert_eq!(new_mail(1), "1 new message");
        assert_eq!(selected(4), "4 selected");
    }
}
