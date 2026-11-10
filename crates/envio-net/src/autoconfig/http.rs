//! Just enough HTTPS to fetch an autoconfig file: a GET over HTTP/1.0 with
//! no cookies and no User-Agent. This file is the pure part: URLs, the
//! request, how a reply is framed and read, and which redirect hosts are
//! allowed. The network side is in `fetch`.

use std::net::IpAddr;

/// Autoconfig files are a few KiB; a body over this is not one.
pub const MAX_BODY: usize = 256 * 1024;
/// The status line and headers, which are not counted in `MAX_BODY`.
pub const MAX_HEAD: usize = 16 * 1024;
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
    /// `Transfer-Encoding: chunked`: not a framing this client reads.
    Chunked,
    /// Any other status, or a body that is too big or cut short: there is
    /// no file here.
    Nothing,
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
    (super::plan::is_hostname(host) && ok_path).then(|| Url {
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
            head.chunked |= value.to_ascii_lowercase().contains("chunked");
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
    head.chunked
        || match head.length {
            Length::None => false,
            Length::Invalid => true,
            Length::Known(n) => {
                n > MAX_BODY
                    || head
                        .body_start
                        .checked_add(n)
                        .is_some_and(|end| raw.len() >= end)
            }
        }
}

/// The reply in `raw` (the bytes read so far, up to its end).
pub fn parse_reply(raw: &[u8]) -> Reply {
    let Some(head) = head(raw) else {
        return Reply::Nothing;
    };
    match head.status {
        200 if head.chunked => Reply::Chunked,
        200 => {
            let rest = &raw[head.body_start..];
            let body = match head.length {
                Length::Invalid => return Reply::Nothing,
                Length::Known(n) if n > MAX_BODY || rest.len() < n => return Reply::Nothing,
                Length::Known(n) => &rest[..n],
                Length::None if rest.len() > MAX_BODY => return Reply::Nothing,
                Length::None => rest,
            };
            String::from_utf8(body.to_vec()).map_or(Reply::Nothing, Reply::Body)
        }
        301 | 302 | 303 | 307 | 308 => head.location.map_or(Reply::Nothing, Reply::Redirect),
        _ => Reply::Nothing,
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(parse_reply(raw), Reply::Chunked);
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
