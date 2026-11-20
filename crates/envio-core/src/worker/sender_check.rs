//! Whether the From address of an open message is vouched for, and which
//! DKIM keys to look up to find out (sender trust, #285, decisions 7 and 8).
//! Pure: the cache and the clock are handed in; the driver is in
//! `sender_lookup.rs`.

use std::collections::BTreeSet;

use envio_mime::dkim::DkimVerdict;
use envio_store::{ImagesDefault, Settings};

use super::dkim_keys::KeyCache;
use crate::api::command::ImagesChoice;

/// Most key names looked up for one message.
const MAX_LOOKUPS: usize = 3;

/// What the core knows about a sender before and after its keys came.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Check {
    /// The provider vouched for it, or a DKIM signature verified with a
    /// cached key.
    pub verified: bool,
    /// The message has a DKIM signature that could verify the From address
    /// (decided from the message alone, no network).
    pub checkable: bool,
    /// The key names to look up now (at most 3).
    pub lookups: Vec<String>,
}

/// Decisions 7 and 8: the fast path, then DKIM with the cached keys; which
/// names to look up (none unless images are blocked by default, `from` is
/// in `images_from`, and the message is not verified yet). Names in
/// `failed` are never returned in `lookups`.
#[allow(clippy::too_many_arguments)] // the signature the plan fixes
pub(crate) fn check(
    raw: &[u8],
    from: &str,
    imap_host: &str,
    settings: &Settings,
    choice: ImagesChoice,
    keys: &KeyCache,
    failed: &BTreeSet<String>,
    now_ms: i64,
) -> Check {
    if envio_mime::sender_verified(raw, from, imap_host) {
        return Check {
            verified: true,
            checkable: true,
            lookups: Vec::new(),
        };
    }
    let names = envio_mime::dkim::key_queries(raw, from);
    if names.is_empty() {
        return Check::default();
    }
    // Hashing the body costs something: only do it when a key is at hand.
    let at_hand = names.iter().any(|n| keys.get(n, now_ms).is_some());
    let verdict = if at_hand {
        let now_s = u64::try_from(now_ms / 1000).unwrap_or(0);
        envio_mime::dkim::verify(raw, from, &|name| keys.get(name, now_ms), now_s)
    } else {
        DkimVerdict::NeedKeys(names)
    };
    let (verified, wanted) = match verdict {
        DkimVerdict::Pass => (true, Vec::new()),
        DkimVerdict::Fail => (false, Vec::new()),
        DkimVerdict::NeedKeys(names) => (false, names),
    };
    // Decision 7: a lookup can work as a read receipt, so only a sender the
    // user already trusts with images, for a message whose images are
    // blocked by default, is looked up.
    let may_look_up = settings.images == ImagesDefault::Ask
        && choice == ImagesChoice::Default
        && settings.images_from.contains(&from.trim().to_lowercase());
    let lookups = if may_look_up {
        wanted
            .into_iter()
            .filter(|name| !failed.contains(name))
            .take(MAX_LOOKUPS)
            .collect()
    } else {
        Vec::new()
    };
    Check {
        verified,
        checkable: true,
        lookups,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use envio_net::dns::TxtAnswer;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/../envio-mime/tests/fixtures/dkim/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    const FROM: &str = "bob@example.com";
    const NAME: &str = "test._domainkey.example.com";
    const NOW: i64 = 1_800_000_000_000;

    fn key_text() -> String {
        let keys = String::from_utf8(fixture("keys.txt")).unwrap();
        keys.lines()
            .find_map(|l| l.strip_prefix(&format!("{NAME}\t")))
            .unwrap()
            .to_owned()
    }

    fn cache_with(answer: TxtAnswer) -> KeyCache {
        let mut cache = KeyCache::default();
        cache.put(NAME.into(), answer, NOW);
        cache
    }

    fn allowed() -> Settings {
        Settings {
            images_from: [FROM.to_owned()].into(),
            ..Settings::default()
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run(
        raw: &[u8],
        settings: &Settings,
        choice: ImagesChoice,
        keys: &KeyCache,
        failed: &BTreeSet<String>,
    ) -> Check {
        check(
            raw,
            FROM,
            "outlook.office365.com",
            settings,
            choice,
            keys,
            failed,
            NOW,
        )
    }

    fn plain(raw: &[u8]) -> Check {
        run(
            raw,
            &allowed(),
            ImagesChoice::Default,
            &KeyCache::default(),
            &BTreeSet::new(),
        )
    }

    #[test]
    fn an_allowed_sender_with_a_signature_and_no_key_yet_is_looked_up() {
        let c = plain(&fixture("relaxed.eml"));
        assert_eq!(
            c,
            Check {
                verified: false,
                checkable: true,
                lookups: vec![NAME.into()],
            }
        );
    }

    #[test]
    fn a_sender_who_is_not_allowed_is_checkable_but_never_looked_up() {
        let c = run(
            &fixture("relaxed.eml"),
            &Settings::default(),
            ImagesChoice::Default,
            &KeyCache::default(),
            &BTreeSet::new(),
        );
        assert!(c.checkable && !c.verified);
        assert!(c.lookups.is_empty());
    }

    #[test]
    fn no_lookup_when_images_always_load_or_the_user_loaded_them() {
        let always = Settings {
            images: ImagesDefault::Always,
            ..allowed()
        };
        let raw = fixture("relaxed.eml");
        let none = KeyCache::default();
        let no_failed = BTreeSet::new();
        let c = run(&raw, &always, ImagesChoice::Default, &none, &no_failed);
        assert!(c.lookups.is_empty() && c.checkable);
        let c = run(&raw, &allowed(), ImagesChoice::Load, &none, &no_failed);
        assert!(c.lookups.is_empty() && c.checkable);
    }

    #[test]
    fn a_cached_key_verifies_without_a_lookup() {
        let keys = cache_with(TxtAnswer::Records {
            texts: vec![key_text()],
            ttl: 3600,
        });
        let c = run(
            &fixture("relaxed.eml"),
            &allowed(),
            ImagesChoice::Default,
            &keys,
            &BTreeSet::new(),
        );
        assert_eq!(
            c,
            Check {
                verified: true,
                checkable: true,
                lookups: vec![],
            }
        );
    }

    #[test]
    fn a_cached_key_verifies_for_a_sender_not_on_the_list_too() {
        // The cache only ever holds what an allowed sender's lookup brought;
        // using it costs no lookup.
        let keys = cache_with(TxtAnswer::Records {
            texts: vec![key_text()],
            ttl: 3600,
        });
        let c = run(
            &fixture("relaxed.eml"),
            &Settings::default(),
            ImagesChoice::Default,
            &keys,
            &BTreeSet::new(),
        );
        assert!(c.verified && c.lookups.is_empty());
    }

    #[test]
    fn a_key_that_does_not_verify_or_is_missing_leaves_it_unverified() {
        let tampered = {
            let mut raw = fixture("relaxed.eml");
            let n = raw.len();
            raw[n - 4] ^= 1;
            raw
        };
        let good = cache_with(TxtAnswer::Records {
            texts: vec![key_text()],
            ttl: 3600,
        });
        let c = run(
            &tampered,
            &allowed(),
            ImagesChoice::Default,
            &good,
            &BTreeSet::new(),
        );
        assert!(c.checkable && !c.verified && c.lookups.is_empty());
        let none = cache_with(TxtAnswer::NoRecord);
        let c = run(
            &fixture("relaxed.eml"),
            &allowed(),
            ImagesChoice::Default,
            &none,
            &BTreeSet::new(),
        );
        assert!(c.checkable && !c.verified && c.lookups.is_empty());
    }

    #[test]
    fn a_failed_name_is_not_asked_for_again() {
        let failed: BTreeSet<String> = [NAME.to_owned()].into();
        let c = run(
            &fixture("relaxed.eml"),
            &allowed(),
            ImagesChoice::Default,
            &KeyCache::default(),
            &failed,
        );
        assert!(c.checkable && !c.verified);
        assert!(c.lookups.is_empty());
    }

    #[test]
    fn unsigned_mail_is_not_checkable() {
        let raw = b"From: Bob <bob@example.com>\r\nSubject: s\r\n\r\nhi\r\n";
        let c = plain(raw);
        assert_eq!(c, Check::default());
    }

    #[test]
    fn a_header_from_the_provider_verifies_without_a_lookup() {
        let mut raw =
            b"Authentication-Results: mx.messagingengine.com; dkim=pass header.d=example.com\r\n"
                .to_vec();
        raw.extend(fixture("relaxed.eml"));
        let c = check(
            &raw,
            FROM,
            "imap.fastmail.com",
            &allowed(),
            ImagesChoice::Default,
            &KeyCache::default(),
            &BTreeSet::new(),
            NOW,
        );
        assert_eq!(
            c,
            Check {
                verified: true,
                checkable: true,
                lookups: vec![],
            }
        );
    }

    #[test]
    fn an_id_less_header_on_an_outlook_account_does_not_verify() {
        let mut raw = b"Authentication-Results: dkim=pass header.d=example.com\r\n".to_vec();
        raw.extend(fixture("relaxed.eml"));
        let c = plain(&raw);
        assert!(!c.verified && c.checkable);
        assert_eq!(c.lookups, [NAME]);
    }
}
