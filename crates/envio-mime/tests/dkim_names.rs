//! The names of header lines: DKIM, the guard and the reader must read one
//! name alike (re-audit finding on PR #336: `Subject\x0c: evil` added below a
//! signed message was the Subject for mail-parser and no field named subject
//! for DKIM, so the signature still passed).
use std::collections::HashMap;

use envio_mime::dkim::{DkimVerdict, KeyAnswer, verify};
use mail_parser::HeaderName;
use proptest::prelude::*;

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

fn check(raw: &[u8], keys: &HashMap<String, KeyAnswer>) -> DkimVerdict {
    verify(raw, FROM, &|n| keys.get(n), NOW)
}

/// What the reader shows of the message: the headers a user sees or that
/// steer decoding (keyed by lowercase name), the plain body and the part
/// count.
fn reader_view(raw: &[u8]) -> Vec<(String, String)> {
    let message = mail_parser::MessageParser::default().parse(raw).unwrap();
    let mut view: Vec<(String, String)> = [
        ("subject", HeaderName::Subject),
        ("to", HeaderName::To),
        ("date", HeaderName::Date),
        ("message-id", HeaderName::MessageId),
        ("from", HeaderName::From),
        ("content-type", HeaderName::ContentType),
        (
            "content-transfer-encoding",
            HeaderName::ContentTransferEncoding,
        ),
        ("mime-version", HeaderName::MimeVersion),
    ]
    .into_iter()
    .map(|(n, h)| (n.to_owned(), format!("{:?}", message.header(h))))
    .collect();
    view.push(("body".into(), format!("{:?}", message.body_text(0))));
    view.push(("parts".into(), message.parts.len().to_string()));
    view
}

/// The names in the first signature's `h=` of a fixture, lowercase.
fn signed_names(raw: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(raw);
    let head = text.split("\r\n\r\n").next().unwrap_or("");
    let start = head.find("DKIM-Signature:").expect("signature");
    let mut end = head.len();
    for (i, _) in head[start..].match_indices("\r\n") {
        let next = head.as_bytes().get(start + i + 2).copied();
        if next.is_some_and(|c| c != b' ' && c != b'\t') {
            end = start + i;
            break;
        }
    }
    let sig: String = head[start..end]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let h = sig
        .split(";h=")
        .nth(1)
        .expect("h=")
        .split(';')
        .next()
        .unwrap();
    h.split(':').map(str::to_ascii_lowercase).collect()
}

/// The view's entries the signature protects: its `h=` names, Content-Type
/// and Content-Transfer-Encoding (the count rule), the body and the parts.
fn protected(view: Vec<(String, String)>, signed: &[String]) -> Vec<(String, String)> {
    view.into_iter()
        .filter(|(n, _)| {
            signed.contains(n)
                || matches!(
                    n.as_str(),
                    "content-type" | "content-transfer-encoding" | "body" | "parts"
                )
        })
        .collect()
}

/// The offsets where a header line starts, up to the blank line.
fn line_starts(raw: &[u8]) -> Vec<usize> {
    let mut at = vec![0];
    for (i, &b) in raw.iter().enumerate() {
        if b == b'\n' {
            at.push(i + 1);
            let rest = &raw[i + 1..];
            if rest.starts_with(b"\r\n") || rest.starts_with(b"\n") {
                break;
            }
        }
    }
    at
}

const NAMES: &[&str] = &[
    "Subject",
    "To",
    "Date",
    "Message-ID",
    "From",
    "Content-Type",
    "Content-Transfer-Encoding",
    "MIME-Version",
    "X-Junk",
];

/// Bytes put between the name and the colon.
const AFTER: &[&[u8]] = &[
    b"", b"\x0c", b"\x0b", b"\0", b"\r", b" \x0c", b"\x0c ", b" ", b"\t", b"\x01", b"\x7f",
    b"\xa0", b" \r", b"\r ",
];
/// Bytes put before the name.
const BEFORE: &[&[u8]] = &[b"", b"\x0c", b"\r", b"\x0b", b"\0", b"\x01"];

fn junk_line(before: &[u8], name: &str, upper: bool, after: &[u8], eol: &[u8]) -> Vec<u8> {
    let name = if upper {
        name.to_ascii_uppercase()
    } else {
        name.to_owned()
    };
    [before, name.as_bytes(), after, b": evil", eol].concat()
}

fn inserted(raw: &[u8], at: usize, line: &[u8]) -> Vec<u8> {
    let mut bad = raw[..at].to_vec();
    bad.extend_from_slice(line);
    bad.extend_from_slice(&raw[at..]);
    bad
}

#[test]
fn a_control_byte_in_a_signed_name_added_below_does_not_pass() {
    let keys = keys();
    let raw = fixture("relaxed.eml");
    let end = *line_starts(&raw).last().unwrap();
    for name in ["Subject", "To", "Date", "Message-ID"] {
        for pad in ["\x0c", "\x0b", "\0", "\r"] {
            for eol in ["\r\n", "\n"] {
                let line = junk_line(b"", name, false, pad.as_bytes(), eol.as_bytes());
                let bad = inserted(&raw, end, &line);
                assert_ne!(
                    check(&bad, &keys),
                    DkimVerdict::Pass,
                    "{name}{pad:?} {eol:?}"
                );
            }
        }
    }
}

/// On every fixture that passes, a junk line of every kind (control bytes in
/// or around the name of each header, both line endings, every line start)
/// either breaks the pass or leaves what the reader shows unchanged.
#[test]
fn every_junk_line_at_every_line_start_of_every_fixture_leaves_a_pass_unchanged() {
    let keys = keys();
    let (mut fixtures, mut passes, mut inserts) = (0, 0, 0);
    let dir = format!("{}/tests/fixtures/dkim", env!("CARGO_MANIFEST_DIR"));
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "eml") {
            continue;
        }
        let raw = std::fs::read(&path).unwrap();
        if check(&raw, &keys) != DkimVerdict::Pass {
            continue;
        }
        fixtures += 1;
        let signed = signed_names(&raw);
        let view = protected(reader_view(&raw), &signed);
        for at in line_starts(&raw) {
            for name in NAMES {
                for before in BEFORE {
                    for after in AFTER {
                        for (upper, eol) in [(false, &b"\r\n"[..]), (true, &b"\n"[..])] {
                            let line = junk_line(before, name, upper, after, eol);
                            let bad = inserted(&raw, at, &line);
                            inserts += 1;
                            if check(&bad, &keys) == DkimVerdict::Pass {
                                passes += 1;
                                assert_eq!(
                                    protected(reader_view(&bad), &signed),
                                    view,
                                    "{}: {line:?} at {at}",
                                    path.display()
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    eprintln!("fixtures {fixtures}, inserts {inserts}, still Pass {passes}");
    assert!(fixtures >= 10, "{fixtures} fixtures pass");
}

proptest! {
    #[test]
    fn random_junk_lines_never_change_the_reader_under_a_pass(
        name in prop::sample::select(NAMES),
        before in prop::sample::select(BEFORE),
        after in prop::sample::select(AFTER),
        upper in any::<bool>(),
        lf in any::<bool>(),
        pick in any::<prop::sample::Index>(),
    ) {
        let keys = keys();
        let raw = fixture("relaxed.eml");
        let signed = signed_names(&raw);
        let starts = line_starts(&raw);
        let at = starts[pick.index(starts.len())];
        let eol: &[u8] = if lf { b"\n" } else { b"\r\n" };
        let line = junk_line(before, name, upper, after, eol);
        let bad = inserted(&raw, at, &line);
        prop_assert!(
            check(&bad, &keys) != DkimVerdict::Pass
                || protected(reader_view(&bad), &signed) == protected(reader_view(&raw), &signed),
            "{line:?} at {at}"
        );
    }
}
