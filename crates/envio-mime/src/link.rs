//! Link targets: making them safe to show and splitting them so the reader
//! can highlight the real host.

use std::borrow::Cow;

/// Unicode format (Cf) and bidi control characters: invisible, and able to
/// reorder or hide what follows them when a link is shown. The list is the
/// Cf set of Unicode 16.
pub(crate) fn is_format_char(c: char) -> bool {
    matches!(c,
        '\u{00AD}' | '\u{0600}'..='\u{0605}' | '\u{061C}' | '\u{06DD}' | '\u{070F}'
        | '\u{0890}'..='\u{0891}' | '\u{08E2}' | '\u{180E}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{206F}' | '\u{FEFF}'
        | '\u{FFF9}'..='\u{FFFB}' | '\u{110BD}' | '\u{110CD}' | '\u{13430}'..='\u{1343F}' | '\u{1BCA0}'..='\u{1BCA3}'
        | '\u{1D173}'..='\u{1D17A}'
        | '\u{E0001}' | '\u{E0020}'..='\u{E007F}')
}

/// Where the authority (`//user@host:port`) of `url` ends, as a byte offset;
/// 0 when it has none (`mailto:`), so the whole rest counts as path.
fn authority_end(url: &str) -> usize {
    let Some(colon) = url.find(':') else { return 0 };
    let after = colon + 1;
    if !url[after..].starts_with("//") {
        return after;
    }
    let start = after + 2;
    url[start..]
        .find(['/', '?', '#'])
        .map_or(url.len(), |i| start + i)
}

/// The href made safe at the source: format characters in the path, query
/// or fragment are percent-encoded (the link keeps working and shows the
/// truth); in the scheme or authority they could make one host look like
/// another, so the link is dropped (`None`).
pub(crate) fn safe_href(href: &str) -> Option<Cow<'_, str>> {
    let split = authority_end(href);
    // Browsers read `\` in an http(s) authority as `/`, so the host they open
    // differs from the one the text suggests; it is never legitimate.
    if href[..split].contains('\\') {
        return None;
    }
    if !href.chars().any(is_format_char) {
        return Some(Cow::Borrowed(href));
    }
    if href[..split].chars().any(is_format_char) {
        return None;
    }
    let mut out = String::with_capacity(href.len() + 12);
    out.push_str(&href[..split]);
    for c in href[split..].chars() {
        if is_format_char(c) {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        } else {
            out.push(c);
        }
    }
    Some(Cow::Owned(out))
}

/// A link split for display; the host is what the reader highlights.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkParts {
    /// Lowercase, without `:`.
    pub scheme: String,
    /// What comes before `@` in the authority (`https://bank.example@evil.example/`
    /// has `bank.example`); for `mailto:`, the local part.
    pub userinfo: Option<String>,
    /// The host as people read it: an IDN is shown as Unicode.
    pub host: String,
    /// The ASCII (punycode) form, when it differs from `host`.
    pub punycode: Option<String>,
    /// Without `:`.
    pub port: Option<String>,
    /// Path, query and fragment as given (format characters included; the
    /// reader shows them as `[U+XXXX]`). For `mailto:`, whatever follows the
    /// first address.
    pub rest: String,
}

/// Splits an http, https or mailto link; `None` for other schemes, a missing
/// or invalid host, or a format character or whitespace in the host.
pub fn link_parts(url: &str) -> Option<LinkParts> {
    let url = url.trim();
    let (scheme, after) = url.split_once(':')?;
    let scheme = scheme.to_ascii_lowercase();
    let (userinfo, hostport, rest) = match scheme.as_str() {
        "http" | "https" => {
            let after = after.strip_prefix("//")?;
            let end = after.find(['/', '?', '#']).unwrap_or(after.len());
            let (authority, rest) = after.split_at(end);
            if authority.contains('\\') || authority.chars().any(bad_host_char) {
                return None;
            }
            let (userinfo, hostport) = match authority.rsplit_once('@') {
                Some((u, h)) => (Some(u.to_owned()), h),
                None => (None, authority),
            };
            (userinfo, hostport, rest)
        }
        "mailto" => {
            let end = after.find([',', '?']).unwrap_or(after.len());
            let (address, rest) = after.split_at(end);
            let (local, domain) = address.rsplit_once('@')?;
            (Some(local.to_owned()), domain, rest)
        }
        _ => return None,
    };
    let (host, port) = split_port(hostport);
    if port.is_some_and(|p| !p.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    // Browsers percent-decode the host before IDNA, so show that one.
    let host = percent_decode(host)?;
    if host.is_empty() || host.chars().any(bad_host_char) {
        return None;
    }
    let (host, punycode) = host_forms(&host)?;
    Some(LinkParts {
        scheme,
        userinfo,
        host,
        punycode,
        port: port.map(str::to_owned),
        rest: rest.to_owned(),
    })
}

fn bad_host_char(c: char) -> bool {
    is_format_char(c) || c.is_control() || c.is_whitespace()
}

/// `None` when the result isn't UTF-8.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(b)) => {
                out.push(b);
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

fn split_port(hostport: &str) -> (&str, Option<&str>) {
    if hostport.starts_with('[') {
        // IPv6 literal: the port follows the closing bracket.
        return match hostport.find(']') {
            Some(i) => (
                &hostport[..=i],
                hostport[i + 1..]
                    .strip_prefix(':')
                    .filter(|p| !p.is_empty()),
            ),
            None => (hostport, None),
        };
    }
    match hostport.rsplit_once(':') {
        Some((h, p)) => (h, Some(p).filter(|p| !p.is_empty())),
        None => (hostport, None),
    }
}

/// (Unicode form, punycode form when different).
fn host_forms(host: &str) -> Option<(String, Option<String>)> {
    if host.starts_with('[') {
        return Some((host.to_owned(), None));
    }
    let ascii = idna::domain_to_ascii(host).ok()?;
    let (unicode, result) = idna::domain_to_unicode(&ascii);
    result.ok()?;
    let punycode = (unicode != ascii).then_some(ascii);
    Some((unicode, punycode))
}
