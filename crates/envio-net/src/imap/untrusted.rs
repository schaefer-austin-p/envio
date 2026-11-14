//! Mail from a server is untrusted, and envio writes its own `X-Envio-*`
//! headers into a big message (the stub marks, see `partial::assemble`). A
//! header of that name that came with the mail must not pass for ours: a
//! small message from a sender could otherwise claim its attachment lives on
//! the server and save as an empty file.

use std::borrow::Cow;

const PREFIX: &[u8] = b"x-envio-";
const NEUTRAL: &[u8] = b"X-Untrusted-";

fn is_ours(line: &[u8]) -> bool {
    line.len() >= PREFIX.len() && line[..PREFIX.len()].eq_ignore_ascii_case(PREFIX)
}

/// `raw` with every line that begins `X-Envio-` renamed `X-Untrusted-Envio-`.
/// Borrowed when nothing needed renaming.
pub(super) fn neutralize(raw: &[u8]) -> Cow<'_, [u8]> {
    if !raw.split(|b| *b == b'\n').any(is_ours) {
        return Cow::Borrowed(raw);
    }
    let mut out = Vec::with_capacity(raw.len() + 64);
    for line in raw.split_inclusive(|b| *b == b'\n') {
        if is_ours(line) {
            out.extend_from_slice(NEUTRAL);
            out.extend_from_slice(&line[2..]);
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
}
