//! The header/body boundary: DKIM and the reader (mail-parser) must see the
//! same body, or a signed message could carry text the signature never
//! covered, shown as the body under a verified sender (audit finding on
//! PR #336). Fixtures are the dkimpy-signed ones (`fixtures/dkim/make.py`).
use std::collections::HashMap;

use envio_mime::dkim::{DkimVerdict, KeyAnswer, key_queries, verify};

const FROM: &str = "bob@example.com";
const NOW: u64 = 1_800_000_000;

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/dkim/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn keys() -> HashMap<String, KeyAnswer> {
    let text = String::from_utf8(fixture("keys.txt")).unwrap();
    text.lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(n, t)| (n.to_owned(), KeyAnswer::Records(vec![t.to_owned()])))
        .collect()
}

fn check(raw: &[u8]) -> DkimVerdict {
    let keys = keys();
    verify(raw, FROM, &|n| keys.get(n), NOW)
}

/// The relaxed fixture with `insert` placed right after its From line.
fn after_from(insert: &str) -> Vec<u8> {
    let raw = String::from_utf8(fixture("relaxed.eml")).unwrap();
    let to = raw.find("\r\nTo:").expect("To field") + 2;
    format!("{}{insert}{}", &raw[..to], &raw[to..]).into_bytes()
}

/// The text the reader shows as the plain body.
fn reader_body(raw: &[u8]) -> String {
    let message = mail_parser::MessageParser::default().parse(raw).unwrap();
    message
        .body_text(0)
        .map(|t| t.into_owned())
        .unwrap_or_default()
}

#[test]
fn the_unmodified_fixture_passes() {
    assert_eq!(check(&fixture("relaxed.eml")), DkimVerdict::Pass);
}

#[test]
fn a_cr_cr_lf_line_cannot_start_an_unsigned_body() {
    // The reader ends the headers at "\r\r\n" and shows everything after it
    // as the body; the old DKIM split read it as a junk header and carried on.
    let raw = after_from("\r\r\nPHISH: send money to X\r\n");
    assert!(
        reader_body(&raw).starts_with("PHISH: send money to X"),
        "the reader shows the injected text as the body"
    );
    assert_eq!(check(&raw), DkimVerdict::Fail);
}

#[test]
fn every_line_ending_mix_that_ends_the_headers_early_fails() {
    for blank in ["\r\r\n", "\r\r\r\n", "\n", "\r\n", "\x0c\r\n", "\r \r\n"] {
        let raw = after_from(&format!("{blank}PHISH: send money to X\r\n"));
        assert_eq!(check(&raw), DkimVerdict::Fail, "{blank:?}");
    }
}

#[test]
fn a_header_line_the_reader_reads_differently_never_passes() {
    // Lines DKIM sees as a differently named field (or junk) but the reader
    // reads as a field of its own: a second From the signature never covered.
    for line in [
        "\rFrom: Mallory <bob@example.com>\r\n",
        "\x0cFrom: Mallory <bob@example.com>\r\n",
        "no colon at all\r\n",
    ] {
        let raw = fixture("relaxed.eml");
        let at = String::from_utf8_lossy(&raw)
            .find("\r\nMIME-Version")
            .unwrap()
            + 2;
        let mut bad = raw[..at].to_vec();
        bad.extend_from_slice(line.as_bytes());
        bad.extend_from_slice(&raw[at..]);
        assert_eq!(check(&bad), DkimVerdict::Fail, "{line:?}");
        assert!(key_queries(&bad, FROM).is_empty(), "{line:?}");
    }
}
