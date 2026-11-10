//! When work is due: the Undo window, retry backoff with jitter, and the
//! poll interval for servers without IDLE. Pure and in memory only; after
//! a restart every pending item is simply due again.

use std::collections::BTreeMap;

/// How long a user action waits before it goes to the server.
pub const UNDO_MS: i64 = 5_000;
/// How often a folder is checked on a server without IDLE.
pub const POLL_MS: i64 = 5 * 60_000;
/// The first retry delay after a temporary failure.
pub const BACKOFF_FIRST_MS: i64 = 1_000;
/// The longest retry delay.
pub const BACKOFF_MAX_MS: i64 = 5 * 60_000;

/// The delay before retry number `failures` (1 for the first failure):
/// 1 s doubling up to 5 min, minus up to a quarter for jitter so that
/// accounts that failed together don't retry together.
pub fn backoff_ms(failures: u32, seed: u64) -> i64 {
    let doublings = failures.saturating_sub(1).min(20);
    let base = (BACKOFF_FIRST_MS << doublings).min(BACKOFF_MAX_MS);
    let quarter = u64::try_from(base / 4).unwrap_or(0);
    let jitter = splitmix(seed) % (quarter + 1);
    base - i64::try_from(jitter).unwrap_or(0)
}

/// What a retry is for: a journal action by its sequence number, or an
/// Outbox item by its id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Key {
    Action(u64),
    Outbox(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Wait {
    failures: u32,
    due_ms: i64,
}

/// Retry times for items that failed temporarily. An item with no entry is
/// due now.
#[derive(Debug, Default)]
pub struct Schedule {
    waits: BTreeMap<Key, Wait>,
}

impl Schedule {
    pub fn is_due(&self, key: &Key, now_ms: i64) -> bool {
        self.waits.get(key).is_none_or(|wait| wait.due_ms <= now_ms)
    }

    /// Records a temporary failure and returns when to try again.
    pub fn failed(&mut self, key: Key, now_ms: i64) -> i64 {
        let failures = self.waits.get(&key).map_or(0, |wait| wait.failures) + 1;
        let seed = seed_of(&key) ^ u64::from(failures) ^ now_ms.unsigned_abs();
        let due_ms = now_ms + backoff_ms(failures, seed);
        self.waits.insert(key, Wait { failures, due_ms });
        due_ms
    }

    /// Forgets an item that went through or is gone.
    pub fn succeeded(&mut self, key: &Key) {
        self.waits.remove(key);
    }

    pub fn failures(&self, key: &Key) -> u32 {
        self.waits.get(key).map_or(0, |wait| wait.failures)
    }

    /// The earliest retry time, if anything is waiting.
    pub fn next_due(&self) -> Option<i64> {
        self.waits.values().map(|wait| wait.due_ms).min()
    }
}

fn seed_of(key: &Key) -> u64 {
    match key {
        Key::Action(seq) => *seq,
        // FNV-1a: stable across runs, unlike the std hasher.
        Key::Outbox(id) => id.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        }),
    }
}

fn splitmix(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_from_one_second_to_five_minutes_with_jitter() {
        for seed in 0..200 {
            let first = backoff_ms(1, seed);
            assert!((750..=1_000).contains(&first), "{first}");
            let third = backoff_ms(3, seed);
            assert!((3_000..=4_000).contains(&third), "{third}");
            let late = backoff_ms(40, seed);
            assert!((225_000..=300_000).contains(&late), "{late}");
        }
        let spread: std::collections::BTreeSet<i64> = (0..50).map(|s| backoff_ms(5, s)).collect();
        assert!(spread.len() > 10, "jitter spreads retries: {spread:?}");
    }

    #[test]
    fn an_item_waits_after_a_failure_and_longer_after_the_next() {
        let mut schedule = Schedule::default();
        let key = Key::Outbox("1700-1".into());
        assert!(schedule.is_due(&key, 0));
        let first = schedule.failed(key.clone(), 10_000);
        assert!(!schedule.is_due(&key, first - 1));
        assert!(schedule.is_due(&key, first));
        let second = schedule.failed(key.clone(), first);
        assert!(second - first >= 1_500);
        assert_eq!(schedule.failures(&key), 2);
        assert_eq!(schedule.next_due(), Some(second));
        schedule.succeeded(&key);
        assert!(schedule.is_due(&key, 0));
        assert_eq!(schedule.next_due(), None);
    }

    #[test]
    fn the_earliest_wait_is_next() {
        let mut schedule = Schedule::default();
        schedule.failed(Key::Action(1), 0);
        schedule.failed(Key::Action(1), 0);
        let soon = schedule.failed(Key::Action(2), 0);
        assert_eq!(schedule.next_due(), Some(soon));
    }
}
