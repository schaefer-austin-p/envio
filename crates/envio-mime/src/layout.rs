//! What a message has that a rebuilt draft would not: its top-level
//! headers, its Bcc and whether its parts are the shape Envio writes
//! (one text/plain, then attachments). For Edit's import of a server
//! draft (#228): the caller decides what loses nothing.

use mail_parser::{MimeHeaders, PartType};

use crate::address::{Address, from_header};
use crate::mail::{Parsed, parse_mail};

/// See the module doc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// Names of the top-level headers, lower case, each once, in order.
    pub headers: Vec<String>,
    /// The Bcc header's mailboxes (the reader's envelope has none).
    pub bcc: Vec<Address>,
    /// The body is one `text/plain` part, alone or first in a
    /// `multipart/mixed` whose other parts are all leaf attachments with a
    /// name.
    pub plain_shape: bool,
}

/// `None` when the bytes are not mail the reader can parse.
pub fn layout(raw: &[u8]) -> Option<Layout> {
    let message = parse_mail(raw)?;
    let mut headers: Vec<String> = Vec::new();
    for name in header_names(raw) {
        if !headers.contains(&name) {
            headers.push(name);
        }
    }
    Some(Layout {
        headers,
        bcc: from_header(message.bcc()),
        plain_shape: plain_shape(&message),
    })
}

/// The names before the colons of the header block; folded lines belong to
/// the header above them.
fn header_names(raw: &[u8]) -> Vec<String> {
    raw.split(|b| *b == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .take_while(|line| !line.is_empty())
        .filter(|line| !matches!(line[0], b' ' | b'\t'))
        .map(|line| {
            let end = line.iter().position(|b| *b == b':').unwrap_or(line.len());
            String::from_utf8_lossy(&line[..end])
                .trim()
                .to_ascii_lowercase()
        })
        .collect()
}

fn plain_shape(message: &Parsed<'_>) -> bool {
    let root = message.root_part();
    match &root.body {
        PartType::Text(_) => is_plain(message, 0),
        PartType::Multipart(kids) => {
            let mixed = root.content_type().is_some_and(|ct| {
                ct.subtype()
                    .is_some_and(|s| s.eq_ignore_ascii_case("mixed"))
            });
            match kids.split_first() {
                Some((first, rest)) => {
                    mixed
                        && is_plain(message, *first)
                        && rest.iter().all(|kid| is_attachment(message, *kid))
                }
                None => false,
            }
        }
        _ => false,
    }
}

/// A text part that is the message's text: text/plain (or untyped) and not
/// a file.
fn is_plain(message: &Parsed<'_>, index: u32) -> bool {
    let Some(part) = message.parts.get(index as usize) else {
        return false;
    };
    let typed_plain = part.content_type().is_none_or(|ct| {
        ct.ctype().eq_ignore_ascii_case("text")
            && ct.subtype().is_none_or(|s| s.eq_ignore_ascii_case("plain"))
    });
    matches!(part.body, PartType::Text(_))
        && typed_plain
        && part.content_disposition().is_none()
        && part.attachment_name().is_none()
}

/// A leaf part sent as a named file.
fn is_attachment(message: &Parsed<'_>, index: u32) -> bool {
    let Some(part) = message.parts.get(index as usize) else {
        return false;
    };
    !matches!(part.body, PartType::Multipart(_))
        && part.attachment_name().is_some()
        && part
            .content_disposition()
            .is_some_and(mail_parser::ContentType::is_attachment)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "From: me@example.org\r\nTo: a@example.org\r\nSubject: s\r\n";

    fn mixed(parts: &str) -> Vec<u8> {
        format!(
            "{HEAD}MIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=B\r\n\r\n{parts}--B--\r\n"
        )
        .into_bytes()
    }

    const TEXT: &str = "--B\r\nContent-Type: text/plain\r\n\r\nt\r\n";
    const FILE: &str = "--B\r\nContent-Type: application/pdf; name=\"a.pdf\"\r\n\
Content-Disposition: attachment; filename=\"a.pdf\"\r\nContent-Transfer-Encoding: base64\r\n\r\nSGk=\r\n";

    fn shaped(raw: &[u8]) -> bool {
        layout(raw).unwrap().plain_shape
    }

    #[test]
    fn names_each_top_level_header_once_unfolded() {
        let raw = b"From: me@example.org\r\nSubject: a\r\n b\r\nX-Priority: 1\r\nx-priority: 2\r\n\r\nt\r\n";
        let l = layout(raw).unwrap();
        assert_eq!(l.headers, vec!["from", "subject", "x-priority"]);
    }

    #[test]
    fn reads_the_bcc_header() {
        let raw = format!("{HEAD}Bcc: Hid <h@example.org>\r\n\r\nt\r\n");
        let l = layout(raw.as_bytes()).unwrap();
        assert_eq!(l.bcc, vec![Address::new(Some("Hid"), "h@example.org")]);
    }

    #[test]
    fn text_alone_or_text_then_files_is_the_plain_shape() {
        assert!(shaped(format!("{HEAD}\r\nt\r\n").as_bytes()));
        assert!(shaped(&mixed(&format!("{TEXT}{FILE}"))));
        assert!(shaped(&mixed(TEXT)));
    }

    #[test]
    fn other_shapes_are_not() {
        let html = format!("{HEAD}Content-Type: text/html\r\n\r\n<p>t</p>\r\n");
        assert!(!shaped(html.as_bytes()));
        let two = format!("{TEXT}--B\r\nContent-Type: text/plain\r\n\r\nsecond\r\n{FILE}");
        assert!(!shaped(&mixed(&two)));
        let nameless =
            "--B\r\nContent-Type: text/calendar; method=REQUEST\r\n\r\nBEGIN:VCALENDAR\r\n";
        assert!(!shaped(&mixed(&format!("{TEXT}{nameless}"))));
        let alt = format!(
            "{HEAD}Content-Type: multipart/alternative; boundary=B\r\n\r\n{TEXT}--B\r\nContent-Type: text/html\r\n\r\n<p>t</p>\r\n--B--\r\n"
        );
        assert!(!shaped(alt.as_bytes()));
        let inline = "--B\r\nContent-Type: image/png; name=\"i.png\"\r\nContent-Disposition: inline; filename=\"i.png\"\r\nContent-ID: <i>\r\n\r\nSGk=\r\n";
        assert!(!shaped(&mixed(&format!("{TEXT}{inline}"))));
    }
}
