//! The TLS stream handed to lettre. lettre always starts by reading a
//! greeting; after STARTTLS the real one was read in the clear, so this
//! stream first plays back a stand-in 220, then passes everything through.

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

use lettre::transport::smtp::client::AsyncTokioStream;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;

use crate::tls::TlsStream;

const STAND_IN: &[u8] = b"220 envio STARTTLS done\r\n";

#[derive(Debug)]
pub(crate) struct SmtpStream {
    tls: TlsStream<TcpStream>,
    /// Bytes of the stand-in greeting still to play back.
    replay: &'static [u8],
}

impl SmtpStream {
    /// After implicit TLS: the server's own greeting comes next.
    pub fn implicit(tls: TlsStream<TcpStream>) -> Self {
        Self { tls, replay: &[] }
    }

    /// After STARTTLS: lettre gets the stand-in greeting first.
    pub fn upgraded(tls: TlsStream<TcpStream>) -> Self {
        Self {
            tls,
            replay: STAND_IN,
        }
    }
}

impl AsyncRead for SmtpStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if !self.replay.is_empty() {
            let n = self.replay.len().min(buf.remaining());
            buf.put_slice(&self.replay[..n]);
            self.replay = &self.replay[n..];
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut self.tls).poll_read(cx, buf)
    }
}

impl AsyncWrite for SmtpStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.tls).poll_write(cx, data)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.tls).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.tls).poll_shutdown(cx)
    }
}

impl AsyncTokioStream for SmtpStream {
    fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.tls.get_ref().0.peer_addr()
    }
}
