//! `mailto:` links (RFC 6068) into a compose draft.

use crate::address::Address;
use crate::thread::normalize_id;

/// What a `mailto:` link fills in. Other headers in the link are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MailtoDraft {
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub subject: String,
    /// `\n` line ends.
    pub body: String,
    /// Without angle brackets.
    pub in_reply_to: Option<String>,
}

/// Parses a `mailto:` URI; `None` when it isn't one.
pub fn parse_mailto(uri: &str) -> Option<MailtoDraft> {
    let uri = uri.trim();
    let scheme = uri.get(..7)?;
    if !scheme.eq_ignore_ascii_case("mailto:") {
        return None;
    }
    let rest = &uri[7..];
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let mut draft = MailtoDraft {
        to: addresses(&decode(path)),
        ..MailtoDraft::default()
    };
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = decode(value);
        match decode(name).to_ascii_lowercase().as_str() {
            "to" => draft.to.extend(addresses(&value)),
            "cc" => draft.cc.extend(addresses(&value)),
            "bcc" => draft.bcc.extend(addresses(&value)),
            "subject" => draft.subject = value.chars().filter(|c| !c.is_control()).collect(),
            "body" => {
                draft.body = value
                    .replace("\r\n", "\n")
                    .chars()
                    .filter(|&c| c == '\n' || c == '\t' || !c.is_control())
                    .collect();
            }
            "in-reply-to" => draft.in_reply_to = normalize_id(&value),
            _ => {}
        }
    }
    Some(draft)
}

/// Comma-separated addresses; entries that fail `Address::parse` (no `@`,
/// control characters) are skipped.
fn addresses(list: &str) -> Vec<Address> {
    list.split(',').filter_map(Address::parse).collect()
}

/// Percent-decoding as RFC 3986 (`+` stays `+`), lossy UTF-8. A `%` not
/// followed by two hex digits is kept as is.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes.get(i + 1..i + 3).and_then(|h| {
            let h = std::str::from_utf8(h).ok()?;
            h.bytes()
                .all(|b| b.is_ascii_hexdigit())
                .then(|| u8::from_str_radix(h, 16).ok())
                .flatten()
        });
        match (bytes[i], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
