//! The plaintext prelude of SMTP STARTTLS (RFC 3207), done by envio rather
//! than lettre so the handshake uses envio's pinning verifier: greeting,
//! EHLO, STARTTLS, then TLS. Nothing but these three exchanges is ever
//! sent in the clear, and a missing STARTTLS is an error, never a
//! plaintext fallback.

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use super::EHLO_NAME;
use super::reply::{self, MAX_LINE, MAX_LINES, Reply};
use crate::connect;
use crate::error::{NetError, Result};
use crate::server::Endpoint;
use crate::tls::{self, Roots, TlsStream};

pub(crate) async fn upgrade(
    tcp: TcpStream,
    endpoint: &Endpoint,
    roots: &Roots,
) -> Result<TlsStream<TcpStream>> {
    let mut io = BufReader::new(tcp);
    connect::within(prelude(&mut io)).await?;
    // Bytes the server sent after its 220 would be read as if they came
    // over TLS (the STARTTLS injection attack); refuse instead.
    if !io.buffer().is_empty() {
        return Err(NetError::Protocol("data after the STARTTLS reply".into()));
    }
    tls::handshake(io.into_inner(), &endpoint.host, roots, endpoint.pin).await
}

async fn prelude<S>(io: &mut BufReader<S>) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let greeting = read_reply(io).await?;
    if greeting.code != 220 {
        return Err(reply::error_for(
            greeting.code,
            greeting.lines.join(" "),
            false,
        ));
    }
    io.write_all(format!("EHLO {EHLO_NAME}\r\n").as_bytes())
        .await?;
    let ehlo = read_reply(io).await?;
    if ehlo.code != 250 {
        return Err(reply::error_for(ehlo.code, ehlo.lines.join(" "), false));
    }
    if !reply::has_starttls(&ehlo) {
        return Err(NetError::NoStartTls);
    }
    io.write_all(b"STARTTLS\r\n").await?;
    let ready = read_reply(io).await?;
    match ready.code {
        220 => Ok(()),
        400..=499 => Err(reply::error_for(ready.code, ready.lines.join(" "), false)),
        _ => Err(NetError::NoStartTls),
    }
}

/// Reads one reply, possibly over several lines.
pub(crate) async fn read_reply<S>(io: &mut BufReader<S>) -> Result<Reply>
where
    S: AsyncRead + Unpin,
{
    let mut lines = Vec::new();
    loop {
        let mut raw = Vec::new();
        let n = (&mut *io)
            .take(MAX_LINE as u64)
            .read_until(b'\n', &mut raw)
            .await?;
        if n == 0 {
            return Err(NetError::Io(std::io::ErrorKind::UnexpectedEof.into()));
        }
        if raw.last() != Some(&b'\n') {
            // Short of the limit, the read only stops at end of stream: the
            // server closed in the middle of a line. A full-length line is
            // a protocol violation.
            if raw.len() < MAX_LINE {
                return Err(NetError::Io(std::io::ErrorKind::UnexpectedEof.into()));
            }
            return Err(NetError::Protocol("SMTP reply line too long".into()));
        }
        let text = String::from_utf8_lossy(&raw);
        let (code, more, rest) = reply::parse_line(&text)
            .ok_or_else(|| NetError::Protocol(format!("not an SMTP reply: {text:?}")))?;
        lines.push(rest.to_owned());
        if !more {
            return Ok(Reply { code, lines });
        }
        if lines.len() >= MAX_LINES {
            return Err(NetError::Protocol("SMTP reply too long".into()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn read(bytes: &[u8]) -> Result<Reply> {
        let mut io = BufReader::new(bytes);
        read_reply(&mut io).await
    }

    #[tokio::test]
    async fn a_multi_line_reply_is_read_whole() {
        let reply = read(b"250-mx.example\r\n250-PIPELINING\r\n250 STARTTLS\r\n")
            .await
            .unwrap();
        assert_eq!(reply.code, 250);
        assert_eq!(reply.lines, ["mx.example", "PIPELINING", "STARTTLS"]);
    }

    #[tokio::test]
    async fn an_endless_line_is_refused() {
        let long = [b'2'; MAX_LINE + 10];
        assert!(matches!(read(&long).await, Err(NetError::Protocol(_))));
    }

    #[tokio::test]
    async fn a_cut_reply_is_an_io_error() {
        assert!(matches!(read(b"250-mx\r\n").await, Err(NetError::Io(_))));
    }
}
