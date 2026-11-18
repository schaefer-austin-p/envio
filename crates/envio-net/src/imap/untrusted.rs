//! Mail from a server is untrusted, and envio writes its own `X-Envio-*`
//! headers into a big message (the stub marks, see `partial::assemble`). A
//! header of that name that came with the mail must not pass for ours: a
//! small message from a sender could otherwise claim its attachment lives on
//! the server and save as an empty file.

use std::borrow::Cow;

const PREFIX: &[u8] = b"x-envio-";
const NEUTRAL: &[u8] = b"X-Untrusted-";

/// Bytes a header parser may skip before a name: ASCII whitespace plus VT.
fn is_skipped(b: &u8) -> bool {
    b.is_ascii_whitespace() || *b == 0x0b
}

fn is_line_end(b: &u8) -> bool {
    matches!(b, b'\r' | b'\n')
}

/// The number of leading bytes of `line` a parser may skip.
fn lead_len(line: &[u8]) -> usize {
    line.iter().take_while(|b| is_skipped(b)).count()
}

fn is_ours(line: &[u8]) -> bool {
    let rest = &line[lead_len(line)..];
    rest.len() >= PREFIX.len() && rest[..PREFIX.len()].eq_ignore_ascii_case(PREFIX)
}

/// `raw` with every line that begins `X-Envio-` (after any whitespace or
/// control bytes a header parser skips; CR, LF and CR LF all end a line) renamed `X-Untrusted-Envio-`.
/// Borrowed when nothing needed renaming.
pub(super) fn neutralize(raw: &[u8]) -> Cow<'_, [u8]> {
    if !raw.split(is_line_end).any(is_ours) {
        return Cow::Borrowed(raw);
    }
    let mut out = Vec::with_capacity(raw.len() + 64);
    for line in raw.split_inclusive(is_line_end) {
        if is_ours(line) {
            let lead = lead_len(line);
            out.extend_from_slice(&line[..lead]);
            out.extend_from_slice(NEUTRAL);
            out.extend_from_slice(&line[lead + 2..]);
        } else {
            out.extend_from_slice(line);
        }
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn our_header_names_are_renamed_in_any_case_and_nothing_else() {
        let raw = b"A: b\r\nX-Envio-Section: 2\r\nx-envio-size: 9\r\n\r\nX-Envio-Section: in the body\r\nbody\r\n";
        let out = neutralize(raw);
        assert_eq!(
            out.as_ref(),
            b"A: b\r\nX-Untrusted-Envio-Section: 2\r\nX-Untrusted-envio-size: 9\r\n\r\n\
              X-Untrusted-Envio-Section: in the body\r\nbody\r\n"
        );
    }

    #[test]
    fn mail_without_them_is_not_copied() {
        let raw = b"A: b\r\nX-Other: 1\r\n\r\nbody\r\n";
        assert!(matches!(neutralize(raw), Cow::Borrowed(_)));
    }

    fn renamed(raw: &[u8]) -> Vec<u8> {
        neutralize(raw).into_owned()
    }

    #[test]
    fn leading_whitespace_or_control_bytes_do_not_hide_our_names() {
        // Bytes mail-parser skips before a header name, and bytes that can
        // end a line or start a fold.
        for lead in [
            &b"\r"[..],
            b"\x0c",
            b"\x0b",
            b"\t",
            b" ",
            b"\r\x0c",
            b" \t\x0c\x0b\r",
            b"\n",
        ] {
            for name in ["X-Envio-Section", "x-envio-truncated", "X-ENVIO-Size"] {
                let mut raw = b"A: b\r\n".to_vec();
                raw.extend_from_slice(lead);
                raw.extend_from_slice(name.as_bytes());
                raw.extend_from_slice(b": 1\r\n\r\nbody\r\n");
                let out = renamed(&raw);
                // Nothing may be left that starts a line, after skipped
                // whitespace, with the prefix.
                for l in out.split(|b| matches!(b, b'\r' | b'\n')) {
                    let t = l.trim_ascii_start();
                    assert!(
                        !(t.len() >= PREFIX.len()
                            && t[..PREFIX.len()].eq_ignore_ascii_case(PREFIX)),
                        "{lead:?} {name}: {out:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_leading_cr_or_ff_is_renamed_keeping_the_lead() {
        assert_eq!(
            renamed(b"A: b\n\rX-Envio-Section: 1.2\n\n"),
            b"A: b\n\rX-Untrusted-Envio-Section: 1.2\n\n"
        );
        assert_eq!(
            renamed(b"A: b\n\x0cX-Envio-Truncated: 1\n\n"),
            b"A: b\n\x0cX-Untrusted-Envio-Truncated: 1\n\n"
        );
    }

    #[test]
    fn a_bare_cr_starts_a_line() {
        assert_eq!(
            renamed(b"A: b\rX-Envio-Section: 1\r\r"),
            b"A: b\rX-Untrusted-Envio-Section: 1\r\r"
        );
    }

    #[test]
    fn near_misses_and_normal_headers_are_untouched() {
        for raw in [
            &b"A: b\r\nX-Envi: 1\r\nX-Envio: 1\r\nXX-Envio-A: 1\r\n\r\n"[..],
            b"A: x-envio-inside value\r\nSubject: hi\r\n\r\n",
            b" \x0c",
            b"",
        ] {
            assert!(matches!(neutralize(raw), Cow::Borrowed(_)), "{raw:?}");
        }
    }
}
