//! The DKIM keys the core has looked up, in memory (sender trust, #285).
//! Pure: the clock is passed in. Nothing is written to disk.

use std::cell::Cell;
use std::collections::HashMap;

use envio_mime::dkim::KeyAnswer;
use envio_net::dns::TxtAnswer;

/// Most names kept; the least recently used goes first.
const CAP: usize = 256;
/// A record is kept for its TTL, within these bounds (seconds).
const MIN_TTL_S: u32 = 60;
const MAX_TTL_S: u32 = 3600;
/// "No such record" is kept this long (seconds).
const NO_RECORD_S: i64 = 300;

#[derive(Default)]
pub(crate) struct KeyCache {
    /// Name to (answer, expiry in ms, last use).
    entries: HashMap<String, (KeyAnswer, i64, Cell<u64>)>,
    tick: Cell<u64>,
}

impl KeyCache {
    pub fn get(&self, name: &str, now_ms: i64) -> Option<&KeyAnswer> {
        let (answer, expires, used) = self.entries.get(name)?;
        if now_ms >= *expires {
            return None;
        }
        used.set(self.next_tick());
        Some(answer)
    }

    /// Records for clamp(ttl, 60 s, 3600 s); `NoRecord` for 300 s;
    /// `Unavailable` is never stored here (see `Shown.failed`).
    pub fn put(&mut self, name: String, answer: TxtAnswer, now_ms: i64) {
        let (answer, ttl_s) = match answer {
            TxtAnswer::Records { texts, ttl } => (
                KeyAnswer::Records(texts),
                i64::from(ttl.clamp(MIN_TTL_S, MAX_TTL_S)),
            ),
            TxtAnswer::NoRecord => (KeyAnswer::NoRecord, NO_RECORD_S),
            TxtAnswer::Unavailable => return,
        };
        if !self.entries.contains_key(&name) && self.entries.len() >= CAP {
            self.evict(now_ms);
        }
        let expires = now_ms.saturating_add(ttl_s * 1000);
        let used = Cell::new(self.next_tick());
        self.entries.insert(name, (answer, expires, used));
    }

    fn next_tick(&self) -> u64 {
        let tick = self.tick.get().wrapping_add(1);
        self.tick.set(tick);
        tick
    }

    /// Makes room for one name: every expired entry goes; if none was
    /// expired, the least recently used one.
    fn evict(&mut self, now_ms: i64) {
        let before = self.entries.len();
        self.entries.retain(|_, (_, expires, _)| now_ms < *expires);
        if self.entries.len() < before {
            return;
        }
        let oldest = self
            .entries
            .iter()
            .min_by_key(|(_, (_, _, used))| used.get())
            .map(|(name, _)| name.clone());
        if let Some(name) = oldest {
            self.entries.remove(&name);
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn records(ttl: u32) -> TxtAnswer {
        TxtAnswer::Records {
            texts: vec!["v=DKIM1; k=rsa; p=AAAA".into()],
            ttl,
        }
    }

    fn held(cache: &KeyCache, name: &str, now_ms: i64) -> bool {
        cache.get(name, now_ms).is_some()
    }

    #[test]
    fn a_record_lives_for_its_ttl_within_one_minute_and_one_hour() {
        let mut cache = KeyCache::default();
        cache.put("short".into(), records(5), 1_000);
        cache.put("mid".into(), records(600), 1_000);
        cache.put("long".into(), records(100_000), 1_000);
        // 5 s is raised to 60 s.
        assert!(held(&cache, "short", 1_000 + 59_999));
        assert!(!held(&cache, "short", 1_000 + 60_000));
        // 600 s is kept as is.
        assert!(held(&cache, "mid", 1_000 + 599_999));
        assert!(!held(&cache, "mid", 1_000 + 600_000));
        // 100 000 s is cut to an hour.
        assert!(held(&cache, "long", 1_000 + 3_599_999));
        assert!(!held(&cache, "long", 1_000 + 3_600_000));
    }

    #[test]
    fn the_answer_is_what_the_resolver_gave() {
        let mut cache = KeyCache::default();
        cache.put("a".into(), records(600), 0);
        assert_eq!(
            cache.get("a", 1),
            Some(&KeyAnswer::Records(vec!["v=DKIM1; k=rsa; p=AAAA".into()]))
        );
        assert_eq!(cache.get("b", 1), None);
    }

    #[test]
    fn no_record_is_kept_for_five_minutes() {
        let mut cache = KeyCache::default();
        cache.put("n".into(), TxtAnswer::NoRecord, 0);
        assert_eq!(cache.get("n", 299_999), Some(&KeyAnswer::NoRecord));
        assert_eq!(cache.get("n", 300_000), None);
    }

    #[test]
    fn unavailable_is_never_stored() {
        let mut cache = KeyCache::default();
        cache.put("u".into(), TxtAnswer::Unavailable, 0);
        assert_eq!(cache.get("u", 1), None);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn a_newer_answer_replaces_an_older_one() {
        let mut cache = KeyCache::default();
        cache.put("k".into(), TxtAnswer::NoRecord, 0);
        cache.put("k".into(), records(600), 10);
        assert!(matches!(cache.get("k", 11), Some(KeyAnswer::Records(_))));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn at_most_256_names_are_kept_and_the_least_recently_used_goes_first() {
        let mut cache = KeyCache::default();
        for i in 0..256 {
            cache.put(format!("n{i}"), records(3600), 0);
        }
        // Reading n0 makes n1 the oldest.
        assert!(held(&cache, "n0", 1));
        cache.put("extra".into(), records(3600), 0);
        assert_eq!(cache.len(), 256);
        assert!(held(&cache, "n0", 1));
        assert!(!held(&cache, "n1", 1));
        assert!(held(&cache, "extra", 1));
        for i in 0..1000 {
            cache.put(format!("m{i}"), records(3600), 0);
        }
        assert_eq!(cache.len(), 256);
    }

    #[test]
    fn expired_entries_go_before_live_ones() {
        let mut cache = KeyCache::default();
        cache.put("old".into(), TxtAnswer::NoRecord, 0);
        for i in 0..255 {
            cache.put(format!("n{i}"), records(3600), 400_000);
        }
        // "old" was used last (at a time it was still live) but expired at
        // 300 000: it is the one dropped for the next name, not n0.
        assert!(held(&cache, "old", 0));
        cache.put("next".into(), records(3600), 400_000);
        assert_eq!(cache.len(), 256);
        assert!(held(&cache, "n0", 400_001));
        assert!(held(&cache, "next", 400_001));
    }
}
