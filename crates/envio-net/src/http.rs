//! Just enough HTTPS for the small fetches envio-net makes: a request over
//! HTTP/1.0 with no cookies and no User-Agent, and how a reply is framed and
//! read. This file is the pure part: URLs, the request, reply framing, and
//! which redirect hosts autoconfig allows. The network side is in
//! `autoconfig::fetch`.

use std::io;
use std::net::IpAddr;

use tokio::io::{AsyncRead, AsyncReadExt};
use zeroize::Zeroizing;

mod chunked;
use chunked::{Chunked, ResponseScan, dechunk};
pub(crate) use chunked::{SizeError, chunk_size};

/// Autoconfig files are a few KiB; a body over this is not one.
pub const MAX_BODY: usize = 256 * 1024;
/// The status line and headers, which are not counted in `MAX_BODY`.
pub const MAX_HEAD: usize = 16 * 1024;
/// The most `read_reply` and `read_response` hold: a reply is read until the
/// buffer passes this.
const MAX_RAW: usize = MAX_HEAD + MAX_BODY;
pub const MAX_REDIRECTS: usize = 3;

#[derive(Debug, PartialEq, Eq)]
pub struct Url {
    pub host: String,
    pub port: u16,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    Body(String),
    Redirect(String),
    /// A Transfer-Encoding of any kind (chunked, gzip, ...): not a framing
    /// autoconfig reads.
    TransferEncoded,
    /// Any other status, or a body that is too big or cut short: there is
    /// no file here.
    Nothing,
}

/// Letters, digits and '-' in labels separated by single dots. A label
/// may not be empty or start or end with '-'.
pub fn is_hostname(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// `https://host[:port]/path`; anything else (http, user info) is None.
pub fn parse_url(url: &str) -> Option<Url> {
    let rest = url.strip_prefix("https://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, port.parse().ok()?),
        None => (authority, 443),
    };
    let ok_path = path.bytes().all(|b| b.is_ascii_graphic());
    (is_hostname(host) && ok_path).then(|| Url {
        host: host.to_ascii_lowercase(),
        port,
        path: path.to_owned(),
    })
}

/// The request. The Host header names the port unless it is the default.
pub fn request(url: &Url) -> String {
    let host = if url.port == 443 {
        url.host.clone()
    } else {
        format!("{}:{}", url.host, url.port)
    };
    format!(
        "GET {} HTTP/1.0\r\nHost: {host}\r\nAccept: application/xml, text/xml\r\nConnection: close\r\n\r\n",
        url.path
    )
}

/// Whether a redirect may point at `host`. This is a check on the name
/// only: it refuses `localhost`, IP literals and the legacy IPv4 spellings
/// a resolver reads as addresses (an all-digit or `0x` last label, e.g.
/// `2130706433`, `127.1`, `0x7f.0x1`). It does not stop a public name that
/// resolves to a private or loopback address (DNS rebinding, or a record
/// the owner pointed at their LAN), and it allows any port. Autoconfig
/// hosts also need a certificate the system trusts, which limits what a
/// redirect can reach but is not a network boundary.
pub fn allowed_redirect_host(host: &str) -> bool {
    let host = host.strip_suffix('.').unwrap_or(host).to_ascii_lowercase();
    let local = host == "localhost" || host.ends_with(".localhost");
    let literal = host.parse::<IpAddr>().is_ok()
        || host.rsplit('.').next().is_some_and(|last| {
            last.starts_with("0x") || (!last.is_empty() && last.bytes().all(|b| b.is_ascii_digit()))
        });
    !local && !literal
}

/// The Content-Length of a reply, once every such header is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Length {
    /// No Content-Length header: the body runs to EOF or the cap.
    None,
    Known(usize),
    /// Missing digits, not a number, or repeated with different values.
    Invalid,
}

/// The status, the headers autoconfig uses, and where the body starts.
struct Head {
    status: u16,
    location: Option<String>,
    length: Length,
    chunked: bool,
    /// A Transfer-Encoding with a coding other than a final `chunked`
    /// (gzip, a repeated `chunked`): not framing this client can read.
    bad_encoding: bool,
    body_start: usize,
}

/// A Content-Length value: ASCII digits only, and within `usize`.
fn digits_length(value: &str) -> Option<usize> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

fn head(raw: &[u8]) -> Option<Head> {
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let text = String::from_utf8_lossy(&raw[..split]);
    let mut lines = text.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    let mut head = Head {
        status,
        location: None,
        length: Length::None,
        chunked: false,
        bad_encoding: false,
        body_start: split + 4,
    };
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let (name, value) = (name.trim(), value.trim());
        if name.eq_ignore_ascii_case("location") {
            head.location.get_or_insert_with(|| value.to_owned());
        } else if name.eq_ignore_ascii_case("content-length") {
            // Repeats are fine only when they agree (RFC 9112 6.3).
            head.length = match (head.length, digits_length(value)) {
                (Length::Invalid, _) | (_, None) => Length::Invalid,
                (Length::None, Some(n)) => Length::Known(n),
                (Length::Known(m), Some(n)) if m == n => Length::Known(m),
                (Length::Known(_), Some(_)) => Length::Invalid,
            };
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            // Only a lone `chunked` is framing this client reads (any
            // other coding, or a second `chunked`, is refused).
            let only_chunked = value.eq_ignore_ascii_case("chunked");
            head.bad_encoding |= !only_chunked || head.chunked;
            head.chunked |= only_chunked;
        }
    }
    Some(head)
}

/// Whether `raw` already holds the whole reply, or nothing more can change
/// the answer: a chunked head, a bad or over-cap Content-Length (both refused),
/// or a body as long as its Content-Length. A reply with no length is complete
/// only at EOF or at the cap.
pub fn reply_complete(raw: &[u8]) -> bool {
    let Some(head) = head(raw) else {
        return false;
    };
    head.chunked || head.bad_encoding || length_complete(&head, raw.len())
}

/// Whether `raw_len` bytes hold the whole body by `head`'s Content-Length,
/// or the length is bad or over the cap (both refused). No length is never
/// complete here: the body runs to the close.
fn length_complete(head: &Head, raw_len: usize) -> bool {
    match head.length {
        Length::None => false,
        Length::Invalid => true,
        Length::Known(n) => {
            n > MAX_BODY
                || head
                    .body_start
                    .checked_add(n)
                    .is_some_and(|end| raw_len >= end)
        }
    }
}

/// Why a reply has no usable body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadReply {
    /// A body over `MAX_BODY`, declared or read: the server is sending
    /// something that is not what was asked for.
    TooBig,
    /// Cut short, a bad length, or not HTTP: a bad connection.
    Unreadable,
    /// Broken chunk framing, an unsupported Transfer-Encoding, or a chunked
    /// body the server ended without its last chunk: the server's fault, and
    /// a retry gets the same.
    Malformed,
}

/// The body of a reply, framed by its Content-Length or the cap.
fn body_of<'a>(head: &Head, raw: &'a [u8]) -> Result<&'a [u8], BadReply> {
    let rest = &raw[head.body_start..];
    match head.length {
        Length::Invalid => Err(BadReply::Unreadable),
        Length::Known(n) if n > MAX_BODY => Err(BadReply::TooBig),
        Length::Known(n) if rest.len() < n => Err(BadReply::Unreadable),
        Length::Known(n) => Ok(&rest[..n]),
        Length::None if rest.len() > MAX_BODY => Err(BadReply::TooBig),
        Length::None => Ok(rest),
    }
}

/// The reply in `raw` (the bytes read so far, up to its end).
pub fn parse_reply(raw: &[u8]) -> Reply {
    let Some(head) = head(raw) else {
        return Reply::Nothing;
    };
    match head.status {
        200 if head.chunked || head.bad_encoding => Reply::TransferEncoded,
        200 => body_of(&head, raw)
            .ok()
            .and_then(|body| String::from_utf8(body.to_vec()).ok())
            .map_or(Reply::Nothing, Reply::Body),
        301 | 302 | 303 | 307 | 308 => head.location.map_or(Reply::Nothing, Reply::Redirect),
        _ => Reply::Nothing,
    }
}

/// The status and body of a reply with any status (a token endpoint answers
/// errors with a JSON body). The body cap is the same as `parse_reply`'s.
/// `clean` is as for [`finish_reply`].
pub fn parse_response(raw: &[u8], clean: bool) -> Result<(u16, Vec<u8>), BadReply> {
    let head = head(raw).ok_or(BadReply::Unreadable)?;
    if head.status == 0 {
        return Err(BadReply::Unreadable);
    }
    // Bad framing is final, except in a reply that is already "try later"
    // (429, 5xx): a broken gateway page is retried, as it was before chunked
    // replies were read.
    let bad_framing = if head.status == 429 || head.status >= 500 {
        BadReply::Unreadable
    } else {
        BadReply::Malformed
    };
    if head.bad_encoding {
        return Err(bad_framing);
    }
    if head.chunked {
        let rest = &raw[head.body_start..];
        return match dechunk(rest) {
            Chunked::Done(ranges, total) => {
                // One allocation of the exact size: growing would leave an
                // unzeroed copy of a token behind.
                let mut body = Vec::with_capacity(total);
                for r in ranges {
                    body.extend_from_slice(&rest[r]);
                }
                Ok((head.status, body))
            }
            Chunked::TooBig => Err(BadReply::TooBig),
            Chunked::Malformed => Err(bad_framing),
            // The read stopped at its cap before the last chunk: the chunk
            // framing counts toward it, but the reply is too big all the same.
            Chunked::Incomplete if raw.len() > MAX_RAW => Err(BadReply::TooBig),
            // Ended without its last chunk: a dropped socket may be worth
            // a retry, a clean close is the server's own framing fault.
            Chunked::Incomplete if clean => Err(bad_framing),
            Chunked::Incomplete => Err(BadReply::Unreadable),
        };
    }
    // An over-cap body is reported even if the stream was cut: the size
    // is what is wrong with it.
    match body_of(&head, raw) {
        Err(BadReply::TooBig) => Err(BadReply::TooBig),
        _ if !(clean || reply_complete(raw)) => Err(BadReply::Unreadable),
        body => body.map(|body| (head.status, body.to_vec())),
    }
}

/// [`parse_reply`], for bytes read until the stream ended. `clean` is
/// whether it ended with a close_notify (or the cap); if not, the reply
/// stands only when it was already complete by its own framing. A body with
/// no Content-Length would otherwise be accepted cut short.
pub fn finish_reply(raw: &[u8], clean: bool) -> Reply {
    if clean || reply_complete(raw) {
        parse_reply(raw)
    } else {
        Reply::Nothing
    }
}

const CHUNK: usize = 8192;
/// The most `read_reply` can hold: it stops once the buffer passes
/// `MAX_HEAD + MAX_BODY`, and the last read adds at most one chunk.
const READ_CAPACITY: usize = MAX_HEAD + MAX_BODY + CHUNK;

/// Reads until the reply is complete, the peer closes, or the reply is
/// over the cap. The flag is false when the peer dropped TCP without
/// close_notify (rustls reports that as `UnexpectedEof`): what was read may
/// be cut short.
///
/// The reply may hold tokens, so the buffer is zeroed on drop (including on
/// an early error) and is allocated once at its largest size: growing it
/// would free the old copy without zeroing it.
pub(crate) async fn read_reply<R: AsyncRead + Unpin>(
    stream: &mut R,
) -> io::Result<(Zeroizing<Vec<u8>>, bool)> {
    read_until(stream, reply_complete).await
}

/// [`read_reply`] for a reader of [`parse_response`]: it reads a chunked
/// reply through its last chunk.
pub(crate) async fn read_response<R: AsyncRead + Unpin>(
    stream: &mut R,
) -> io::Result<(Zeroizing<Vec<u8>>, bool)> {
    let mut scan = ResponseScan::default();
    read_until(stream, |raw| scan.complete(raw)).await
}

async fn read_until<R: AsyncRead + Unpin>(
    stream: &mut R,
    mut complete: impl FnMut(&[u8]) -> bool,
) -> io::Result<(Zeroizing<Vec<u8>>, bool)> {
    let mut raw = Zeroizing::new(Vec::with_capacity(READ_CAPACITY));
    let mut chunk = Zeroizing::new([0u8; CHUNK]);
    while !complete(&raw) && raw.len() <= MAX_RAW {
        match stream.read(&mut chunk[..]).await {
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok((raw, false)),
            Err(e) => return Err(e),
        }
    }
    Ok((raw, true))
}

#[cfg(test)]
use chunked::{MAX_CHUNK_LINE, response_complete};

#[cfg(test)]
mod chunked_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_reply_at_the_cap_never_reallocates_the_buffer() {
        // No Content-Length and more than the cap: read until it stops.
        let mut wire = b"HTTP/1.0 200 OK\r\n\r\n".to_vec();
        wire.resize(MAX_HEAD + MAX_BODY + 3 * CHUNK, b'a');
        let (raw, clean) = read_reply(&mut &wire[..]).await.unwrap();
        assert!(clean);
        assert!(raw.len() > MAX_BODY);
        assert_eq!(
            raw.capacity(),
            Vec::<u8>::with_capacity(READ_CAPACITY).capacity()
        );
    }

    #[test]
    fn an_unclean_end_refuses_a_reply_without_a_length() {
        let cut = b"HTTP/1.0 200 OK\r\n\r\n<clientConf";
        assert_eq!(finish_reply(cut, true), Reply::Body("<clientConf".into()));
        assert_eq!(finish_reply(cut, false), Reply::Nothing);
        // A body whole by its Content-Length doesn't depend on the close.
        let whole = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhi";
        assert_eq!(finish_reply(whole, false), Reply::Body("hi".into()));
    }

    #[test]
    fn a_response_keeps_its_status_and_body() {
        let raw = b"HTTP/1.0 400 Bad Request\r\nContent-Length: 2\r\n\r\n{}";
        assert_eq!(parse_response(raw, false), Ok((400, b"{}".to_vec())));
        assert_eq!(
            parse_response(b"HTTP/1.0 503 X\r\n\r\nhi", true).unwrap().0,
            503
        );
        // Cut short without a length, no status: nothing.
        assert_eq!(
            parse_response(b"HTTP/1.0 200 OK\r\n\r\nhi", false),
            Err(BadReply::Unreadable)
        );
        assert_eq!(parse_response(b"junk", true), Err(BadReply::Unreadable));
        let big = format!("HTTP/1.0 400 X\r\n\r\n{}", "a".repeat(MAX_BODY + 1));
        assert_eq!(parse_response(big.as_bytes(), true), Err(BadReply::TooBig));
    }

    #[test]
    fn only_https_urls_parse() {
        assert_eq!(
            parse_url("https://Autoconfig.Example.org/mail/config-v1.1.xml?emailaddress=x"),
            Some(Url {
                host: "autoconfig.example.org".into(),
                port: 443,
                path: "/mail/config-v1.1.xml?emailaddress=x".into(),
            })
        );
        assert_eq!(parse_url("https://example.org:8443").unwrap().port, 8443);
        assert_eq!(parse_url("https://example.org").unwrap().path, "/");
        for bad in [
            "http://example.org/",
            "https://user@example.org/",
            "https://ex ample.org/",
            "https://example.org/a b",
        ] {
            assert_eq!(parse_url(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_request_is_a_bare_http_1_0_get() {
        let url = parse_url("https://example.org/a").unwrap();
        let text = request(&url);
        assert!(text.starts_with("GET /a HTTP/1.0\r\nHost: example.org\r\n"));
        assert!(!text.contains("User-Agent"));
        assert!(text.ends_with("\r\n\r\n"));
    }

    #[test]
    fn the_host_header_names_a_non_default_port() {
        let url = parse_url("https://localhost:8443/a").unwrap();
        assert!(request(&url).contains("\r\nHost: localhost:8443\r\n"));
    }

    #[test]
    fn replies_become_a_body_a_redirect_or_nothing() {
        assert_eq!(
            parse_reply(b"HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\n\r\n<clientConfig/>"),
            Reply::Body("<clientConfig/>".into())
        );
        assert_eq!(
            parse_reply(b"HTTP/1.1 301 Moved\r\nlocation: https://b.example/x\r\n\r\n"),
            Reply::Redirect("https://b.example/x".into())
        );
        assert_eq!(
            parse_reply(b"HTTP/1.1 404 Not Found\r\n\r\nnope"),
            Reply::Nothing
        );
        assert_eq!(parse_reply(b"HTTP/1.1 302 Found\r\n\r\n"), Reply::Nothing);
        assert_eq!(parse_reply(b"garbage"), Reply::Nothing);
        assert_eq!(
            parse_reply(b"HTTP/1.1 200 OK\r\n\r\n\xff\xfe"),
            Reply::Nothing
        );
    }

    #[test]
    fn a_chunked_reply_is_refused() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n";
        assert_eq!(parse_reply(raw), Reply::TransferEncoded);
        assert!(reply_complete(raw));
    }

    #[test]
    fn a_declared_length_ends_the_body_and_the_read() {
        let head = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n";
        let full = format!("{head}hello and then more");
        assert_eq!(parse_reply(full.as_bytes()), Reply::Body("hello".into()));
        assert!(reply_complete(full.as_bytes()));
        assert!(!reply_complete(format!("{head}hel").as_bytes()));
        // Cut short of its declared length: no file.
        assert_eq!(parse_reply(format!("{head}hel").as_bytes()), Reply::Nothing);
    }

    #[test]
    fn the_body_cap_counts_body_bytes_with_or_without_a_length() {
        let body = |n: usize| "a".repeat(n);
        let with_length =
            |n: usize| format!("HTTP/1.1 200 OK\r\nContent-Length: {n}\r\n\r\n{}", body(n));
        let without = |n: usize| format!("HTTP/1.0 200 OK\r\n\r\n{}", body(n));
        assert!(
            matches!(parse_reply(with_length(MAX_BODY).as_bytes()), Reply::Body(b) if b.len() == MAX_BODY)
        );
        assert_eq!(
            parse_reply(with_length(MAX_BODY + 1).as_bytes()),
            Reply::Nothing
        );
        assert!(matches!(
            parse_reply(without(MAX_BODY).as_bytes()),
            Reply::Body(_)
        ));
        assert_eq!(
            parse_reply(without(MAX_BODY + 1).as_bytes()),
            Reply::Nothing
        );
    }

    #[test]
    fn a_content_length_past_the_cap_is_refused_before_the_body_arrives() {
        for length in [
            "18446744073709551615".to_owned(),
            (MAX_BODY + 1).to_string(),
        ] {
            let raw = format!("HTTP/1.1 200 OK\r\nContent-Length: {length}\r\n\r\nhi");
            assert!(reply_complete(raw.as_bytes()), "{length}");
            assert_eq!(parse_reply(raw.as_bytes()), Reply::Nothing, "{length}");
        }
    }

    #[test]
    fn repeated_content_lengths_must_agree() {
        let same = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Length: 5\r\n\r\nhello";
        assert_eq!(parse_reply(same), Reply::Body("hello".into()));
        let differ = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Length: 6\r\n\r\nhello!";
        assert_eq!(parse_reply(differ), Reply::Nothing);
        assert!(reply_complete(differ));
    }

    #[test]
    fn a_length_that_is_not_digits_is_an_error() {
        for value in ["-1", "+5", "5 apples", "", "18446744073709551616"] {
            let raw = format!("HTTP/1.1 200 OK\r\nContent-Length: {value}\r\n\r\nhello");
            assert_eq!(parse_reply(raw.as_bytes()), Reply::Nothing, "{value:?}");
            assert!(reply_complete(raw.as_bytes()), "{value:?}");
        }
    }

    #[test]
    fn redirects_may_not_use_hex_or_short_ipv4_forms() {
        for refused in ["0x7f000001", "0x7f.0x1", "2130706433", "127.1"] {
            assert!(!allowed_redirect_host(refused), "{refused}");
        }
        assert!(allowed_redirect_host("autoconfig.example.org"));
    }

    #[test]
    fn redirects_may_not_point_at_localhost_or_an_address() {
        for refused in [
            "localhost",
            "LOCALHOST",
            "localhost.",
            "app.localhost",
            "127.0.0.1",
            "10.0.0.5",
            "::1",
            "2130706433",
        ] {
            assert!(!allowed_redirect_host(refused), "{refused}");
        }
        for allowed in ["autoconfig.example.org", "mail.example.org.", "example.org"] {
            assert!(allowed_redirect_host(allowed), "{allowed}");
        }
    }
}
