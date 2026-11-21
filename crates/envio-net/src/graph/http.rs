//! HTTP/1.1 over `tls::connect` with keep-alive, for Graph. One connection is
//! kept between requests; whether a request may use it, retry on a fresh one
//! or must report "maybe sent" is decided here (see [`Sent`]).

use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

use super::reply::{self, FailKind};
use crate::COMMAND_TIMEOUT;
use crate::connect;
use crate::error::NetError;
use crate::server::{Endpoint, Security};
use crate::tls::{self, Roots, TlsStream};

/// A reply body over this is a `Protocol` error (`Refused` for `$value`).
pub const MAX_JSON_BODY: usize = 4 * 1024 * 1024;
/// The cap for a message's MIME (`.../$value`).
pub const MAX_VALUE_BODY: usize = 64 * 1024 * 1024;
const PREFER_IDS: &str = "IdType=\"ImmutableId\"";

pub struct Request<'a> {
    pub method: &'static str,
    /// Path and query, already percent-encoded; no spaces or control bytes.
    pub path: &'a str,
    pub headers: Vec<(&'static str, String)>,
    /// Content type and bytes.
    pub body: Option<(&'static str, Vec<u8>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    /// The first header called `name`, any case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// A failed request, by what the server can have seen.
#[derive(Debug)]
pub enum Sent {
    /// Nothing was written, or the request failed with no reply byte on a
    /// reused connection (it was probably closed idle): the server did not
    /// act, as far as we know.
    Before(NetError),
    /// The request was written and the reply was lost: it may have been
    /// acted on.
    After(NetError),
}

impl Sent {
    pub fn into_error(self) -> NetError {
        match self {
            Self::Before(e) | Self::After(e) => e,
        }
    }
}

pub struct Http {
    endpoint: Endpoint,
    roots: Roots,
    conn: Option<TlsStream<TcpStream>>,
    read_timeout: Duration,
}

/// How an attempt on one connection failed.
struct Failed {
    error: NetError,
    /// The request was fully written.
    written: bool,
    got_byte: bool,
}

impl Http {
    pub fn new(host: &str, port: u16, roots: Roots) -> Self {
        Self {
            endpoint: Endpoint::new(host, port, Security::Tls),
            roots,
            conn: None,
            read_timeout: COMMAND_TIMEOUT,
        }
    }

    /// Whether a connection is being kept for the next request.
    pub fn is_connected(&self) -> bool {
        self.conn.is_some()
    }

    /// Drops the kept connection (before a rebuild, say).
    pub fn close(&mut self) {
        self.conn = None;
    }

    /// Sends `req` and reads the reply. `fresh` always opens a new
    /// connection. On a reused one, a failure before any reply byte retries
    /// once on a new connection if `idempotent`, else is `Sent::Before`.
    pub async fn send(
        &mut self,
        req: &Request<'_>,
        fresh: bool,
        idempotent: bool,
    ) -> Result<Response, Sent> {
        let bytes = encode(&self.endpoint, req).map_err(Sent::Before)?;
        let cap = if req.path.ends_with("/$value") {
            MAX_VALUE_BODY
        } else {
            MAX_JSON_BODY
        };
        let write_limit = connect::transfer_timeout(req.body.as_ref().map_or(0, |(_, b)| b.len()));
        let mut reused = !fresh && self.conn.is_some();
        loop {
            if !reused {
                self.conn = None;
                let conn = tls::connect(&self.endpoint, &self.roots)
                    .await
                    .map_err(|e| Sent::Before(tls_error(e)))?;
                self.conn = Some(conn);
            }
            match self.attempt(&bytes, cap, write_limit).await {
                Ok(response) => return Ok(response),
                Err(f) => {
                    self.conn = None;
                    if reused && !f.got_byte {
                        // Probably closed while idle.
                        if idempotent {
                            reused = false;
                            continue;
                        }
                        return Err(Sent::Before(f.error));
                    }
                    return Err(if f.written || f.got_byte {
                        Sent::After(f.error)
                    } else {
                        Sent::Before(f.error)
                    });
                }
            }
        }
    }

    async fn attempt(
        &mut self,
        bytes: &[u8],
        cap: usize,
        write_limit: Duration,
    ) -> Result<Response, Failed> {
        let conn = self.conn.as_mut().expect("connected");
        let wrote = tokio::time::timeout(write_limit, async {
            conn.write_all(bytes).await?;
            conn.flush().await
        })
        .await;
        let early = |error| Failed {
            error,
            written: false,
            got_byte: false,
        };
        match wrote {
            Err(_) => return Err(early(NetError::Timeout)),
            Ok(Err(e)) => return Err(early(NetError::Io(e))),
            Ok(Ok(())) => {}
        }
        match reply::read_reply(conn, cap, self.read_timeout).await {
            Ok(read) => {
                if read.close {
                    self.conn = None;
                }
                Ok(read.response)
            }
            Err(fail) => Err(Failed {
                error: match fail.kind {
                    FailKind::Net(e) => e,
                    FailKind::TooBig if cap == MAX_VALUE_BODY => {
                        NetError::Refused("message too large".into())
                    }
                    FailKind::TooBig => NetError::Protocol("reply too large".into()),
                },
                written: true,
                got_byte: fail.got_byte,
            }),
        }
    }

    /// Test hook: a shorter wait for each reply read.
    #[doc(hidden)]
    pub fn set_read_timeout(&mut self, limit: Duration) {
        self.read_timeout = limit;
    }
}

/// `Untrusted` and `PinChanged` have no accept action on Graph.
fn tls_error(e: NetError) -> NetError {
    match e {
        NetError::Untrusted { reason, .. } => NetError::Tls(format!("untrusted: {reason:?}")),
        NetError::PinChanged { .. } => NetError::Tls("certificate changed".into()),
        other => other,
    }
}

fn clean(s: &str) -> bool {
    !s.bytes().any(|b| b < 0x20 || b == 0x7f)
}

/// The request as bytes. Every request carries `Host`, `Prefer` (the
/// ImmutableId preference first, more joined with commas), `Connection:
/// keep-alive` and `Accept`.
fn encode(endpoint: &Endpoint, req: &Request<'_>) -> Result<Vec<u8>, NetError> {
    let bad = |what: &str| NetError::Protocol(format!("bad request {what}"));
    if req.path.is_empty()
        || !req.path.starts_with('/')
        || req.path.contains(' ')
        || !clean(req.path)
    {
        return Err(bad("path"));
    }
    let host = if endpoint.port == 443 {
        endpoint.host.clone()
    } else {
        format!("{}:{}", endpoint.host, endpoint.port)
    };
    let mut prefer = vec![PREFER_IDS.to_owned()];
    let mut head = format!("{} {} HTTP/1.1\r\nHost: {host}\r\n", req.method, req.path);
    for (name, value) in &req.headers {
        if !clean(value) || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return Err(bad("header"));
        }
        if name.eq_ignore_ascii_case("prefer") {
            prefer.push(value.clone());
        } else {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
    }
    head.push_str(&format!("Prefer: {}\r\n", prefer.join(", ")));
    head.push_str("Connection: keep-alive\r\nAccept: application/json\r\n");
    let mut out;
    match &req.body {
        Some((ctype, body)) => {
            head.push_str(&format!(
                "Content-Type: {ctype}\r\nContent-Length: {}\r\n\r\n",
                body.len()
            ));
            out = head.into_bytes();
            out.extend_from_slice(body);
        }
        None if req.method == "GET" => {
            head.push_str("\r\n");
            out = head.into_bytes();
        }
        None => {
            head.push_str("Content-Length: 0\r\n\r\n");
            out = head.into_bytes();
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(port: u16) -> Endpoint {
        Endpoint::new("graph.example.org", port, Security::Tls)
    }

    #[test]
    fn prefer_values_share_one_header_with_immutable_ids_first() {
        let req = Request {
            method: "GET",
            path: "/v1.0/me",
            headers: vec![("Prefer", "odata.maxpagesize=200".into())],
            body: None,
        };
        let text = String::from_utf8(encode(&endpoint(443), &req).unwrap()).unwrap();
        assert!(
            text.contains("Prefer: IdType=\"ImmutableId\", odata.maxpagesize=200\r\n"),
            "{text}"
        );
        assert_eq!(text.matches("Prefer:").count(), 1);
        assert!(text.contains("Host: graph.example.org\r\n"));
        assert!(text.contains("Connection: keep-alive\r\n"));
    }

    #[test]
    fn injection_and_odd_paths_are_refused_before_anything_is_written() {
        let mut req = Request {
            method: "GET",
            path: "/v1.0/me\r\nX: y",
            headers: vec![],
            body: None,
        };
        assert!(encode(&endpoint(443), &req).is_err());
        req.path = "/a b";
        assert!(encode(&endpoint(443), &req).is_err());
        req.path = "/ok";
        req.headers = vec![("X-A", "v\r\nEvil: 1".into())];
        assert!(encode(&endpoint(443), &req).is_err());
    }

    #[test]
    fn a_body_gets_its_length_and_a_bodyless_post_a_zero_length() {
        let mut req = Request {
            method: "POST",
            path: "/x",
            headers: vec![],
            body: Some(("text/plain", b"abc".to_vec())),
        };
        let text = String::from_utf8(encode(&endpoint(8443), &req).unwrap()).unwrap();
        assert!(text.contains("Host: graph.example.org:8443\r\n"));
        assert!(text.ends_with("Content-Length: 3\r\n\r\nabc"), "{text}");
        req.body = None;
        let text = String::from_utf8(encode(&endpoint(443), &req).unwrap()).unwrap();
        assert!(text.ends_with("Content-Length: 0\r\n\r\n"));
    }
}
