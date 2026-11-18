//! Mail from a server can carry `X-Envio-*` headers of its own; envio-net
//! renames them (`neutralize`) so envio-mime never reads them as the stubs
//! envio writes itself. Runs the renamed bytes through the real parser; no
//! server needed.

use envio_mime::Message;
use envio_net::imap::untrusted::neutralize;

/// A message with a text part and an attachment, each carrying headers
/// `<prefix>-truncated`, `<prefix>-section` and `<prefix>-size`; `prefix`
/// is what comes before the `-truncated` etc.
fn mail(prefix: &[u8]) -> Vec<u8> {
    let mut raw = b"From: a@b.test\r\nTo: c@d.test\r\nSubject: s\r\nMIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"b\"\r\n\r\n--b\r\n\
Content-Type: text/plain; charset=utf-8\r\n"
        .to_vec();
    raw.extend_from_slice(prefix);
    raw.extend_from_slice(b"-truncated: 1\r\n\r\nhello\r\n--b\r\n");
    raw.extend_from_slice(
        b"Content-Type: application/pdf\r\nContent-Disposition: attachment; filename=\"f.pdf\"\r\n",
    );
    raw.extend_from_slice(prefix);
    raw.extend_from_slice(b"-section: 1.2\r\n");
    raw.extend_from_slice(prefix);
    raw.extend_from_slice(b"-size: 9\r\n\r\nAAAA\r\n--b--\r\n");
    raw
}

/// (an attachment is a stub, the text is marked cut short) for `raw` as
/// the reader sees it.
fn parsed(raw: &[u8]) -> (bool, bool) {
    match envio_mime::parse(raw) {
        Message::Parsed {
            attachments,
            truncated,
            ..
        } => (attachments.iter().any(|a| a.section.is_some()), truncated),
        Message::Raw(_) => panic!("parsed as raw"),
    }
}

fn seen(raw: &[u8]) -> (bool, bool) {
    parsed(&neutralize(raw))
}

#[test]
fn the_parser_does_read_a_plain_envio_header() {
    // The control: without neutralising, the marks count.
    assert_eq!(parsed(&mail(b"X-Envio")), (true, true));
    assert_eq!(seen(&mail(b"X-Envio")), (false, false));
}

#[test]
fn leading_whitespace_and_control_bytes_cannot_smuggle_our_headers() {
    for lead in [
        &b"\r"[..],
        b"\x0c",
        b"\x0b",
        b"\t",
        b" ",
        b"\r ",
        b"\x0c\x0c",
        b" \t\x0c\x0b\r",
        b"\n",
    ] {
        for name in ["X-Envio", "x-envio", "X-ENVIO"] {
            let mut n = lead.to_vec();
            n.extend_from_slice(name.as_bytes());
            assert_eq!(seen(&mail(&n)), (false, false), "{lead:?} {name}");
        }
    }
}

/// Names with whitespace inside. neutralize leaves these alone; they are
/// safe only because envio-mime compares the header name exactly. If the
/// parser or `stub_header` ever normalised names, this fails.
#[test]
fn names_with_inner_whitespace_are_not_our_headers() {
    for prefix in [
        &b"X-Envio "[..],
        b"X- Envio",
        b"X-Envio\t",
        b"X-Envio\r",
        b"X-Envio\x0b",
    ] {
        assert_eq!(seen(&mail(prefix)), (false, false), "{prefix:?}");
        // Untouched by neutralize or not, the parser must not take them.
        assert_eq!(parsed(&mail(prefix)), (false, false), "raw {prefix:?}");
    }
}
