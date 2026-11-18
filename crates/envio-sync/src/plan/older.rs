//! "Load older": which of the server's UIDs to fetch next. Pure.
//!
//! Paged by what is not cached yet, not by UID order: mail can have been
//! appended out of date order, so old mail may sit at higher UIDs than the
//! window's (#94). Nothing is skipped; newest UID first is the order
//! (the same as date order in a mailbox filled in date order).

use std::collections::BTreeSet;

use crate::plan::sync::{BATCH, Kept};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OlderPlan {
    /// At most [`BATCH`] UIDs, newest first.
    pub fetch: Vec<u32>,
    /// After this batch no server UID is left uncached or unplanned.
    pub exhausted: bool,
}

/// `server` is every UID in the folder (any order); `cached` is the
/// folder's cached UIDs; `planned` what earlier batches of this run planned
/// (mail that was not stored must not be planned again).
pub fn plan_older(server: &[u32], cached: &BTreeSet<u32>, planned: Option<&Kept>) -> OlderPlan {
    let missing: BTreeSet<u32> = server
        .iter()
        .copied()
        .filter(|uid| !cached.contains(uid) && !planned.is_some_and(|kept| kept.holds(*uid)))
        .collect();
    OlderPlan {
        exhausted: missing.len() <= BATCH,
        fetch: missing.iter().rev().take(BATCH).copied().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_yields_the_uncached_uids_in_batches() {
        let server: Vec<u32> = (1..=120).collect();
        let mut cached = BTreeSet::new();
        let first = plan_older(&server, &cached, None);
        assert_eq!(first.fetch, (71..=120).rev().collect::<Vec<_>>());
        assert!(!first.exhausted);
        cached.extend(first.fetch.iter().copied());
        let second = plan_older(&server, &cached, None);
        assert_eq!(second.fetch, (21..=70).rev().collect::<Vec<_>>());
        assert!(!second.exhausted);
        cached.extend(second.fetch.iter().copied());
        let third = plan_older(&server, &cached, None);
        assert_eq!(third.fetch, (1..=20).rev().collect::<Vec<_>>());
        assert!(third.exhausted);
    }

    #[test]
    fn older_reaches_uids_above_the_cached_ones() {
        // The window's mail is UIDs 1 to 20; old mail was appended after it.
        let server: Vec<u32> = (1..=120).collect();
        let cached: BTreeSet<u32> = (1..=20).collect();
        let plan = plan_older(&server, &cached, None);
        assert_eq!(plan.fetch, (71..=120).rev().collect::<Vec<_>>());
        assert!(!plan.exhausted);
    }

    #[test]
    fn older_skips_uids_already_cached() {
        let cached: BTreeSet<u32> = [3, 5].into();
        let plan = plan_older(&[1, 2, 3, 4, 5], &cached, None);
        assert_eq!(plan.fetch, [4, 2, 1]);
        assert!(plan.exhausted);
    }

    #[test]
    fn older_skips_uids_planned_before() {
        let planned = Kept::default().with(&[4, 2]);
        let plan = plan_older(&[1, 2, 3, 4, 5], &BTreeSet::new(), Some(&planned));
        assert_eq!(plan.fetch, [5, 3, 1]);
    }

    #[test]
    fn older_is_exhausted_when_nothing_is_missing() {
        let plan = plan_older(&[], &BTreeSet::new(), None);
        assert!(plan.fetch.is_empty());
        assert!(plan.exhausted);
    }
}
