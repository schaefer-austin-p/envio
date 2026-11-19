use super::canon::*;
use super::signature::{Algorithm, Canon, Signature};
use proptest::prelude::*;
use std::borrow::Cow;

const SP: Canon = Canon::Simple;
const RX: Canon = Canon::Relaxed;

fn sha(data: &[u8]) -> [u8; 32] {
    let d = ring::digest::digest(&ring::digest::SHA256, data);
    d.as_ref().try_into().unwrap()
}

fn sig(h: &[&str], canon: Canon) -> Signature {
    Signature {
        algorithm: Algorithm::RsaSha256,
        header_canon: canon,
        body_canon: canon,
        domain: "example.com".into(),
        selector: "s".into(),
        signed_headers: h.iter().map(|s| s.to_string()).collect(),
        body_hash: vec![],
        signature: vec![],
        body_length: None,
        agent: None,
        expires: None,
    }
}

/// `signed_data` for a signature field that is not among the message's
/// fields yet: it goes at the bottom, as DKIM-Signature fields are added.
fn signed_data(fields: &[&[u8]], sig_field: &[u8], sig: &Signature) -> Vec<u8> {
    let mut all = fields.to_vec();
    all.push(sig_field);
    super::canon::signed_data(&all, all.len() - 1, sig).unwrap()
}

fn joined(raw: &[u8], c: Canon) -> Vec<u8> {
    header_fields(raw)
        .iter()
        .flat_map(|f| canon_field(f, c))
        .collect()
}

// RFC 6376 3.4.6.
const RFC_HEADERS: &[u8] = b"A: X\r\nB : Y\t\r\n\tZ  \r\n";
const RFC_BODY: &[u8] = b" C \r\nD \t E\r\n\r\n\r\n";

#[test]
fn rfc_example_headers() {
    let f = header_fields(RFC_HEADERS);
    assert_eq!(f, vec![&b"A: X\r\n"[..], &b"B : Y\t\r\n\tZ  \r\n"[..]]);
    assert_eq!(joined(RFC_HEADERS, SP), RFC_HEADERS);
    assert_eq!(joined(RFC_HEADERS, RX), b"a:X\r\nb:Y Z\r\n");
}

#[test]
fn rfc_example_body() {
    assert_eq!(canon_body(RFC_BODY, SP), b" C \r\nD \t E\r\n");
    assert_eq!(canon_body(RFC_BODY, RX), b" C\r\nD E\r\n");
    assert_eq!(
        body_hash(RFC_BODY, SP, None),
        Some(sha(b" C \r\nD \t E\r\n"))
    );
    assert_eq!(body_hash(RFC_BODY, RX, None), Some(sha(b" C\r\nD E\r\n")));
}

#[test]
fn empty_body_hashes() {
    assert_eq!(body_hash(b"", RX, None), Some(sha(b"")));
    assert_eq!(body_hash(b"", SP, None), Some(sha(b"\r\n")));
    assert_eq!(body_hash(b"\r\n\r\n", RX, None), Some(sha(b"")));
    assert_eq!(body_hash(b"\r\n\r\n", SP, None), Some(sha(b"\r\n")));
    assert_eq!(body_hash(b" \t\r\n \r\n", RX, None), Some(sha(b"")));
    // The well-known constants, so `sha` above is not the only witness.
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD;
    assert_eq!(
        b64.encode(body_hash(b"", SP, None).unwrap()),
        "frcCV1k9oG9oKj3dpUqdJg1PxRT2RSN/XKdLCPjaYaY="
    );
    assert_eq!(
        b64.encode(body_hash(b"", RX, None).unwrap()),
        "47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU="
    );
}

#[test]
fn missing_final_crlf_is_added() {
    assert_eq!(canon_body(b"abc", SP), b"abc\r\n");
    assert_eq!(canon_body(b"abc", RX), b"abc\r\n");
    assert_eq!(canon_body(b"abc\r\n\r\n\r\n", SP), b"abc\r\n");
}

#[test]
fn relaxed_body_wsp_rules() {
    assert_eq!(canon_body(b"a \t b\r\n", RX), b"a b\r\n");
    assert_eq!(canon_body(b"a\t\tb\r\n", RX), b"a b\r\n");
    assert_eq!(canon_body(b"a  \r\n", RX), b"a\r\n");
    assert_eq!(canon_body(b"a\t\r\n", RX), b"a\r\n");
    // Leading WSP is reduced, not removed.
    assert_eq!(canon_body(b" \t a\r\n", RX), b" a\r\n");
    // A lone CR, FF and NBSP are not WSP.
    assert_eq!(canon_body(b"a\r \r\n", RX), b"a\r\r\n");
    assert_eq!(canon_body(b"a \r b\r\n", RX), b"a \r b\r\n");
    assert_eq!(canon_body(b"a\x0c\r\n", RX), b"a\x0c\r\n");
    assert_eq!(canon_body(b"a\xc2\xa0\r\n", RX), b"a\xc2\xa0\r\n");
    // WSP-only middle lines stay as empty lines; trailing ones go.
    assert_eq!(
        canon_body(b"a\r\n \r\nb\r\n \r\n\t\r\n", RX),
        b"a\r\n\r\nb\r\n"
    );
    // Simple keeps all of it.
    assert_eq!(canon_body(b"a \t b \r\n \r\n", SP), b"a \t b \r\n \r\n");
}

#[test]
fn simple_body_keeps_non_utf8_bytes() {
    let b = b"\xff\xfe caf\xe9\r\n";
    assert_eq!(canon_body(b, SP), b);
    assert_eq!(body_hash(b, SP, None), Some(sha(b)));
}

#[test]
fn relaxed_header_rules() {
    assert_eq!(
        canon_field(b"Subject:  a \t b  \r\n", RX),
        b"subject:a b\r\n"
    );
    assert_eq!(canon_field(b"SUBJECT :\tx\r\n", RX), b"subject:x\r\n");
    assert_eq!(canon_field(b"X: a\r\n b\r\n\tc\r\n", RX), b"x:a b c\r\n");
    assert_eq!(canon_field(b"X:\r\n a\r\n", RX), b"x:a\r\n");
    assert_eq!(canon_field(b"X:   \r\n", RX), b"x:\r\n");
    // A lone CR is not WSP.
    assert_eq!(canon_field(b"X: a\r b\r\n", RX), b"x:a\r b\r\n");
    // Simple keeps case, spaces and folds.
    assert_eq!(
        canon_field(b"Subject:  a \t b  \r\n", SP),
        b"Subject:  a \t b  \r\n"
    );
    assert_eq!(canon_field(b"X: a\r\n b\r\n", SP), b"X: a\r\n b\r\n");
    // Non-UTF-8 bytes survive both forms.
    assert_eq!(canon_field(b"X: \xff\r\n", SP), b"X: \xff\r\n");
    assert_eq!(canon_field(b"X: \xff\r\n", RX), b"x:\xff\r\n");
}

#[test]
fn bare_lf_is_crlf() {
    let lf = b"From: a@example.com\nSubject: hi\n there\n\nline one\n\nline two  \n\n";
    let crlf =
        b"From: a@example.com\r\nSubject: hi\r\n there\r\n\r\nline one\r\n\r\nline two  \r\n\r\n";
    assert_eq!(header_fields(lf).len(), 2);
    assert_eq!(body(lf).as_ref(), body(crlf).as_ref());
    assert_eq!(body(lf).as_ref(), b"line one\r\n\r\nline two  \r\n\r\n");
    for c in [SP, RX] {
        assert_eq!(
            body_hash(&body(lf), c, None),
            body_hash(&body(crlf), c, None)
        );
        assert_eq!(joined(lf, c), joined(crlf, c));
        let s = sig(&["from", "subject"], c);
        assert_eq!(
            signed_data(&header_fields(lf), b"DKIM-Signature: b=x\n", &s),
            signed_data(&header_fields(crlf), b"DKIM-Signature: b=x\r\n", &s)
        );
    }
    assert_eq!(canon_field(b"X: a\n b\n", SP), b"X: a\r\n b\r\n");
    assert_eq!(canon_field(b"X: a\r\n b", SP), b"X: a\r\n b\r\n");
}

#[test]
fn header_fields_split() {
    let raw = b"A: 1\r\nB: 2\r\n  cont\r\n\tmore\r\nC: 3\r\n\r\nD: not a header\r\n";
    assert_eq!(
        header_fields(raw),
        vec![
            &b"A: 1\r\n"[..],
            &b"B: 2\r\n  cont\r\n\tmore\r\n"[..],
            &b"C: 3\r\n"[..]
        ]
    );
    // No blank line: all of it is header, the last field unterminated.
    assert_eq!(
        header_fields(b"A: 1\r\nB: 2"),
        vec![&b"A: 1\r\n"[..], &b"B: 2"[..]]
    );
    // A blank line first: no headers.
    assert!(header_fields(b"\r\nA: 1\r\n").is_empty());
    assert!(header_fields(b"\nA: 1\n").is_empty());
    assert!(header_fields(b"").is_empty());
}

#[test]
fn body_split() {
    assert_eq!(
        body(b"A: 1\r\n\r\nhello\r\n\r\nworld").as_ref(),
        b"hello\r\n\r\nworld"
    );
    assert_eq!(body(b"A: 1\r\nB: 2\r\n").as_ref(), b"");
    assert_eq!(body(b"\r\nhello").as_ref(), b"hello");
    assert_eq!(body(b"A: 1\n\nx\ny").as_ref(), b"x\r\ny");
    assert!(matches!(body(b"A: 1\r\n\r\nx\r\n"), Cow::Borrowed(_)));
    assert_eq!(body(b"A: 1\r\n\r\nx\ry\r\n").as_ref(), b"x\ry\r\n");
    assert_eq!(body(b"A: 1\r\n\r\n\xff\xfe\r\n").as_ref(), b"\xff\xfe\r\n");
    // CRLF already there is not doubled, mixed endings are fixed.
    assert_eq!(
        body(b"A: 1\r\n\r\nx\r\ny\nz\r\n").as_ref(),
        b"x\r\ny\r\nz\r\n"
    );
}

#[test]
fn l_equal_shorter_longer() {
    let b = b"hello\r\n";
    assert_eq!(body_hash(b, SP, Some(7)), Some(sha(b)));
    assert_eq!(body_hash(b, RX, Some(7)), Some(sha(b)));
    assert_eq!(body_hash(b, SP, Some(6)), None);
    assert_eq!(body_hash(b, SP, Some(0)), None);
    assert_eq!(body_hash(b, SP, Some(8)), None);
    assert_eq!(body_hash(b, SP, Some(u64::MAX)), None);
    // l counts canonical bytes: relaxed shrinks "a  \r\n" to 3.
    assert_eq!(body_hash(b"a  \r\n", RX, Some(3)), Some(sha(b"a\r\n")));
    assert_eq!(body_hash(b"a  \r\n", RX, Some(5)), None);
    assert_eq!(body_hash(b"a  \r\n", SP, Some(5)), Some(sha(b"a  \r\n")));
    // Empty body: simple is one CRLF (2), relaxed is 0.
    assert_eq!(body_hash(b"", SP, Some(2)), Some(sha(b"\r\n")));
    assert_eq!(body_hash(b"", SP, Some(0)), None);
    assert_eq!(body_hash(b"", RX, Some(0)), Some(sha(b"")));
    assert_eq!(body_hash(b"", RX, Some(1)), None);
}

#[test]
fn bottom_up_selection() {
    let raw = b"Received: first\r\nReceived: second\r\nFrom: a@example.com\r\n\r\n";
    let f = header_fields(raw);
    let s = sig(&["received", "received", "from"], SP);
    assert_eq!(
        signed_data(&f, b"DKIM-Signature: b=\r\n", &s),
        b"Received: second\r\nReceived: first\r\nFrom: a@example.com\r\nDKIM-Signature: b="
    );
    let s1 = sig(&["received"], SP);
    assert_eq!(
        signed_data(&f, b"DKIM-Signature: b=\r\n", &s1),
        b"Received: second\r\nDKIM-Signature: b="
    );
    // Three fields, two listed: the lower two, never the top one.
    let raw3 = b"R: 1\r\nR: 2\r\nR: 3\r\n\r\n";
    let s2 = sig(&["r", "r"], SP);
    assert_eq!(
        signed_data(&header_fields(raw3), b"D: b=\r\n", &s2),
        b"R: 3\r\nR: 2\r\nD: b="
    );
}

#[test]
fn over_signing_adds_nothing() {
    let raw = b"From: a@example.com\r\nTo: b@example.com\r\n\r\n";
    let f = header_fields(raw);
    let s = sig(&["from", "from", "to"], SP);
    assert_eq!(
        signed_data(&f, b"DKIM-Signature: b=\r\n", &s),
        b"From: a@example.com\r\nTo: b@example.com\r\nDKIM-Signature: b="
    );
    let s = sig(&["content-type", "from"], SP);
    assert_eq!(
        signed_data(&f, b"DKIM-Signature: b=\r\n", &s),
        b"From: a@example.com\r\nDKIM-Signature: b="
    );
}

#[test]
fn extra_from_above_changes_nothing() {
    let signed = b"From: a@example.com\r\nSubject: s\r\n\r\n";
    let attacked = b"From: evil@example.net\r\nFrom: a@example.com\r\nSubject: s\r\n\r\n";
    let s = sig(&["from", "subject"], RX);
    let sf = b"DKIM-Signature: b=\r\n";
    assert_eq!(
        signed_data(&header_fields(signed), sf, &s),
        signed_data(&header_fields(attacked), sf, &s)
    );
    let below = b"From: a@example.com\r\nFrom: evil@example.net\r\nSubject: s\r\n\r\n";
    assert_ne!(
        signed_data(&header_fields(signed), sf, &s),
        signed_data(&header_fields(below), sf, &s)
    );
}

#[test]
fn names_match_case_insensitively_and_relaxed_output_is_canonical() {
    let raw = b"FROM: a@example.com\r\nsubject : A  B\r\n\r\n";
    let s = sig(&["from", "subject"], RX);
    assert_eq!(
        signed_data(&header_fields(raw), b"DKIM-Signature: v=1; b=zz\r\n", &s),
        b"from:a@example.com\r\nsubject:A B\r\ndkim-signature:v=1; b="
    );
}

#[test]
fn sig_field_b_emptied_only() {
    let s = sig(&[], SP);
    let d = |f: &[u8]| signed_data(&[], f, &s);
    assert_eq!(
        d(b"DKIM-Signature: v=1; b=QUJD; bh=eHl6\r\n"),
        b"DKIM-Signature: v=1; b=; bh=eHl6"
    );
    assert_eq!(
        d(b"DKIM-Signature: v=1; b= QUJD ; bh=x\r\n"),
        b"DKIM-Signature: v=1; b=; bh=x"
    );
    assert_eq!(
        d(b"DKIM-Signature: b =QUJD;v=1\r\n"),
        b"DKIM-Signature: b =;v=1"
    );
    // bh= is not b=, a name ending in b is not b=.
    assert_eq!(
        d(b"DKIM-Signature: bh=QUJD; xb=1; b=Q\r\n"),
        b"DKIM-Signature: bh=QUJD; xb=1; b="
    );
    assert_eq!(
        d(b"DKIM-Signature: bh=QUJD\r\n"),
        b"DKIM-Signature: bh=QUJD"
    );
    assert_eq!(
        d(b"DKIM-Signature: v=1; b=QUJD\r\n"),
        b"DKIM-Signature: v=1; b="
    );
    // A value folded over lines: the whole value goes, folds included.
    assert_eq!(
        d(b"DKIM-Signature: v=1; b=QU\r\n JD\r\n\tRU; bh=x\r\n"),
        b"DKIM-Signature: v=1; b=; bh=x"
    );
    assert_eq!(
        d(b"DKIM-Signature: b=; v=1\r\n"),
        b"DKIM-Signature: b=; v=1"
    );
    // Folds elsewhere stay for simple.
    assert_eq!(
        d(b"DKIM-Signature: v=1;\r\n b=QUJD;\r\n bh=x\r\n"),
        b"DKIM-Signature: v=1;\r\n b=;\r\n bh=x"
    );
    // b= as the first tag, with and without a space after the colon.
    assert_eq!(
        d(b"DKIM-Signature:b=QUJD;v=1\r\n"),
        b"DKIM-Signature:b=;v=1"
    );
    assert_eq!(
        d(b"DKIM-Signature: b=QUJD;v=1\r\n"),
        b"DKIM-Signature: b=;v=1"
    );
    // An equals sign inside the value does not end it early.
    assert_eq!(
        d(b"DKIM-Signature: b=QU==; v=1\r\n"),
        b"DKIM-Signature: b=; v=1"
    );
}

#[test]
fn sig_field_b_folded_simple() {
    let s = sig(&[], SP);
    let d = |f: &[u8]| signed_data(&[], f, &s);
    // RFC 6376 3.5: the b= value is deleted "including all surrounding
    // whitespace". Folded right after the `=`, in the middle, with a `;`:
    assert_eq!(
        d(b"DKIM-Signature: v=1; b=\r\n QUJD\r\n RUZH;\r\n bh=x\r\n"),
        b"DKIM-Signature: v=1; b=;\r\n bh=x"
    );
    // ... and with whitespace before the `;` too.
    assert_eq!(
        d(b"DKIM-Signature: v=1; b=\r\n QUJD\r\n RUZH \r\n ; bh=x\r\n"),
        b"DKIM-Signature: v=1; b=; bh=x"
    );
    // ... and as the last tag, with and without a final `;`.
    assert_eq!(
        d(b"DKIM-Signature: v=1; b=\r\n QUJD\r\n RUZH\r\n"),
        b"DKIM-Signature: v=1; b="
    );
    assert_eq!(
        d(b"DKIM-Signature: v=1; b=\r\n QUJD\r\n RUZH;\r\n"),
        b"DKIM-Signature: v=1; b=;"
    );
}

#[test]
fn sig_field_relaxed_form() {
    let s = sig(&[], RX);
    assert_eq!(
        signed_data(&[], b"DKIM-Signature:  v=1;\r\n  b=QUJD ;  bh=x  \r\n", &s),
        b"dkim-signature:v=1; b=; bh=x"
    );
}

#[test]
fn sig_field_b_folded_relaxed() {
    let s = sig(&[], RX);
    let d = |f: &[u8]| signed_data(&[], f, &s);
    assert_eq!(
        d(b"DKIM-Signature: v=1; b=\r\n QUJD\r\n RUZH;\r\n bh=x\r\n"),
        b"dkim-signature:v=1; b=; bh=x"
    );
    assert_eq!(
        d(b"DKIM-Signature: v=1; b=\r\n QUJD\r\n RUZH;\r\n"),
        b"dkim-signature:v=1; b=;"
    );
    assert_eq!(
        d(b"DKIM-Signature: v=1; b=\r\n QUJD\r\n RUZH\r\n"),
        b"dkim-signature:v=1; b="
    );
}

#[test]
fn h_dkim_signature_never_selects_the_signature_itself() {
    // The signature being checked is the lower one; `h=dkim-signature`
    // must pick the other one, not it (its b= would be unemptied).
    let raw =
        b"DKIM-Signature: other\r\nFrom: a@example.com\r\nDKIM-Signature: v=1; b=QUJD\r\n\r\n";
    let f = header_fields(raw);
    let own = f[2];
    let s = sig(&["dkim-signature", "from"], SP);
    assert_eq!(
        super::canon::signed_data(&f, 2, &s).unwrap(),
        b"DKIM-Signature: other\r\nFrom: a@example.com\r\nDKIM-Signature: v=1; b="
    );
    assert_eq!(f[2], own);
    // Alone, it contributes only its own emptied copy, once.
    let raw1 = b"DKIM-Signature: v=1; b=QUJD\r\n\r\n";
    let f1 = header_fields(raw1);
    let s1 = sig(&["dkim-signature"], SP);
    assert_eq!(
        super::canon::signed_data(&f1, 0, &s1).unwrap(),
        b"DKIM-Signature: v=1; b="
    );
    // Listed twice with one other: the other once, then nothing.
    let s2 = sig(&["dkim-signature", "dkim-signature"], SP);
    assert_eq!(
        super::canon::signed_data(&f, 2, &s2).unwrap(),
        b"DKIM-Signature: other\r\nDKIM-Signature: v=1; b="
    );
}

#[test]
fn signature_index_out_of_range_is_none() {
    let raw = b"From: a@example.com\r\nDKIM-Signature: b=x\r\n\r\n";
    let f = header_fields(raw);
    let s = sig(&["from"], SP);
    assert!(super::canon::signed_data(&f, 2, &s).is_none());
    assert!(super::canon::signed_data(&f, usize::MAX, &s).is_none());
    assert!(super::canon::signed_data(&[], 0, &s).is_none());
    assert!(super::canon::signed_data(&f, 1, &s).is_some());
}

#[test]
fn whole_example_both_forms() {
    let raw = b"A: X\r\nB : Y\t\r\n\tZ  \r\n\r\n C \r\nD \t E\r\n\r\n\r\n";
    let f = header_fields(raw);
    let s = sig(&["a", "b"], RX);
    assert_eq!(
        signed_data(&f, b"DKIM-Signature: b=\r\n", &s),
        b"a:X\r\nb:Y Z\r\ndkim-signature:b="
    );
    assert_eq!(body_hash(&body(raw), RX, None), Some(sha(b" C\r\nD E\r\n")));
    assert_eq!(
        body_hash(&body(raw), SP, None),
        Some(sha(b" C \r\nD \t E\r\n"))
    );
}

proptest! {
    #[test]
    fn never_panics_on_bytes(
        raw in proptest::collection::vec(any::<u8>(), 0..4096),
        l in proptest::option::of(any::<u64>()),
    ) {
        let f = header_fields(&raw);
        let b = body(&raw);
        for c in [SP, RX] {
            let _ = body_hash(&b, c, l);
            let s = sig(&["from", "from", "received", "x"], c);
            let _ = signed_data(&f, &raw, &s);
            for x in &f {
                let _ = canon_field(x, c);
            }
        }
    }

    #[test]
    fn fields_are_slices_of_input(raw in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let lo = raw.as_ptr() as usize;
        let mut end = lo;
        for x in header_fields(&raw) {
            let p = x.as_ptr() as usize;
            prop_assert!(p >= end && p + x.len() <= lo + raw.len());
            end = p + x.len();
        }
    }

    #[test]
    fn relaxed_body_hash_idempotent(
        body_in in proptest::collection::vec(any::<u8>(), 0..1024),
        wsp in "[ \t]{0,5}",
        blanks in 0usize..4,
    ) {
        let base = body(&[b"\r\n".as_slice(), &body_in].concat()).into_owned();
        let mut more = base.clone();
        more.extend_from_slice(wsp.as_bytes());
        for _ in 0..blanks {
            more.extend_from_slice(b"\r\n");
        }
        prop_assert_eq!(body_hash(&base, RX, None), body_hash(&more, RX, None));
        let once = canon_body(&base, RX);
        prop_assert_eq!(canon_body(&once, RX), once);
    }

    #[test]
    fn simple_body_canon_idempotent(body_in in proptest::collection::vec(any::<u8>(), 0..1024)) {
        let base = body(&[b"\r\n".as_slice(), &body_in].concat()).into_owned();
        let once = canon_body(&base, SP);
        prop_assert_eq!(canon_body(&once, SP), once);
    }

    #[test]
    fn lf_and_crlf_agree(lines in proptest::collection::vec("[ -~\t]{0,20}", 0..10)) {
        let lf = format!("A: 1\n\n{}", lines.join("\n"));
        let crlf = format!("A: 1\r\n\r\n{}", lines.join("\r\n"));
        for c in [SP, RX] {
            prop_assert_eq!(
                body_hash(&body(lf.as_bytes()), c, None),
                body_hash(&body(crlf.as_bytes()), c, None)
            );
        }
    }

    #[test]
    fn l_matches_length_exactly(body_in in proptest::collection::vec(any::<u8>(), 0..256)) {
        let b = body(&[b"\r\n".as_slice(), &body_in].concat()).into_owned();
        for c in [SP, RX] {
            let n = canon_body(&b, c).len() as u64;
            prop_assert!(body_hash(&b, c, Some(n)).is_some());
            prop_assert_eq!(body_hash(&b, c, Some(n)), body_hash(&b, c, None));
            if n > 0 {
                prop_assert!(body_hash(&b, c, Some(n - 1)).is_none());
            }
            prop_assert!(body_hash(&b, c, Some(n + 1)).is_none());
        }
    }

    #[test]
    fn bounded_time_on_pathological(n in 1usize..20000, pick in 0u8..4) {
        let unit: &[u8] = match pick {
            0 => b" ",
            1 => b"\r\n",
            2 => b"a:\r\n ",
            _ => b"\n",
        };
        let raw = unit.repeat(n);
        let start = std::time::Instant::now();
        let f = header_fields(&raw);
        let b = body(&raw);
        let names = ["a"; 50];
        let s = sig(&names, RX);
        let _ = signed_data(&f, &raw, &s);
        let _ = body_hash(&b, RX, None);
        let _ = body_hash(&b, SP, None);
        prop_assert!(start.elapsed() < std::time::Duration::from_secs(5));
    }
}
