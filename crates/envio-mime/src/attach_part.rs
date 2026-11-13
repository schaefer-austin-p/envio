//! An outgoing attachment as a MIME part, always base64.
//!
//! mail-builder picks 7bit or quoted-printable for `text/*` parts by
//! scanning the bytes, and a bare CR or NUL then goes out raw (a way to
//! smuggle SMTP commands past the relay). So every attachment is encoded
//! here and tagged `base64`, whatever its type; mail-builder writes such a
//! part as given.

use mail_builder::headers::content_type::ContentType;
use mail_builder::mime::{BodyPart, MimePart};

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 in lines of 76 characters joined by CRLF, no trailing CRLF.
fn base64_lines(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() / 3 * 4 + bytes.len() / 57 * 2 + 4);
    let mut column = 0;
    for chunk in bytes.chunks(3) {
        if column == 76 {
            out.extend_from_slice(b"\r\n");
            column = 0;
        }
        let mut n = 0u32;
        for (i, b) in chunk.iter().enumerate() {
            n |= u32::from(*b) << (16 - 8 * i);
        }
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize]);
            } else {
                out.push(b'=');
            }
        }
        column += 4;
    }
    out
}

/// `content_type` gets `charset=utf-8` when it is `text/*` and `bytes` are
/// valid UTF-8.
pub(crate) fn part<'a>(content_type: &'a str, filename: &'a str, bytes: &[u8]) -> MimePart<'a> {
    let mut ct = ContentType::new(content_type);
    if content_type.starts_with("text/") && std::str::from_utf8(bytes).is_ok() {
        ct = ct.attribute("charset", "utf-8");
    }
    MimePart::new(ct, BodyPart::Binary(base64_lines(bytes).into()))
        .attachment(filename)
        .transfer_encoding("base64")
}

#[cfg(test)]
mod tests {
    use super::base64_lines;

    #[test]
    fn rfc4648_vectors() {
        for (plain, coded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64_lines(plain.as_bytes()), coded.as_bytes(), "{plain}");
        }
    }
}
