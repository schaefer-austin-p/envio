//! DKIM verification through the public API, on messages signed by dkimpy
//! (an independent signer; see `fixtures/dkim/make.py`).
use std::collections::HashMap;

use envio_mime::dkim::{DkimVerdict, KeyAnswer, key_queries, verify};
use proptest::prelude::*;

const FROM: &str = "bob@example.com";
/// After every `t=` in the fixtures, before nothing: a fixed "now".
const NOW: u64 = 1_800_000_000;

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/dkim/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn eml(name: &str) -> Vec<u8> {
    fixture(&format!("{name}.eml"))
}

fn keys() -> HashMap<String, KeyAnswer> {
    let text = String::from_utf8(fixture("keys.txt")).unwrap();
    text.lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(n, t)| (n.to_owned(), KeyAnswer::Records(vec![t.to_owned()])))
        .collect()
}

fn check_with(raw: &[u8], from: &str, keys: &HashMap<String, KeyAnswer>, now: u64) -> DkimVerdict {
    verify(raw, from, &|n| keys.get(n), now)
}

fn check(raw: &[u8]) -> DkimVerdict {
    check_with(raw, FROM, &keys(), NOW)
}

/// Replace the first occurrence of `from` with `to`.
fn replace(raw: &[u8], from: &str, to: &str) -> Vec<u8> {
    String::from_utf8(raw.to_vec())
        .unwrap()
        .replacen(from, to, 1)
        .into_bytes()
}

/// Change one byte of the body (or add one to an empty body).
fn tamper_body(raw: &[u8]) -> Vec<u8> {
    let at = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    let mut out = raw.to_vec();
    match out.get_mut(at) {
        Some(b) => *b = if *b == b'X' { b'Y' } else { b'X' },
        None => out.push(b'X'),
    }
    out
}

#[test]
fn every_signer_format_passes_and_fails_when_the_body_changes() {
    for name in [
        "relaxed",
        "simple",
        "relaxed_simple",
        "overs",
        "folded",
        "eightbit",
        "empty",
        "ed",
        "sub",
        "agent",
        "length",
    ] {
        let raw = eml(name);
        assert_eq!(check(&raw), DkimVerdict::Pass, "{name} passes");
        assert_eq!(
            check(&tamper_body(&raw)),
            DkimVerdict::Fail,
            "{name} tampered"
        );
    }
}

#[test]
fn a_changed_signed_header_fails_and_an_unsigned_one_does_not() {
    let raw = eml("relaxed");
    assert_eq!(
        check(&replace(&raw, "Subject: Hello", "Subject: Hellp")),
        DkimVerdict::Fail
    );
    let mut added = raw.clone();
    let at = added.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 2;
    added.splice(at..at, b"X-Unsigned: 1\r\n".iter().copied());
    assert_eq!(check(&added), DkimVerdict::Pass);
}

#[test]
fn a_signature_without_content_type_in_h_is_ignored() {
    let raw = eml("nocontent");
    assert!(key_queries(&raw, FROM).is_empty());
    assert_eq!(check(&raw), DkimVerdict::Fail);
    let html = replace(
        &raw,
        "MIME-Version",
        "Content-Type: text/html\r\nMIME-Version",
    );
    assert_eq!(check(&html), DkimVerdict::Fail);
}

#[test]
fn a_second_from_field_fails_even_with_a_valid_signature() {
    let raw = eml("relaxed");
    let two = [b"From: Bob <bob@example.com>\r\n".as_slice(), &raw].concat();
    assert!(key_queries(&two, FROM).is_empty());
    assert_eq!(check(&two), DkimVerdict::Fail);
    let two = [b"From: Mallory <mallory@example.net>\r\n".as_slice(), &raw].concat();
    assert_eq!(
        check_with(&two, "mallory@example.net", &keys(), NOW),
        DkimVerdict::Fail
    );
    assert_eq!(check(&two), DkimVerdict::Fail);
}

#[test]
fn the_signing_domain_must_align_with_from() {
    let net = eml("net");
    assert!(key_queries(&net, FROM).is_empty());
    assert_eq!(check(&net), DkimVerdict::Fail);
    assert_eq!(
        key_queries(&eml("sub"), FROM),
        ["test._domainkey.mail.example.com"]
    );
    let duck = eml("duck");
    assert!(key_queries(&duck, "bob@victim.duckdns.org").is_empty());
    assert_eq!(
        check_with(&duck, "bob@victim.duckdns.org", &keys(), NOW),
        DkimVerdict::Fail
    );
}

#[test]
fn the_from_shown_must_be_the_from_signed() {
    assert_eq!(
        check_with(&eml("relaxed"), "bob@example.org", &keys(), NOW),
        DkimVerdict::Fail
    );
}

#[test]
fn keys_not_yet_known_are_asked_for_once() {
    let raw = eml("relaxed");
    assert_eq!(key_queries(&raw, FROM), ["test._domainkey.example.com"]);
    let none = HashMap::new();
    assert_eq!(
        check_with(&raw, FROM, &none, NOW),
        DkimVerdict::NeedKeys(vec!["test._domainkey.example.com".into()])
    );
    let name = "test._domainkey.example.com".to_owned();
    let gone = HashMap::from([(name.clone(), KeyAnswer::NoRecord)]);
    assert_eq!(check_with(&raw, FROM, &gone, NOW), DkimVerdict::Fail);
    let down = HashMap::from([(name, KeyAnswer::Unavailable)]);
    assert_eq!(check_with(&raw, FROM, &down, NOW), DkimVerdict::Fail);
}

#[test]
fn an_unmatched_body_hash_asks_for_no_key() {
    let none = HashMap::new();
    assert_eq!(
        check_with(&tamper_body(&eml("relaxed")), FROM, &none, NOW),
        DkimVerdict::Fail
    );
}

#[test]
fn an_expired_signature_fails() {
    let raw = eml("xpast");
    assert_eq!(
        check_with(&raw, FROM, &keys(), 900_000_000),
        DkimVerdict::Pass
    );
    assert_eq!(check(&raw), DkimVerdict::Fail);
}

#[test]
fn a_signature_that_does_not_cover_appended_html_fails() {
    let raw = eml("length");
    assert_eq!(check(&raw), DkimVerdict::Pass);
    let mut more = raw;
    more.extend_from_slice(b"<img src=\"https://tracker.example.net/p.gif\">\r\n");
    assert_eq!(check(&more), DkimVerdict::Fail);
}

fn with_record(name: &str, record: &str) -> HashMap<String, KeyAnswer> {
    HashMap::from([(name.to_owned(), KeyAnswer::Records(vec![record.to_owned()]))])
}

fn record_of(name: &str) -> String {
    match keys().remove(name) {
        Some(KeyAnswer::Records(mut r)) => r.remove(0),
        _ => panic!("no key {name}"),
    }
}

#[test]
fn a_512_bit_rsa_key_fails() {
    let name = "small._domainkey.example.com";
    assert_eq!(check(&eml("small")), DkimVerdict::Fail);
    let keys = with_record(name, &record_of(name));
    assert_eq!(
        check_with(&eml("small"), FROM, &keys, NOW),
        DkimVerdict::Fail
    );
}

#[test]
fn the_key_type_must_match_the_algorithm() {
    let rsa = record_of("test._domainkey.example.com");
    let keys = with_record("ed._domainkey.example.com", &rsa);
    assert_eq!(check_with(&eml("ed"), FROM, &keys, NOW), DkimVerdict::Fail);
    let ed = record_of("ed._domainkey.example.com");
    let keys = with_record("test._domainkey.example.com", &ed);
    assert_eq!(
        check_with(&eml("relaxed"), FROM, &keys, NOW),
        DkimVerdict::Fail
    );
}

#[test]
fn a_revoked_key_fails() {
    let keys = with_record("test._domainkey.example.com", "v=DKIM1; k=rsa; p=");
    assert_eq!(
        check_with(&eml("relaxed"), FROM, &keys, NOW),
        DkimVerdict::Fail
    );
}

#[test]
fn a_strict_key_needs_the_agent_domain_to_equal_d() {
    let name = "test._domainkey.example.com";
    let raw = eml("agent");
    let loose = with_record(name, &record_of(name));
    assert_eq!(check_with(&raw, FROM, &loose, NOW), DkimVerdict::Pass);
    let strict = with_record(name, &format!("{}; t=s", record_of(name)));
    assert_eq!(check_with(&raw, FROM, &strict, NOW), DkimVerdict::Fail);
    assert_eq!(
        check_with(&eml("relaxed"), FROM, &strict, NOW),
        DkimVerdict::Pass
    );
}

#[test]
fn only_the_first_five_signatures_are_tried() {
    let raw = eml("relaxed");
    let end = raw.windows(2).position(|w| w == b"\r\n").unwrap();
    // The signature field runs to the first line not starting with a space.
    let mut stop = 0;
    for (i, w) in raw.windows(3).enumerate() {
        if w[0] == b'\n' && w[1] != b' ' && w[1] != b'\t' && i > end {
            stop = i + 1;
            break;
        }
    }
    let good = &raw[..stop];
    // Same length, so it still parses: its body hash is wrong.
    let at = good.windows(3).position(|w| w == b"bh=").unwrap() + 3;
    let mut broken = good.to_vec();
    broken[at..at + 4].copy_from_slice(b"AAAA");
    let message = &raw[stop..];
    let with = |n: usize| {
        let mut m = Vec::new();
        for _ in 0..n {
            m.extend_from_slice(&broken);
        }
        m.extend_from_slice(good);
        m.extend_from_slice(message);
        m
    };
    assert_eq!(check(&with(4)), DkimVerdict::Pass, "fifth is tried");
    assert_eq!(check(&with(5)), DkimVerdict::Fail, "sixth is not");
}

#[test]
fn at_most_three_key_names_are_asked_for() {
    let raw = eml("relaxed");
    let end = raw.windows(3).position(|w| w == b"\r\nF").unwrap() + 2;
    let sig = &raw[..end];
    let mut m = Vec::new();
    for s in ["a", "b", "c", "d", "a"] {
        m.extend_from_slice(&replace(sig, "s=test", &format!("s={s}")));
    }
    m.extend_from_slice(&raw[end..]);
    assert_eq!(
        key_queries(&m, FROM),
        [
            "a._domainkey.example.com",
            "b._domainkey.example.com",
            "c._domainkey.example.com"
        ]
    );
}

/// RFC 8463 Appendix A.3, signed with both an RSA and an Ed25519 key. The
/// `h=` lists no `content-type`, so the public API ignores it.
#[test]
fn the_rfc_8463_message_is_ignored_for_lack_of_content_type() {
    let raw = fixture("rfc8463.eml");
    let from = "joe@football.example.com";
    assert!(key_queries(&raw, from).is_empty());
    assert_eq!(
        check_with(&raw, from, &HashMap::new(), NOW),
        DkimVerdict::Fail
    );
}

proptest! {
    #[test]
    fn verify_never_panics(
        raw in proptest::collection::vec(any::<u8>(), 0..1500),
        text in ".{0,200}",
    ) {
        let mut keys = HashMap::new();
        keys.insert("test._domainkey.example.com".to_owned(), KeyAnswer::Records(vec![text]));
        let _ = verify(&raw, FROM, &|n| keys.get(n), NOW);
        let _ = key_queries(&raw, FROM);
    }

    #[test]
    fn verify_never_panics_on_a_damaged_fixture(cut in 0usize..2000, byte in any::<u8>(), at in 0usize..2000) {
        let mut raw = eml("relaxed");
        let n = raw.len();
        raw[at % n] = byte;
        raw.truncate(cut.max(1) % n + 1);
        let _ = check(&raw);
    }
}

/// `eml(name)`'s signature field copied once per selector, above the rest.
fn with_selectors(name: &str, selectors: &[&str]) -> Vec<u8> {
    let raw = eml(name);
    let end = raw.windows(3).position(|w| w == b"\r\nF").unwrap() + 2;
    let mut m = Vec::new();
    for s in selectors {
        m.extend_from_slice(&replace(&raw[..end], "s=test", &format!("s={s}")));
    }
    m.extend_from_slice(&raw[end..]);
    m
}

#[test]
fn a_fourth_key_name_is_never_a_need() {
    let m = with_selectors("relaxed", &["a", "b", "c", "d"]);
    let gone = |s: &str| (format!("{s}._domainkey.example.com"), KeyAnswer::NoRecord);
    let keys: HashMap<String, KeyAnswer> = ["a", "b", "c"].into_iter().map(gone).collect();
    assert_eq!(check_with(&m, FROM, &keys, NOW), DkimVerdict::Fail);
    // Without the answer for the third, that one is still asked for.
    let keys: HashMap<String, KeyAnswer> = ["a", "b"].into_iter().map(gone).collect();
    assert_eq!(
        check_with(&m, FROM, &keys, NOW),
        DkimVerdict::NeedKeys(vec!["c._domainkey.example.com".into()])
    );
}

fn records(list: Vec<String>) -> HashMap<String, KeyAnswer> {
    HashMap::from([(
        "test._domainkey.example.com".into(),
        KeyAnswer::Records(list),
    )])
}

/// A well-formed record that does not verify `relaxed.eml`.
const OTHER_KEY: &str = "v=DKIM1; k=ed25519; p=11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=";

#[test]
fn every_record_of_a_name_is_tried() {
    let good = record_of("test._domainkey.example.com");
    let both = records(vec![OTHER_KEY.into(), good.clone()]);
    assert_eq!(
        check_with(&eml("relaxed"), FROM, &both, NOW),
        DkimVerdict::Pass
    );
    let junk = records(vec!["not a record".into(), good]);
    assert_eq!(
        check_with(&eml("relaxed"), FROM, &junk, NOW),
        DkimVerdict::Pass
    );
}

#[test]
fn at_most_five_signature_checks_are_made() {
    let good = record_of("test._domainkey.example.com");
    let with = |wrong: usize| {
        let mut list = vec![OTHER_KEY.to_owned(); wrong];
        list.push(good.clone());
        records(list)
    };
    assert_eq!(
        check_with(&eml("relaxed"), FROM, &with(4), NOW),
        DkimVerdict::Pass
    );
    assert_eq!(
        check_with(&eml("relaxed"), FROM, &with(5), NOW),
        DkimVerdict::Fail
    );
}

#[test]
fn a_signed_content_transfer_encoding_passes() {
    assert_eq!(check(&eml("cte_signed")), DkimVerdict::Pass);
}

#[test]
fn an_unsigned_content_transfer_encoding_added_below_fails() {
    let raw = eml("cte_appended");
    assert!(key_queries(&raw, FROM).is_empty());
    assert_eq!(check(&raw), DkimVerdict::Fail);
}

#[test]
fn every_content_type_field_must_be_signed() {
    assert_eq!(check(&eml("ct_twice")), DkimVerdict::Pass);
    // An extra Content-Type above the signed one: DKIM still verifies the
    // bottom one, but the signature does not cover every such field.
    let raw = replace(
        &eml("relaxed"),
        "\r\nFrom:",
        "\r\nContent-Type: text/html\r\nFrom:",
    );
    assert!(key_queries(&raw, FROM).is_empty());
    assert_eq!(check(&raw), DkimVerdict::Fail);
}
