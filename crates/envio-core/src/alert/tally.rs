//! How much new mail the user has not yet seen, per account: one card per
//! account replaces the last, so its count covers every alert since the
//! app was last on screen, not just the latest sync.

use std::collections::BTreeMap;

use crate::api::event::NewMail;

#[derive(Debug, Default)]
pub struct Tally {
    per_account: BTreeMap<String, NewMail>,
}

impl Tally {
    /// Adds mail the user is about to be alerted to; returns the total for
    /// its account. `first` is kept only while the total is one message.
    pub fn add(&mut self, mail: &NewMail) -> NewMail {
        let total = self
            .per_account
            .entry(mail.account.clone())
            .or_insert_with(|| NewMail {
                account: mail.account.clone(),
                count: 0,
                first: None,
            });
        total.count = total.count.saturating_add(mail.count);
        total.first = if total.count == 1 {
            mail.first.clone()
        } else {
            None
        };
        total.clone()
    }

    /// The app is on screen: nothing is unseen.
    pub fn reset_all(&mut self) {
        self.per_account.clear();
    }

    /// The account's switch went off: its next alert counts from one.
    pub fn reset(&mut self, account: &str) {
        self.per_account.remove(account);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mail(account: &str, count: u32, first: Option<(&str, &str)>) -> NewMail {
        NewMail {
            account: account.to_owned(),
            count,
            first: first.map(|(a, b)| (a.to_owned(), b.to_owned())),
        }
    }

    #[test]
    fn three_adds_count_three() {
        let mut tally = Tally::default();
        assert_eq!(tally.add(&mail("a", 1, None)).count, 1);
        assert_eq!(tally.add(&mail("a", 1, None)).count, 2);
        assert_eq!(tally.add(&mail("a", 1, None)).count, 3);
    }

    #[test]
    fn first_is_kept_for_one_and_dropped_after_two() {
        let mut tally = Tally::default();
        let one = tally.add(&mail("a", 1, Some(("Ann", "Hi"))));
        assert_eq!(one.first, Some(("Ann".into(), "Hi".into())));
        let two = tally.add(&mail("a", 1, Some(("Bob", "Yo"))));
        assert_eq!((two.count, two.first), (2, None));
        assert_eq!(tally.add(&mail("a", 1, Some(("Cy", "Hey")))).first, None);
    }

    #[test]
    fn reset_all_starts_every_account_over() {
        let mut tally = Tally::default();
        tally.add(&mail("a", 2, None));
        tally.add(&mail("b", 2, None));
        tally.reset_all();
        assert_eq!(tally.add(&mail("a", 1, None)).count, 1);
        assert_eq!(tally.add(&mail("b", 1, None)).count, 1);
    }

    #[test]
    fn reset_clears_only_that_account() {
        let mut tally = Tally::default();
        tally.add(&mail("a", 2, None));
        tally.add(&mail("b", 2, None));
        tally.reset("a");
        assert_eq!(tally.add(&mail("a", 1, None)).count, 1);
        assert_eq!(tally.add(&mail("b", 1, None)).count, 3);
    }

    #[test]
    fn accounts_are_separate() {
        let mut tally = Tally::default();
        tally.add(&mail("a", 2, None));
        assert_eq!(tally.add(&mail("b", 1, None)).count, 1);
    }
}
