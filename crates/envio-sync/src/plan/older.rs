//! "Load older": which of the server's UIDs below the oldest cached one to
//! fetch next. Pure.

use std::collections::BTreeSet;

use crate::plan::sync::BATCH;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OlderPlan {
    /// At most [`BATCH`] UIDs, newest first.
    pub fetch: Vec<u32>,
    /// After this batch no server UID below the oldest fetched is left
    /// uncached.
    pub exhausted: bool,
}

/// `below` is the server's UIDs under the oldest cached one (any order);
/// `cached` is the folder's cached UIDs.
pub fn plan_older(below: &[u32], cached: &BTreeSet<u32>) -> OlderPlan {
    let missing: BTreeSet<u32> = below
        .iter()
        .copied()
        .filter(|uid| !cached.contains(uid))
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
    fn older_yields_uids_below_the_oldest_cached_in_batches() {
        let below: Vec<u32> = (1..=120).collect();
        let mut cached = BTreeSet::new();
        let first = plan_older(&below, &cached);
        assert_eq!(first.fetch, (71..=120).rev().collect::<Vec<_>>());
        assert!(!first.exhausted);
        cached.extend(first.fetch.iter().copied());
        let second = plan_older(&below, &cached);
        assert_eq!(second.fetch, (21..=70).rev().collect::<Vec<_>>());
        assert!(!second.exhausted);
        cached.extend(second.fetch.iter().copied());
        let third = plan_older(&below, &cached);
        assert_eq!(third.fetch, (1..=20).rev().collect::<Vec<_>>());
        assert!(third.exhausted);
    }

    #[test]
    fn older_skips_uids_already_cached() {
        let cached: BTreeSet<u32> = [3, 5].into();
        let plan = plan_older(&[1, 2, 3, 4, 5], &cached);
        assert_eq!(plan.fetch, [4, 2, 1]);
        assert!(plan.exhausted);
    }

    #[test]
    fn older_is_exhausted_when_nothing_is_below() {
        let plan = plan_older(&[], &BTreeSet::new());
        assert!(plan.fetch.is_empty());
        assert!(plan.exhausted);
    }
}
