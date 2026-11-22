//! An incremental HTTP/1.1 reply reader: the status line and headers, then
//! exactly the body the framing says (Content-Length, chunked, or to EOF),
//! leaving the connection at the next reply. `crate::http` decodes a buffered
//! body read to EOF; a keep-alive connection cannot do that.

use std::io;
use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, AsyncReadExt};

use crate::error::NetError;
use crate::http::{MAX_HEAD, SizeError, chunk_size};

use super::http::Response;

/// The longest chunk-size line (size and extensions) read.
const MAX_CHUNK_LINE: usize = 1024;
/// The slowest link a sized body is still waited for: the reply's deadline
/// grows with its announced length at this rate.
const MIN_RATE: u64 = 16 * 1024;

/// The time allowed for a reply announcing `len` body bytes: `base`, or
/// `len` at `MIN_RATE` if that is longer.
pub(super) fn allowed(base: Duration, len: usize) -> Duration {
    base.max(Duration::from_secs(len as u64 / MIN_RATE))
}

/// Interim (1xx) replies skipped before the reply is called broken.
const MAX_INTERIM: usize = 8;

/// Why a reply could not be read, and whether any byte of it had arrived
/// (a reused connection that fails with none was probably closed idle).
#[derive(Debug)]
pub(super) struct Fail {
    pub got_byte: bool,
    pub kind: FailKind,
}

#[derive(Debug)]
pub(super) enum FailKind {
    Net(NetError),
    /// The body is over the cap.
    TooBig,
}

/// A read reply, and whether the connection must not be reused.
pub(super) struct Read {
    pub response: Response,
    pub close: bool,
}

struct Buffered<'a, S> {
    stream: &'a mut S,
    buf: Vec<u8>,
    pos: usize,
    got: bool,
    timeout: Duration,
    /// When the whole reply is due, however steadily it trickles in.
    due: Instant,
}

impl<S: AsyncRead + Unpin> Buffered<'_, S> {
    fn fail(&self, error: NetError) -> Fail {
        Fail {
            got_byte: self.got,
            kind: FailKind::Net(error),
        }
    }

    fn protocol(&self, text: &str) -> Fail {
        self.fail(NetError::Protocol(text.to_owned()))
    }

    /// Reads more bytes; EOF is an error (callers that accept it use
    /// `fill_or_eof`).
    async fn fill(&mut self) -> Result<(), Fail> {
        if self.fill_or_eof().await? == 0 {
            return Err(self.fail(NetError::Io(io::ErrorKind::UnexpectedEof.into())));
        }
        Ok(())
    }

    async fn fill_or_eof(&mut self) -> Result<usize, Fail> {
        let mut chunk = [0u8; 16 * 1024];
        let left = self.due.saturating_duration_since(Instant::now());
        let limit = self.timeout.min(left);
        let n = match tokio::time::timeout(limit, self.stream.read(&mut chunk)).await {
            Err(_) => return Err(self.fail(NetError::Timeout)),
            Ok(Err(e)) => return Err(self.fail(NetError::Io(e))),
            Ok(Ok(n)) => n,
        };
        if n > 0 {
            self.got = true;
            self.buf.extend_from_slice(&chunk[..n]);
        }
        Ok(n)
    }

    /// The bytes up to the next CRLF (not included), at most `limit` long.
    async fn line(&mut self, limit: usize) -> Result<Vec<u8>, Fail> {
        loop {
            let rest = &self.buf[self.pos..];
            if let Some(i) = rest.windows(2).position(|w| w == b"\r\n") {
                let line = rest[..i].to_vec();
                self.pos += i + 2;
                return Ok(line);
            }
            if rest.len() > limit + 1 {
                return Err(self.protocol("reply line too long"));
            }
            self.fill().await?;
        }
    }

    /// Exactly `n` bytes, moved out of the buffer (no second copy of a big
    /// body); what follows them stays buffered.
    async fn take_body(&mut self, n: usize) -> Result<Vec<u8>, Fail> {
        while self.buf.len() - self.pos < n {
            self.fill().await?;
        }
        let rest = self.buf.split_off(self.pos + n);
        let mut body = std::mem::replace(&mut self.buf, rest);
        body.drain(..self.pos);
        self.pos = 0;
        Ok(body)
    }

    /// Exactly `n` bytes.
    async fn take(&mut self, n: usize) -> Result<Vec<u8>, Fail> {
        while self.buf.len() - self.pos < n {
            self.fill().await?;
        }
        let out = self.buf[self.pos..self.pos + n].to_vec();
        self.pos += n;
        Ok(out)
    }
}

/// How the body is framed.
enum Framing {
    None,
    Length(usize),
    Chunked,
    ToEof,
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}
/// The status and headers of a head (no final blank line).
type Head = (u16, bool, Vec<(String, String)>);

fn parse_head(raw: &[u8]) -> Option<Head> {
    let text = std::str::from_utf8(raw).ok()?;
    let mut lines = text.split("\r\n");
    let status_line = lines.next()?;
    let mut parts = status_line.splitn(3, ' ');
    let version = parts.next()?;
    let http10 = match version {
        "HTTP/1.1" => false,
        "HTTP/1.0" => true,
        _ => return None,
    };
    let status: u16 = parts.next()?.parse().ok()?;
    let mut headers = Vec::new();
    for line in lines {
        let (name, value) = line.split_once(':')?;
        if name.is_empty() || name.contains(|c: char| c.is_whitespace()) {
            return None;
        }
        headers.push((name.to_owned(), value.trim().to_owned()));
    }
    Some((status, http10, headers))
}

fn framing(status: u16, headers: &[(String, String)]) -> Result<Framing, &'static str> {
    if (100..200).contains(&status) || status == 204 || status == 304 {
        return Ok(Framing::None);
    }
    if let Some(te) = header(headers, "transfer-encoding") {
        return if te.trim().eq_ignore_ascii_case("chunked") {
            Ok(Framing::Chunked)
        } else {
            Err("unsupported transfer encoding")
        };
    }
    match header(headers, "content-length") {
        Some(v) => v
            .trim()
            .parse()
            .map(Framing::Length)
            .map_err(|_| "bad Content-Length"),
        None => Ok(Framing::ToEof),
    }
}

/// Reads one reply from `stream`. `cap` bounds the body; each read waits at
/// most `timeout` and the whole reply at most `total`. Up to `MAX_INTERIM`
/// interim 1xx replies are skipped.
pub(super) async fn read_reply<S: AsyncRead + Unpin>(
    stream: &mut S,
    cap: usize,
    timeout: Duration,
    total: Duration,
) -> Result<Read, Fail> {
    let mut r = Buffered {
        stream,
        buf: Vec::new(),
        pos: 0,
        got: false,
        timeout,
        due: Instant::now() + total,
    };
    let started = Instant::now();
    let mut interim = 0;
    let (status, http10, headers) = loop {
        let start = r.pos;
        let head_end = loop {
            if let Some(i) = r.buf[start..].windows(4).position(|w| w == b"\r\n\r\n") {
                break start + i;
            }
            if r.buf.len() - start > MAX_HEAD {
                return Err(r.protocol("reply head too long"));
            }
            r.fill().await?;
        };
        let Some(parsed) = parse_head(&r.buf[start..head_end]) else {
            return Err(r.protocol("malformed reply head"));
        };
        r.pos = head_end + 4;
        if !(100..200).contains(&parsed.0) || parsed.0 == 101 {
            break parsed;
        }
        interim += 1;
        if interim > MAX_INTERIM {
            return Err(r.protocol("too many interim replies"));
        }
        // Drop what was read for it, so endless interim replies cannot grow
        // the buffer.
        r.buf.drain(..r.pos);
        r.pos = 0;
    };
    let mut close = http10
        || header(&headers, "connection")
            .is_some_and(|v| v.split(',').any(|t| t.trim().eq_ignore_ascii_case("close")));
    let body = match framing(status, &headers).map_err(|e| r.protocol(e))? {
        Framing::None => Vec::new(),
        Framing::Length(n) if n > cap => return Err(too_big(&r)),
        Framing::Length(n) => {
            r.due = started + allowed(total, n);
            r.take_body(n).await?
        }
        Framing::Chunked => chunked(&mut r, cap).await?,
        Framing::ToEof => {
            close = true;
            loop {
                if r.buf.len() - r.pos > cap {
                    return Err(too_big(&r));
                }
                if r.fill_or_eof().await? == 0 {
                    break;
                }
            }
            r.buf.drain(..r.pos);
            r.pos = 0;
            std::mem::take(&mut r.buf)
        }
    };
    // Bytes beyond the reply are not ours to guess at.
    if matches!(
        framing(status, &headers),
        Ok(Framing::Length(_) | Framing::Chunked)
    ) && r.pos < r.buf.len()
    {
        close = true;
    }
    Ok(Read {
        response: Response {
            status,
            headers,
            body,
        },
        close,
    })
}

fn too_big<S>(r: &Buffered<'_, S>) -> Fail {
    Fail {
        got_byte: r.got,
        kind: FailKind::TooBig,
    }
}

async fn chunked<S: AsyncRead + Unpin>(
    r: &mut Buffered<'_, S>,
    cap: usize,
) -> Result<Vec<u8>, Fail> {
    let mut body = Vec::new();
    loop {
        let line = r.line(MAX_CHUNK_LINE).await?;
        let size = match chunk_size(&line) {
            Ok(size) => size,
            Err(SizeError::Overflow) => return Err(too_big(r)),
            Err(SizeError::Malformed) => return Err(r.protocol("bad chunk size")),
        };
        if size == 0 {
            // Trailers, ignored, up to the blank line.
            let mut seen = 0;
            loop {
                let trailer = r.line(MAX_HEAD).await?;
                if trailer.is_empty() {
                    return Ok(body);
                }
                seen += trailer.len();
                if seen > MAX_HEAD {
                    return Err(r.protocol("trailers too long"));
                }
            }
        }
        if size > cap || body.len() + size > cap {
            return Err(too_big(r));
        }
        body.extend(r.take(size).await?);
        if r.take(2).await? != b"\r\n" {
            return Err(r.protocol("chunk not followed by CRLF"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Duration = Duration::from_secs(5);

    async fn read(bytes: &[u8], cap: usize) -> Result<Read, Fail> {
        let mut cursor = bytes;
        read_reply(&mut cursor, cap, T, T).await
    }

    #[test]
    fn a_big_announced_body_gets_a_longer_deadline() {
        let base = Duration::from_secs(600);
        assert_eq!(allowed(base, 1024), base);
        // 64 MiB at 16 KiB/s is 4096 s.
        assert_eq!(allowed(base, 64 * 1024 * 1024), Duration::from_secs(4096));
    }

    #[tokio::test]
    async fn endless_interim_replies_are_an_error() {
        let mut raw = Vec::new();
        for _ in 0..50 {
            raw.extend_from_slice(b"HTTP/1.1 100 Continue\r\n\r\n");
        }
        raw.extend_from_slice(b"HTTP/1.1 204 No Content\r\n\r\n");
        let e = read(&raw, 9).await.err().unwrap();
        assert!(
            matches!(&e.kind, FailKind::Net(NetError::Protocol(m)) if m.contains("interim")),
            "{e:?}"
        );
    }

    #[tokio::test]
    async fn a_reply_that_trickles_in_past_the_deadline_times_out() {
        use tokio::io::AsyncWriteExt;
        let (mut client, mut server) = tokio::io::duplex(64);
        let feeder = tokio::spawn(async move {
            for b in b"HTTP/1.1 200 OK\r\nContent-Length: 400\r\n\r\n"
                .iter()
                .chain(&[b'x'; 400])
            {
                if server.write_all(&[*b]).await.is_err() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
        let started = Instant::now();
        // Each read is well inside its 5 s limit; the whole is due in 300 ms.
        let e = read_reply(&mut client, 1000, T, Duration::from_millis(300))
            .await
            .err()
            .unwrap();
        assert!(matches!(e.kind, FailKind::Net(NetError::Timeout)), "{e:?}");
        assert!(started.elapsed() < Duration::from_secs(2));
        feeder.abort();
    }

    #[tokio::test]
    async fn content_length_stops_at_the_length() {
        let r = read(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabc", 100)
            .await
            .unwrap();
        assert_eq!(
            (r.response.status, r.response.body, r.close),
            (200, b"abc".to_vec(), false)
        );
    }

    #[tokio::test]
    async fn chunked_joins_the_chunks_and_skips_trailers() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2;x=1\r\nde\r\n0\r\nT: v\r\n\r\n";
        let r = read(raw, 100).await.unwrap();
        assert_eq!(r.response.body, b"abcde");
        assert!(!r.close);
    }

    #[tokio::test]
    async fn connection_close_and_eof_bodies_close() {
        let r = read(
            b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 1\r\n\r\nx",
            9,
        )
        .await
        .unwrap();
        assert!(r.close);
        let r = read(b"HTTP/1.1 200 OK\r\n\r\nrest", 9).await.unwrap();
        assert_eq!((r.response.body, r.close), (b"rest".to_vec(), true));
    }

    #[tokio::test]
    async fn no_body_statuses_and_interim_replies() {
        let r = read(
            b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 204 No Content\r\n\r\n",
            9,
        )
        .await
        .unwrap();
        assert_eq!((r.response.status, r.response.body.len()), (204, 0));
    }

    #[tokio::test]
    async fn over_the_cap_and_bad_framing() {
        let big = read(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n", 9).await;
        assert!(matches!(
            big,
            Err(Fail {
                kind: FailKind::TooBig,
                ..
            })
        ));
        let big = read(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nA\r\n",
            9,
        )
        .await;
        assert!(matches!(
            big,
            Err(Fail {
                kind: FailKind::TooBig,
                ..
            })
        ));
        let bad = read(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\n\r\n", 9).await;
        assert!(matches!(
            bad,
            Err(Fail {
                kind: FailKind::Net(NetError::Protocol(_)),
                ..
            })
        ));
        let cut = read(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nab", 9).await;
        assert!(matches!(
            cut,
            Err(Fail {
                got_byte: true,
                kind: FailKind::Net(NetError::Io(_))
            })
        ));
        let none = read(b"", 9).await;
        assert!(matches!(
            none,
            Err(Fail {
                got_byte: false,
                ..
            })
        ));
    }
}
